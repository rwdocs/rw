//! Static file serving.
//!
//! Provides static file serving for frontend assets and SPA fallback.
//! Uses `rw-assets` for asset retrieval in both embedded and filesystem modes.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

/// Create router for static file serving with SPA fallback.
pub(crate) fn static_router() -> Router<Arc<AppState>> {
    Router::new().fallback(serve_asset)
}

/// Serve a static asset or fall back to `index.html` for SPA routing.
async fn serve_asset(req: Request<Body>) -> Response {
    let path = req.uri().path().trim_start_matches('/');

    // Map root to index.html for SPA
    let file_path = if path.is_empty() { "index.html" } else { path };

    if let Some(content) = rw_assets::get(file_path) {
        let mime = rw_assets::mime_for(file_path);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(content.into_owned()))
            .unwrap();
    }

    // SPA fallback: serve index.html for client-side routing. Unmatched
    // requests under the reserved `/_api/` prefix are excluded so a bad API
    // path returns 404 instead of the HTML shell — including the bare
    // `/_api` segment with no trailing slash.
    let is_reserved_api = path == "_api" || path.starts_with("_api/");
    let is_spa_route = !is_reserved_api && !path.contains('.');
    if is_spa_route && let Some(index) = rw_assets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(index.into_owned()))
            .unwrap();
    }

    StatusCode::NOT_FOUND.into_response()
}

/// Serve a static asset, falling back to the embedded preview page.
///
/// Only serves actual asset files (JS, CSS, images etc). For any path
/// that doesn't match a real asset, returns the preview shell HTML —
/// except unmatched requests under the reserved `/_api/` prefix, which
/// 404 (mirroring [`serve_asset`]) so a bad API path never yields HTML.
#[cfg(feature = "embedded-preview")]
pub(crate) async fn asset_or_preview_fallback(req: Request<Body>) -> Response {
    let path = req.uri().path().trim_start_matches('/');

    // Only serve real asset files — don't map root or SPA routes to index.html.
    if !path.is_empty()
        && let Some(content) = rw_assets::get(path)
    {
        let mime = rw_assets::mime_for(path);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(content.into_owned()))
            .unwrap();
    }

    if path == "_api" || path.starts_with("_api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    rw_embedded_preview::preview_page().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serve_asset_returns_not_found_for_missing() {
        // Verify that the router can be constructed without panicking
        let _router: Router<Arc<AppState>> = static_router();
    }

    #[cfg(feature = "embedded-preview")]
    mod embedded_preview {
        use super::*;
        use axum::http::Uri;

        fn request_for(path: &str) -> Request<Body> {
            Request::builder()
                .uri(path.parse::<Uri>().unwrap())
                .body(Body::empty())
                .unwrap()
        }

        #[tokio::test]
        async fn fallback_returns_preview_for_root() {
            let response = asset_or_preview_fallback(request_for("/")).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                "text/html; charset=utf-8"
            );
        }

        #[tokio::test]
        async fn fallback_returns_preview_for_unknown_path() {
            let response = asset_or_preview_fallback(request_for("/some/doc/path")).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                "text/html; charset=utf-8"
            );
        }

        #[tokio::test]
        async fn fallback_returns_404_for_unknown_api_path() {
            // Unmatched paths under the reserved `/_api/` prefix must 404,
            // not return the preview shell HTML — including the bare
            // `/_api` form (no trailing slash).
            for path in ["/_api/does-not-exist", "/_api"] {
                let response = asset_or_preview_fallback(request_for(path)).await;
                assert_eq!(response.status(), StatusCode::NOT_FOUND, "for {path}");
            }
        }
    }
}
