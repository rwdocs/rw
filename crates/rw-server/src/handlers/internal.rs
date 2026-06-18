//! Internal (non-public) API endpoints.
//!
//! Token-guarded endpoints used by the `rw` CLI to notify a running server.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;

use crate::state::AppState;

const TOKEN_HEADER: &str = "X-RW-Token";

/// Request body for `POST /_api/_internal/events`. Tagged by `type`; an
/// unknown type is rejected with 400. New broadcastable event types add a
/// variant here.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EventRequest {
    Comments,
}

/// Handle `POST /_api/_internal/events`.
///
/// Authenticated by the `X-RW-Token` header matching the token written to
/// `.rw/server.json`. The JSON body names the event to broadcast to
/// live-reload subscribers (a no-op when live reload is disabled).
///
/// - No token configured on the server → `404 Not Found` (feature disabled).
/// - Token configured but request token missing/mismatched → `403 Forbidden`.
/// - Token valid but body missing or an unknown event type → `400 Bad Request`.
/// - Success → `204 No Content`.
pub(crate) async fn post_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(expected) = state.notify_token.as_deref() else {
        return StatusCode::NOT_FOUND;
    };
    let provided = headers.get(TOKEN_HEADER).and_then(|v| v.to_str().ok());
    if provided != Some(expected) {
        return StatusCode::FORBIDDEN;
    }
    let Ok(event) = serde_json::from_slice::<EventRequest>(&body) else {
        return StatusCode::BAD_REQUEST;
    };
    match event {
        EventRequest::Comments => state.notify_comments_changed(),
    }
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::live_reload::ReloadEvent;
    use crate::testing::TestServer;

    const PATH: &str = "/_api/_internal/events";

    #[tokio::test]
    async fn valid_token_returns_204() {
        let server = TestServer::with_comments().await;
        let resp = server
            .post_json_with_header(
                PATH,
                "X-RW-Token",
                TestServer::TEST_NOTIFY_TOKEN,
                json!({"type":"comments"}),
            )
            .await;
        assert_eq!(resp.status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn wrong_token_returns_403() {
        let server = TestServer::with_comments().await;
        let resp = server
            .post_json_with_header(
                PATH,
                "X-RW-Token",
                "not-the-token",
                json!({"type":"comments"}),
            )
            .await;
        assert_eq!(resp.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn missing_token_returns_403() {
        let server = TestServer::with_comments().await;
        let resp = server.post_json(PATH, json!({"type":"comments"})).await;
        assert_eq!(resp.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn no_configured_token_returns_404() {
        let server = TestServer::with_comments_no_token().await;
        let resp = server
            .post_json_with_header(
                PATH,
                "X-RW-Token",
                TestServer::TEST_NOTIFY_TOKEN,
                json!({"type":"comments"}),
            )
            .await;
        assert_eq!(resp.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn valid_token_broadcasts_comments_event() {
        let server = TestServer::with_live_reload().await;
        let mut rx = server.subscribe_reload();
        let resp = server
            .post_json_with_header(
                PATH,
                "X-RW-Token",
                TestServer::TEST_NOTIFY_TOKEN,
                json!({"type":"comments"}),
            )
            .await;
        assert_eq!(resp.status, StatusCode::NO_CONTENT);
        assert!(
            matches!(rx.try_recv(), Ok(ReloadEvent::Comments)),
            "valid token should broadcast a Comments event to subscribers",
        );
    }

    #[tokio::test]
    async fn unknown_event_type_returns_400() {
        let server = TestServer::with_comments().await;
        let resp = server
            .post_json_with_header(
                PATH,
                "X-RW-Token",
                TestServer::TEST_NOTIFY_TOKEN,
                json!({"type":"bogus"}),
            )
            .await;
        assert_eq!(resp.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn empty_body_with_valid_token_returns_400() {
        let server = TestServer::with_comments().await;
        let resp = server
            .post_with_header(PATH, "X-RW-Token", TestServer::TEST_NOTIFY_TOKEN)
            .await;
        assert_eq!(resp.status, StatusCode::BAD_REQUEST);
    }
}
