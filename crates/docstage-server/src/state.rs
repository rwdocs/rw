//! Application state.
//!
//! Shared state for all request handlers.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use docstage_core::{PageRenderer, SiteLoader};

use crate::live_reload::LiveReloadManager;

/// Application state shared across all handlers.
pub struct AppState {
    /// Page renderer for markdown to HTML conversion.
    pub renderer: PageRenderer,
    /// Site loader for document structure.
    pub site_loader: Arc<RwLock<SiteLoader>>,
    /// Live reload manager (if enabled).
    pub live_reload: Option<LiveReloadManager>,
    /// Enable verbose output (show warnings).
    pub verbose: bool,
    /// Application version for cache invalidation.
    pub version: String,
    /// Static files directory.
    pub static_dir: PathBuf,
}

impl AppState {
    /// Check if live reload is enabled.
    #[must_use]
    pub fn live_reload_enabled(&self) -> bool {
        self.live_reload.is_some()
    }
}
