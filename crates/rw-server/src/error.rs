//! Error types for the HTTP server.

use std::net::AddrParseError;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rw_storage::StorageError;
use serde_json::json;

/// Error returned when the server fails to start.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// Failed to start file watching for live reload.
    #[error("failed to start file watcher: {0}")]
    Watch(#[from] StorageError),

    /// Invalid bind address.
    #[error("invalid bind address: {0}")]
    InvalidAddress(#[from] AddrParseError),

    /// I/O error (bind or serve failure).
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Handler error type (internal).
#[derive(Debug, thiserror::Error)]
pub(crate) enum HandlerError {
    /// Page not found at the given path.
    #[error("Page not found: {0}")]
    PageNotFound(String),

    /// Render error from rw-site.
    #[error("Render error: {0}")]
    Render(#[from] rw_site::RenderError),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Storage backend unavailable.
    #[error("Storage error: {0}")]
    Storage(#[from] rw_storage::StorageError),
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            Self::PageNotFound(path) => (
                StatusCode::NOT_FOUND,
                json!({"error": "Page not found", "path": path}),
            ),
            Self::Render(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": e.to_string()}),
            ),
            Self::Io(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": e.to_string()}),
            ),
            Self::Storage(e) => (
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"error": "Storage unavailable", "detail": e.display_chain()}),
            ),
        };

        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn test_storage_error_returns_503() {
        let err = HandlerError::Storage(
            rw_storage::StorageError::new(rw_storage::StorageErrorKind::Unavailable)
                .with_backend("S3"),
        );
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
