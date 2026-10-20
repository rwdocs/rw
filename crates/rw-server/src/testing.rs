//! In-process test harness for the axum HTTP layer.
//!
//! `TestServer` builds the real router against a comment store in a temporary
//! directory and a mock site, then routes individual requests through
//! `tower::ServiceExt::oneshot` so tests exercise the full middleware + handler
//! stack without binding a TCP socket.
//!
//! Test-only — gated under `#[cfg(test)]` so it never ships in release builds.

use std::path::PathBuf;
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
use tempfile::TempDir;
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

use crate::app;
use crate::live_reload::{LiveReloadManager, ReloadEvent};
use crate::state::AppState;

/// Test-only HTTP harness: wraps the production router around an on-disk
/// comment store and a `MockStorage`-backed site (empty via
/// [`new`](Self::new), or populated via [`with_storage`](Self::with_storage)).
/// Each call routes a single request through the full axum stack via `oneshot`.
pub(crate) struct TestServer {
    router: Router,
    state: Arc<AppState>,
    comments_dir: TempDir,
}

impl TestServer {
    /// Build a server backed by an empty `MockStorage`.
    pub(crate) async fn new() -> Self {
        Self::with_storage(MockStorage::new()).await
    }

    /// Build a server whose `Site` is backed by the given populated
    /// `MockStorage`, for exercising the page-rendering handlers.
    pub(crate) async fn with_storage(storage: MockStorage) -> Self {
        Self::build(storage, false).await
    }

    /// Build a server with live reload enabled (no file watcher started), so
    /// tests can subscribe to broadcast events.
    pub(crate) async fn with_live_reload(storage: MockStorage) -> Self {
        Self::build(storage, true).await
    }

    async fn build(storage: MockStorage, live_reload: bool) -> Self {
        let site = Arc::new(Site::new(
            Arc::new(storage),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        ));
        let comments_dir = tempfile::tempdir().unwrap();
        let comment_store = Arc::new(
            SqliteCommentStore::open(&comments_dir.path().join("sqlite.db"))
                .await
                .unwrap(),
        );
        let live_reload = if live_reload {
            let comments = comment_store.watch_commits().await.unwrap();
            Some(Arc::new(
                LiveReloadManager::without_watcher().polling_comments(comments),
            ))
        } else {
            None
        };

        let state = Arc::new(AppState {
            site,
            live_reload,
            comment_store,
        });

        Self {
            router: app::create_router(Arc::clone(&state)),
            state,
            comments_dir,
        }
    }

    /// The comment database, for opening a second writer on it.
    pub(crate) fn comments_db(&self) -> PathBuf {
        self.comments_dir.path().join("sqlite.db")
    }

    /// The state behind the router, for calling handlers directly.
    pub(crate) fn state(&self) -> Arc<AppState> {
        Arc::clone(&self.state)
    }

    /// Subscribe to broadcast reload events. Panics if live reload is off.
    pub(crate) fn subscribe_reload(&self) -> broadcast::Receiver<ReloadEvent> {
        self.state
            .live_reload
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

    /// Every value of a response header, in order.
    pub(crate) fn header_all(&self, name: &str) -> Vec<String> {
        self.headers
            .get_all(name)
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect()
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
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
