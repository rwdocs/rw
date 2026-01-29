//! Application state.
//!
//! Shared state for all request handlers.

use std::sync::Arc;

use rw_site::Site;

use crate::live_reload::LiveReloadManager;

/// Application state shared across all handlers.
pub(crate) struct AppState {
    /// Unified site structure and page renderer.
    pub(crate) site: Arc<Site>,
    /// Live reload manager (if enabled).
    pub(crate) live_reload: Option<LiveReloadManager>,
    /// Enable verbose output (show warnings).
    pub(crate) verbose: bool,
    /// Application version for cache invalidation.
    pub(crate) version: String,
}

impl AppState {
    /// Check if live reload is enabled.
    #[must_use]
    pub(crate) fn live_reload_enabled(&self) -> bool {
        self.live_reload.is_some()
    }
}
