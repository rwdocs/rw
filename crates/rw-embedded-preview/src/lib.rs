//! Embedded preview shell for RW documentation engine.
//!
//! Serves a self-contained HTML page that wraps the RW viewer in a
//! minimal host-app shell for visual testing of embedded mode.
//! Replaces the normal SPA frontend with a Backstage-like shell
//! that embeds the viewer via `mountRw()`.

use axum::http::{StatusCode, header};
use axum::response::Response;

/// Serve the embedded preview HTML page.
///
/// Returns the same page regardless of the path — the JS extracts
/// the document path from the URL and passes it as `initialPath`.
pub async fn preview_page() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(PREVIEW_HTML.into())
        .unwrap()
}

/// Serve the preview page JavaScript as an external script.
///
/// Separated from the HTML to comply with Content-Security-Policy
/// `script-src 'self'` (inline scripts are blocked).
pub async fn preview_script() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/javascript; charset=utf-8")
        .body(PREVIEW_JS.into())
        .unwrap()
}

const PREVIEW_HTML: &str = include_str!("preview.html");
const PREVIEW_JS: &str = include_str!("preview.js");
