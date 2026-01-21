//! Static file serving.
//!
//! Provides static file serving for frontend assets and SPA fallback.

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

/// Create router for static file serving.
///
/// # Arguments
///
/// * `static_dir` - Directory containing static files
pub fn static_router(static_dir: &Path) -> Router<Arc<AppState>> {
    let assets_dir = static_dir.join("assets");
    let favicon_path = static_dir.join("favicon.png");

    let mut router = Router::new();

    // Serve /assets directory
    if assets_dir.exists() {
        router = router.nest_service("/assets", ServeDir::new(assets_dir));
    }

    // Serve favicon
    if favicon_path.exists() {
        router = router.route_service("/favicon.png", ServeFile::new(favicon_path));
    }

    router
}

/// SPA fallback handler.
///
/// Serves index.html for all non-API routes to support client-side routing.
pub async fn spa_fallback(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let index_path = state.static_dir.join("index.html");

    match tokio::fs::read_to_string(&index_path).await {
        Ok(content) => Html(content).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
