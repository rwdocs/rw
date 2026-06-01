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
use tower::ServiceExt;
use uuid::Uuid;

use crate::app;
use crate::state::AppState;

/// Test-only HTTP harness: wraps the production router around an in-memory
/// comment store and an empty mock site. Each call routes a single request
/// through the full axum stack via `oneshot`.
pub(crate) struct TestServer {
    router: Router,
}

impl TestServer {
    /// Build a server backed by an in-memory `SqliteCommentStore` and an empty
    /// `MockStorage`-backed `Site`.
    pub(crate) async fn with_comments() -> Self {
        let storage = Arc::new(MockStorage::new());
        let site = Arc::new(Site::new(
            storage,
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
            #[cfg(feature = "embedded-preview")]
            embedded_preview: false,
        });

        Self {
            router: app::create_router(state),
        }
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
