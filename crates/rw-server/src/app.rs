//! Router construction.
//!
//! Builds the axum router with all routes and middleware.

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, patch, post};
use tower::ServiceBuilder;

use crate::handlers;
use crate::live_reload;
use crate::middleware::security;
use crate::state::AppState;
use crate::static_files;

/// Create the application router.
pub(crate) fn create_router(state: Arc<AppState>, embedded_preview: bool) -> Router {
    let mut router = Router::new()
        .route("/_api/config", get(handlers::config::get_config))
        .route(
            "/_api/navigation",
            get(handlers::navigation::get_navigation),
        )
        .route("/_api/pages/", get(handlers::pages::get_root_page))
        .route("/_api/pages/{*path}", get(handlers::pages::get_page))
        .route(
            "/_api/comments",
            get(handlers::comments::list_comments).post(handlers::comments::create_comment),
        )
        .route(
            "/_api/comments/{id}",
            patch(handlers::comments::update_comment).delete(handlers::comments::delete_comment),
        )
        .route(
            "/_api/_internal/events",
            post(handlers::internal::post_event),
        );

    if let Some(live_reload) = &state.live_reload {
        router = router.route(
            "/ws/live-reload",
            get(live_reload::ws_handler).with_state(Arc::clone(live_reload)),
        );
    }

    // The preview shell replaces only the SPA fallback; static assets are
    // still served normally.
    if embedded_preview {
        router = router
            .route(
                "/__embedded_preview.js",
                get(rw_embedded_preview::preview_script),
            )
            .route(
                "/__embedded_preview.css",
                get(rw_embedded_preview::preview_style),
            )
            .fallback(static_files::asset_or_preview_fallback);
    } else {
        router = router.fallback(static_files::serve_asset);
    }

    router
        .layer(
            ServiceBuilder::new()
                .layer(security::csp_layer())
                .layer(security::content_type_options_layer())
                .layer(security::frame_options_layer())
                .layer(security::cache_control_layer()),
        )
        .with_state(state)
}
