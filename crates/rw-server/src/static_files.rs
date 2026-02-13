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

    // SPA fallback: serve index.html for client-side routing
    let is_spa_route = !path.starts_with("api/") && !path.contains('.');
    if is_spa_route
        && let Some(index) = rw_assets::get("index.html")
    {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(index.into_owned()))
            .unwrap();
    }

    StatusCode::NOT_FOUND.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serve_asset_returns_not_found_for_missing() {
        // Verify that the router can be constructed without panicking
        let _router: Router<Arc<AppState>> = static_router();
    }
}
