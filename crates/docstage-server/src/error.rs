//! Error types for the HTTP server.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Server error type.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ServerError {
    /// Page not found at the given path.
    #[error("Page not found: {0}")]
    PageNotFound(String),

    /// Render error from docstage-site.
    #[error("Render error: {0}")]
    Render(#[from] docstage_site::RenderError),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl IntoResponse for ServerError {
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
        };

        (status, axum::Json(body)).into_response()
    }
}
