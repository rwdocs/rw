//! Application state.
//!
//! Shared state for all request handlers.

use std::sync::Arc;

use rw_comments::SqliteCommentStore;
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
    /// Comment store.
    pub(crate) comment_store: Arc<SqliteCommentStore>,
    /// Secret token from `.rw/server.json` that authenticates the internal
    /// comments-changed notify endpoint. `None` when the bound address could
    /// not be read (`listener.local_addr()` failed), in which case the endpoint
    /// returns 404 and the live-notify feature is effectively disabled.
    pub(crate) notify_token: Option<String>,
    /// Enable embedded preview page at /.
    #[cfg(feature = "embedded-preview")]
    pub(crate) embedded_preview: bool,
}

impl AppState {
    /// Check if live reload is enabled.
    #[must_use]
    pub(crate) fn live_reload_enabled(&self) -> bool {
        self.live_reload.is_some()
    }

    /// Broadcast a comments-changed event to live-reload subscribers, if
    /// enabled. No-op when live reload is off.
    pub(crate) fn notify_comments_changed(&self) {
        if let Some(ref live_reload) = self.live_reload {
            live_reload.notify_comments_changed();
        }
    }
}
