//! Router construction.
//!
//! Builds the axum router with all routes and middleware.

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use tower::ServiceBuilder;

use crate::handlers;
use crate::live_reload;
use crate::middleware::security;
use crate::state::AppState;
use crate::static_files;

/// Create the application router.
///
/// # Arguments
///
/// * `state` - Shared application state
pub(crate) fn create_router(state: Arc<AppState>) -> Router {
    // API routes
    let api_routes = Router::new()
        .route("/api/config", get(handlers::config::get_config))
        .route("/api/navigation", get(handlers::navigation::get_navigation))
        .route("/api/pages/", get(handlers::pages::get_root_page))
        .route("/api/pages/{*path}", get(handlers::pages::get_page));

    let mut router = Router::new().merge(api_routes);

    // WebSocket for live reload
    if state.live_reload.is_some() {
        router = router.route("/ws/live-reload", get(live_reload::ws_handler));
    }

    // Static files and SPA fallback
    router = router.merge(static_files::static_router());

    // Add security headers middleware
    router
        .layer(
            ServiceBuilder::new()
                .layer(security::csp_layer())
                .layer(security::content_type_options_layer())
                .layer(security::frame_options_layer()),
        )
        .with_state(state)
}
