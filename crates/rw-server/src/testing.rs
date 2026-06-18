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
use axum::http::{Method, Request, StatusCode};
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
/// [`with_comments`](Self::with_comments), or populated via
/// [`with_storage`](Self::with_storage)). Each call routes a single request
/// through the full axum stack via `oneshot`.
pub(crate) struct TestServer {
    router: Router,
    reload_tx: Option<broadcast::Sender<ReloadEvent>>,
}

impl TestServer {
    /// Fixed notify token used by the in-process harness so endpoint tests can
    /// present a matching `X-RW-Token` header.
    pub(crate) const TEST_NOTIFY_TOKEN: &str = "test-notify-token";

    /// Build a server backed by an in-memory `SqliteCommentStore` and an empty
    /// `MockStorage`-backed `Site`.
    pub(crate) async fn with_comments() -> Self {
        Self::build(MockStorage::new()).await
    }

    /// Build a server whose `Site` is backed by the given populated
    /// `MockStorage`, for exercising the page-rendering handlers.
    pub(crate) async fn with_storage(storage: MockStorage) -> Self {
        Self::build(storage).await
    }

    async fn build(storage: MockStorage) -> Self {
        Self::build_with_token(storage, Some(Self::TEST_NOTIFY_TOKEN.to_owned())).await
    }

    /// Build a server with a comments store but no configured notify token,
    /// for testing the "feature disabled" path.
    pub(crate) async fn with_comments_no_token() -> Self {
        Self::build_with_token(MockStorage::new(), None).await
    }

    async fn build_with_token(storage: MockStorage, notify_token: Option<String>) -> Self {
        let site = Arc::new(Site::new(
            Arc::new(storage),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        ));
        let comment_store = Arc::new(SqliteCommentStore::open_memory().await.unwrap());

        let state = Arc::new(AppState {
            site,
            live_reload: None,
            verbose: false,
            version: "test".to_owned(),
            comment_store,
            notify_token,
            #[cfg(feature = "embedded-preview")]
            embedded_preview: false,
        });

        Self {
            router: app::create_router(state),
            reload_tx: None,
        }
    }

    /// Build a server with live reload enabled (no file watcher started), so
    /// tests can subscribe to broadcast events. Uses the fixed test token.
    pub(crate) async fn with_live_reload() -> Self {
        let site = Arc::new(Site::new(
            Arc::new(MockStorage::new()),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        ));
        let comment_store = Arc::new(SqliteCommentStore::open_memory().await.unwrap());
        let (tx, _rx) = broadcast::channel::<ReloadEvent>(16);
        let live_reload = LiveReloadManager::new(Arc::clone(&site), tx.clone());

        let state = Arc::new(AppState {
            site,
            live_reload: Some(live_reload),
            verbose: false,
            version: "test".to_owned(),
            comment_store,
            notify_token: Some(Self::TEST_NOTIFY_TOKEN.to_owned()),
            #[cfg(feature = "embedded-preview")]
            embedded_preview: false,
        });

        Self {
            router: app::create_router(state),
            reload_tx: Some(tx),
        }
    }

    /// Subscribe to broadcast reload events. Panics if live reload is off.
    pub(crate) fn subscribe_reload(&self) -> broadcast::Receiver<ReloadEvent> {
        self.reload_tx
            .as_ref()
            .expect("TestServer built without live reload")
            .subscribe()
    }

    async fn send(&self, req: Request<Body>) -> TestResponse {
        let response = self.router.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        TestResponse { status, bytes }
    }

    /// `GET <path>`.
    pub(crate) async fn get(&self, path: &str) -> TestResponse {
        let req = Request::builder()
            .method(Method::GET)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        self.send(req).await
    }

    /// `DELETE <path>`.
    pub(crate) async fn delete(&self, path: &str) -> TestResponse {
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        self.send(req).await
    }

    /// `POST <path>` with a JSON body.
    pub(crate) async fn post_json(&self, path: &str, body: Value) -> TestResponse {
        let req = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(req).await
    }

    /// `POST <path>` with an empty body and a single header.
    pub(crate) async fn post_with_header(
        &self,
        path: &str,
        header: &str,
        value: &str,
    ) -> TestResponse {
        let req = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header, value)
            .body(Body::empty())
            .unwrap();
        self.send(req).await
    }

    /// `POST <path>` with a JSON body and a single extra header.
    pub(crate) async fn post_json_with_header(
        &self,
        path: &str,
        header: &str,
        value: &str,
        body: Value,
    ) -> TestResponse {
        let req = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header("content-type", "application/json")
            .header(header, value)
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(req).await
    }

    /// `PATCH <path>` with a JSON body.
    pub(crate) async fn patch_json(&self, path: &str, body: Value) -> TestResponse {
        let req = Request::builder()
            .method(Method::PATCH)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(req).await
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

/// Captured HTTP response — status plus already-collected body bytes.
pub(crate) struct TestResponse {
    pub(crate) status: StatusCode,
    bytes: body::Bytes,
}

impl TestResponse {
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
