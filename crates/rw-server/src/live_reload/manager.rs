//! Live reload manager.
//!
//! Coordinates file watching and WebSocket broadcasting for live reload.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::broadcast;

use rw_site::SiteLoader;
use rw_storage::{Storage, StorageEventKind, WatchHandle};

/// Event sent to connected WebSocket clients when files change.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReloadEvent {
    /// Event type (always "reload").
    #[serde(rename = "type")]
    event_type: String,
    /// Documentation path that changed.
    path: String,
}

/// Manages file watching and broadcasting reload events.
pub(crate) struct LiveReloadManager {
    site_loader: Arc<SiteLoader>,
    broadcaster: broadcast::Sender<ReloadEvent>,
    _watch_handle: Option<WatchHandle>,
}

impl LiveReloadManager {
    /// Create a new live reload manager.
    ///
    /// # Arguments
    ///
    /// * `site_loader` - Site loader for cache invalidation and path resolution
    /// * `broadcaster` - Broadcast channel sender for reload events
    #[must_use]
    pub(crate) fn new(
        site_loader: Arc<SiteLoader>,
        broadcaster: broadcast::Sender<ReloadEvent>,
    ) -> Self {
        Self {
            site_loader,
            broadcaster,
            _watch_handle: None,
        }
    }

    /// Start the file watcher.
    ///
    /// Spawns a background task that watches for file changes and broadcasts
    /// reload events to connected WebSocket clients.
    ///
    /// # Errors
    ///
    /// Returns an error if the file watcher cannot be created.
    pub(crate) fn start(&mut self, storage: &dyn Storage) -> Result<(), rw_storage::StorageError> {
        let (rx, handle) = storage.watch()?;

        // Store the watch handle to keep the watcher alive
        self._watch_handle = Some(handle);

        // Spawn task to process storage events
        let site_loader = Arc::clone(&self.site_loader);
        let broadcaster = self.broadcaster.clone();

        std::thread::spawn(move || {
            for event in rx.iter() {
                Self::handle_storage_event(&event, &site_loader, &broadcaster);
            }
        });

        Ok(())
    }

    /// Handle a storage event.
    fn handle_storage_event(
        event: &rw_storage::StorageEvent,
        site_loader: &Arc<SiteLoader>,
        broadcaster: &broadcast::Sender<ReloadEvent>,
    ) {
        // Resolve doc path based on event kind
        let doc_path = match event.kind {
            StorageEventKind::Modified => {
                // Content change only - use cached site, no traversal needed.
                // The page renderer will re-read the file on next request.
                Self::resolve_doc_path_cached(&event.path, site_loader)
            }
            StorageEventKind::Created => {
                // Check if file already exists in cached site. If so, this is really
                // a modification (editors often save via "write temp + rename" which
                // appears as a Create event). Only do full reload for genuinely new files.
                if let Some(path) = Self::resolve_doc_path_cached(&event.path, site_loader) {
                    // File exists in cached site - treat as modification
                    Some(path)
                } else {
                    // New file - must reload to add it to site structure
                    site_loader.invalidate();
                    Self::resolve_doc_path(&event.path, site_loader)
                }
            }
            StorageEventKind::Removed => {
                // File deleted - invalidate and compute path from filename
                site_loader.invalidate();
                Self::compute_doc_path(&event.path)
            }
            _ => {
                // Unknown event kind - ignore
                return;
            }
        };

        let Some(doc_path) = doc_path else {
            return;
        };

        // Broadcast reload event
        let reload_event = ReloadEvent {
            event_type: "reload".to_string(),
            path: doc_path,
        };
        let _ = broadcaster.send(reload_event);
    }

    /// Compute documentation path from relative filesystem path for deleted files.
    fn compute_doc_path(relative_path: &Path) -> Option<String> {
        // Convert path components to URL segments
        let segments: Vec<_> = relative_path
            .with_extension("")
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();

        // Handle index files: /guide/index -> /guide, /index -> /
        let path = if segments.last().is_some_and(|s| s == "index") {
            let parent_segments = &segments[..segments.len() - 1];
            if parent_segments.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", parent_segments.join("/"))
            }
        } else {
            format!("/{}", segments.join("/"))
        };

        Some(path)
    }

    /// Resolve relative file system path to documentation URL path.
    ///
    /// Triggers a site reload if cache is invalid (for Created events).
    fn resolve_doc_path(relative_path: &Path, site_loader: &Arc<SiteLoader>) -> Option<String> {
        let site = site_loader.reload_if_needed();
        let page = site.get_page_by_source(relative_path)?;

        Some(page.path.clone())
    }

    /// Resolve relative file system path using cached site (no reload).
    ///
    /// Used for Modified events where site structure hasn't changed.
    fn resolve_doc_path_cached(
        relative_path: &Path,
        site_loader: &Arc<SiteLoader>,
    ) -> Option<String> {
        let site = site_loader.get();
        let page = site.get_page_by_source(relative_path)?;

        Some(page.path.clone())
    }

    /// Get a receiver for reload events.
    #[must_use]
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<ReloadEvent> {
        self.broadcaster.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reload_event_serialization() {
        let event = ReloadEvent {
            event_type: "reload".to_string(),
            path: "/guide".to_string(),
        };

        let json = serde_json::to_value(&event).unwrap();

        assert_eq!(json["type"], "reload");
        assert_eq!(json["path"], "/guide");
    }

    #[test]
    fn test_compute_doc_path_simple() {
        let relative_path = Path::new("guide.md");

        let result = LiveReloadManager::compute_doc_path(relative_path);
        assert_eq!(result, Some("/guide".to_string()));
    }

    #[test]
    fn test_compute_doc_path_nested() {
        let relative_path = Path::new("api/reference.md");

        let result = LiveReloadManager::compute_doc_path(relative_path);
        assert_eq!(result, Some("/api/reference".to_string()));
    }

    #[test]
    fn test_compute_doc_path_index() {
        let relative_path = Path::new("guide/index.md");

        let result = LiveReloadManager::compute_doc_path(relative_path);
        assert_eq!(result, Some("/guide".to_string()));
    }

    #[test]
    fn test_compute_doc_path_root_index() {
        let relative_path = Path::new("index.md");

        let result = LiveReloadManager::compute_doc_path(relative_path);
        assert_eq!(result, Some("/".to_string()));
    }
}
