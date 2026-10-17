//! In-process test harness for the axum HTTP layer.
//!
//! `TestServer` builds the real router against an in-memory comment store and
//! a mock site, then routes individual requests through `tower::ServiceExt::oneshot`
//! so tests exercise the full middleware + handler stack without binding a
//! TCP socket.
//!
//! Test-only — gated under `#[cfg(test)]` so it never ships in release builds.

use std::sync::Arc;

use axum::Router;
use axum::body::{self, Body};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use http_body_util::BodyExt;
use rw_cache::NullCache;
use rw_comments::SqliteCommentStore;
use rw_site::{PageRendererConfig, Site};
use rw_storage::MockStorage;
use serde_json::Value;
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

use crate::app;
use crate::live_reload::{LiveReloadManager, ReloadEvent};
use crate::state::AppState;

/// Test-only HTTP harness: wraps the production router around an in-memory
/// comment store and a `MockStorage`-backed site (empty via
/// [`new`](Self::new), or populated via [`with_storage`](Self::with_storage)).
/// Each call routes a single request through the full axum stack via `oneshot`.
pub(crate) struct TestServer {
    router: Router,
    reload_tx: Option<broadcast::Sender<ReloadEvent>>,
}

impl TestServer {
    /// Fixed notify token used by the in-process harness so endpoint tests can
    /// present a matching `X-RW-Token` header.
    pub(crate) const TEST_NOTIFY_TOKEN: &str = "test-notify-token";

    /// Build a server backed by an empty `MockStorage`.
    pub(crate) async fn new() -> Self {
        Self::with_storage(MockStorage::new()).await
    }

    /// Build a server whose `Site` is backed by the given populated
    /// `MockStorage`, for exercising the page-rendering handlers.
    pub(crate) async fn with_storage(storage: MockStorage) -> Self {
        Self::build(storage, Some(Self::TEST_NOTIFY_TOKEN.to_owned()), None).await
    }

    /// Build a server with no configured notify token, for testing the
    /// "feature disabled" path.
    pub(crate) async fn without_notify_token() -> Self {
        Self::build(MockStorage::new(), None, None).await
    }

    /// Build a server with live reload enabled (no file watcher started), so
    /// tests can subscribe to broadcast events.
    pub(crate) async fn with_live_reload() -> Self {
        let (tx, _rx) = broadcast::channel::<ReloadEvent>(16);
        Self::build(
            MockStorage::new(),
            Some(Self::TEST_NOTIFY_TOKEN.to_owned()),
            Some(tx),
        )
        .await
    }

    async fn build(
        storage: MockStorage,
        notify_token: Option<String>,
        reload_tx: Option<broadcast::Sender<ReloadEvent>>,
    ) -> Self {
        let site = Arc::new(Site::new(
            Arc::new(storage),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        ));
        let live_reload = reload_tx
            .as_ref()
            .map(|tx| LiveReloadManager::new(Arc::clone(&site), tx.clone()));
        let comment_store = Arc::new(SqliteCommentStore::open_memory().await.unwrap());

        let state = Arc::new(AppState {
            site,
            live_reload,
            verbose: false,
            comment_store,
            notify_token,
            embedded_preview: false,
        });

        Self {
            router: app::create_router(state),
            reload_tx,
        }
    }

    /// Subscribe to broadcast reload events. Panics if live reload is off.
    pub(crate) fn subscribe_reload(&self) -> broadcast::Receiver<ReloadEvent> {
        self.reload_tx
            .as_ref()
            .expect("TestServer built without live reload")
            .subscribe()
    }

    /// Send `method path`. A `Some` body is sent as JSON with a matching
    /// `content-type`; `None` sends an empty body.
    pub(crate) async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> TestResponse {
        let mut builder = Request::builder().method(method).uri(path);
        for &(name, value) in headers {
            builder = builder.header(name, value);
        }
        let body = match body {
            Some(json) => {
                builder = builder.header("content-type", "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        let response = self
            .router
            .clone()
            .oneshot(builder.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        TestResponse {
            status,
            headers,
            bytes,
        }
    }

    pub(crate) async fn get(&self, path: &str) -> TestResponse {
        self.request(Method::GET, path, None, &[]).await
    }

    pub(crate) async fn delete(&self, path: &str) -> TestResponse {
        self.request(Method::DELETE, path, None, &[]).await
    }

    pub(crate) async fn post_json(&self, path: &str, body: Value) -> TestResponse {
        self.request(Method::POST, path, Some(body), &[]).await
    }

    pub(crate) async fn patch_json(&self, path: &str, body: Value) -> TestResponse {
        self.request(Method::PATCH, path, Some(body), &[]).await
    }

    /// Convenience helper: create a top-level comment via `POST /_api/comments`
    /// and return the parsed JSON. Panics on non-201.
    pub(crate) async fn create_comment(&self, document_id: &str, body: &str) -> Value {
        let resp = self
            .post_json(
                "/_api/comments",
                serde_json::json!({
                    "documentId": document_id,
                    "body": body,
                }),
            )
            .await;
        assert_eq!(
            resp.status,
            StatusCode::CREATED,
            "create_comment failed: {} {}",
            resp.status,
            resp.text()
        );
        resp.json()
    }

    /// Convenience helper: create a reply to `parent_id` via the same endpoint.
    pub(crate) async fn create_reply(
        &self,
        document_id: &str,
        parent_id: Uuid,
        body: &str,
    ) -> Value {
        let resp = self
            .post_json(
                "/_api/comments",
                serde_json::json!({
                    "documentId": document_id,
                    "parentId": parent_id.to_string(),
                    "body": body,
                }),
            )
            .await;
        assert_eq!(
            resp.status,
            StatusCode::CREATED,
            "create_reply failed: {} {}",
            resp.status,
            resp.text()
        );
        resp.json()
    }
}

/// Captured HTTP response — status, headers, plus already-collected body bytes.
pub(crate) struct TestResponse {
    pub(crate) status: StatusCode,
    headers: HeaderMap,
    bytes: body::Bytes,
}

impl TestResponse {
    /// Value of a response header as a string, or `None` if absent. Panics if
    /// the header value is not valid UTF-8 (never true for the headers tests
    /// assert on here).
    pub(crate) fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap().to_owned())
    }

    /// Parse the body as JSON. Panics on parse failure.
    pub(crate) fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or_else(|e| {
            panic!(
                "failed to parse response as JSON: {e}; body = {}",
                self.text()
            )
        })
    }

    /// Body as a lossy UTF-8 string (for diagnostics).
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}
