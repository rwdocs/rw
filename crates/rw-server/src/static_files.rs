//! Static file serving.
//!
//! Provides static file serving for frontend assets and SPA fallback.
//! Uses `rw-assets` for asset retrieval in both embedded and filesystem modes.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Serve a static asset or fall back to `index.html` for SPA routing.
pub(crate) async fn serve_asset(req: Request<Body>) -> Response {
    let path = req.uri().path().trim_start_matches('/');

    let file_path = if path.is_empty() { "index.html" } else { path };
    if let Some(response) = asset_response(file_path) {
        return response;
    }

    let is_spa_route = !is_reserved_api(path) && !path.contains('.');
    if is_spa_route && let Some(index) = rw_assets::get("index.html") {
        return ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], index).into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

/// Serve a static asset, falling back to the embedded preview page.
///
/// Only serves actual asset files (JS, CSS, images etc). For any other
/// path outside `/_api`, returns the preview shell HTML.
pub(crate) async fn asset_or_preview_fallback(req: Request<Body>) -> Response {
    let path = req.uri().path().trim_start_matches('/');

    // Only serve real asset files — don't map root or SPA routes to index.html.
    if !path.is_empty()
        && let Some(response) = asset_response(path)
    {
        return response;
    }

    if is_reserved_api(path) {
        return StatusCode::NOT_FOUND.into_response();
    }

    rw_embedded_preview::preview_page()
}

fn asset_response(path: &str) -> Option<Response> {
    let content = rw_assets::get(path)?;
    Some(([(header::CONTENT_TYPE, rw_assets::mime_for(path))], content).into_response())
}

/// A bad API path must 404 rather than get an HTML shell, and that includes
/// the bare `_api` with no trailing slash.
fn is_reserved_api(path: &str) -> bool {
    path == "_api" || path.starts_with("_api/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestServer;

    #[tokio::test]
    async fn test_serve_asset_returns_not_found_for_missing() {
        let server = TestServer::new().await;
        // A dotted path is an asset request, never an SPA route.
        for path in ["/missing-asset.js", "/_api/does-not-exist", "/_api"] {
            let response = server.get(path).await;
            assert_eq!(response.status, StatusCode::NOT_FOUND, "for {path}");
        }
    }

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
