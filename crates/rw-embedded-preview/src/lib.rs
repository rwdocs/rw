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
#[allow(clippy::unused_async)] // must be async to satisfy axum's Handler trait
pub async fn preview_page() -> Response {
    static_response(PREVIEW_HTML, "text/html; charset=utf-8")
}

/// Serve the preview page JavaScript as an external script.
///
/// Separated from the HTML to comply with Content-Security-Policy
/// `script-src 'self'` (inline scripts are blocked).
#[allow(clippy::unused_async)] // must be async to satisfy axum's Handler trait
pub async fn preview_script() -> Response {
    static_response(PREVIEW_JS, "text/javascript; charset=utf-8")
}

/// Serve the preview page CSS as an external stylesheet.
///
/// Separated from the HTML for consistency with the external JS approach.
#[allow(clippy::unused_async)] // must be async to satisfy axum's Handler trait
pub async fn preview_style() -> Response {
    static_response(PREVIEW_CSS, "text/css; charset=utf-8")
}

fn static_response(body: &'static str, content_type: &str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(body.into())
        .unwrap()
}

const PREVIEW_HTML: &str = include_str!("preview.html");
const PREVIEW_JS: &str = include_str!("preview.js");
const PREVIEW_CSS: &str = include_str!("preview.css");

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preview_page_returns_html() {
        let response = preview_page().await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn preview_script_returns_javascript() {
        let response = preview_script().await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn preview_style_returns_css() {
        let response = preview_style().await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/css; charset=utf-8"
        );
    }
}
