//! Static file serving.
//!
//! Provides static file serving for frontend assets and SPA fallback.
//!
//! # Modes
//!
//! - **Development** (default): Serves files from `frontend/dist` directory
//! - **Production** (`embed-assets` feature): Serves embedded assets from binary

#[cfg(not(feature = "embed-assets"))]
use std::path::Path;
use std::sync::Arc;

use axum::Router;
#[cfg(feature = "embed-assets")]
use axum::body::Body;
#[cfg(not(feature = "embed-assets"))]
use axum::http::StatusCode;
#[cfg(feature = "embed-assets")]
use axum::http::{Request, StatusCode, header};
#[cfg(not(feature = "embed-assets"))]
use axum::response::Html;
use axum::response::IntoResponse;
#[cfg(feature = "embed-assets")]
use axum::response::Response;
#[cfg(not(feature = "embed-assets"))]
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

/// Default static directory for development mode.
#[cfg(not(feature = "embed-assets"))]
const DEV_STATIC_DIR: &str = "frontend/dist";

/// Embedded frontend assets.
///
/// In release builds (with `embed-assets` feature), files are embedded
/// in the binary at compile time from `frontend/dist`.
#[cfg(feature = "embed-assets")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../frontend/dist"]
#[prefix = ""]
struct Assets;

/// Create router for static file serving.
///
/// With `embed-assets` feature: serves from embedded assets.
/// Without feature: serves from `frontend/dist` directory.
#[cfg(feature = "embed-assets")]
pub(crate) fn static_router() -> Router<Arc<AppState>> {
    Router::new().fallback(serve_embedded)
}

/// Create router for static file serving (filesystem mode).
#[cfg(not(feature = "embed-assets"))]
pub(crate) fn static_router() -> Router<Arc<AppState>> {
    let static_dir = Path::new(DEV_STATIC_DIR);
    let assets_dir = static_dir.join("assets");
    let favicon_path = static_dir.join("favicon.png");

    let mut router = Router::new();

    if assets_dir.exists() {
        router = router.nest_service("/assets", ServeDir::new(assets_dir));
    }

    if favicon_path.exists() {
        router = router.route_service("/favicon.png", ServeFile::new(favicon_path));
    }

    router
}

/// Serve embedded static files.
#[cfg(feature = "embed-assets")]
async fn serve_embedded(req: Request<Body>) -> Response {
    let path = req.uri().path().trim_start_matches('/');

    // Map root to index.html for SPA
    let file_path = if path.is_empty() { "index.html" } else { path };

    if let Some(content) = Assets::get(file_path) {
        let mime = mime_guess::from_path(file_path).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content.data.into_owned()))
            .unwrap();
    }

    // SPA fallback: serve index.html for client-side routing
    let is_spa_route = !path.starts_with("api/") && !path.contains('.');
    if is_spa_route && let Some(index) = Assets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(index.data.into_owned()))
            .unwrap();
    }

    StatusCode::NOT_FOUND.into_response()
}

/// SPA fallback handler (filesystem mode only).
#[cfg(not(feature = "embed-assets"))]
pub(crate) async fn spa_fallback() -> impl IntoResponse {
    let index_path = Path::new(DEV_STATIC_DIR).join("index.html");

    match tokio::fs::read_to_string(&index_path).await {
        Ok(content) => Html(content).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "embed-assets")]
    #[test]
    fn test_index_html_embedded() {
        assert!(Assets::get("index.html").is_some());
    }

    #[cfg(feature = "embed-assets")]
    #[test]
    fn test_assets_embedded() {
        let files: Vec<_> = Assets::iter().collect();
        assert!(files.iter().any(|f| f.ends_with(".js")));
        assert!(files.iter().any(|f| f.ends_with(".css")));
    }

    #[cfg(not(feature = "embed-assets"))]
    #[test]
    fn test_dev_static_dir_constant() {
        assert_eq!(DEV_STATIC_DIR, "frontend/dist");
    }
}
