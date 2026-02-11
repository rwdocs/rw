//! Live reload manager.
//!
//! Coordinates file watching and WebSocket broadcasting for live reload.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::broadcast;

use rw_site::Site;
use rw_storage::{Storage, StorageEventKind, WatchHandle};

use crate::handlers::to_url_path;

/// Event sent to connected WebSocket clients when files change.
///
/// Clone is required by `tokio::sync::broadcast` which delivers a copy to each subscriber.
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
    site: Arc<Site>,
    broadcaster: broadcast::Sender<ReloadEvent>,
    watch_handle: Option<WatchHandle>,
}

impl LiveReloadManager {
    /// Create a new live reload manager.
    ///
    /// # Arguments
    ///
    /// * `site` - Site for cache invalidation and path resolution
    /// * `broadcaster` - Broadcast channel sender for reload events
    #[must_use]
    pub(crate) fn new(site: Arc<Site>, broadcaster: broadcast::Sender<ReloadEvent>) -> Self {
        Self {
            site,
            broadcaster,
            watch_handle: None,
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
        self.watch_handle = Some(handle);

        // Spawn task to process storage events
        let site = Arc::clone(&self.site);
        let broadcaster = self.broadcaster.clone();

        std::thread::spawn(move || {
            for event in rx.iter() {
                Self::handle_storage_event(&event, &site, &broadcaster);
            }
        });

        Ok(())
    }

    /// Handle a storage event.
    fn handle_storage_event(
        event: &rw_storage::StorageEvent,
        site: &Arc<Site>,
        broadcaster: &broadcast::Sender<ReloadEvent>,
    ) {
        // Storage events now use URL paths directly (e.g., "guide", "domain/api").
        // Resolve doc path based on event kind.
        // The debouncer already handles editor save patterns (Removed + Created → Modified),
        // so we can trust the event types directly.
        let known = match event.kind {
            StorageEventKind::Modified => {
                // Content change only - use cached site state, no traversal needed.
                site.has_page(&event.path)
            }
            StorageEventKind::Created => {
                // New file - invalidate so next access reloads site structure
                site.invalidate();
                site.has_page(&event.path)
            }
            StorageEventKind::Removed => {
                // File deleted - check cached site before invalidating
                let known = site.has_page(&event.path);
                site.invalidate();
                known
            }
        };

        if !known {
            return;
        }

        // Broadcast reload event with URL path (add leading slash for frontend)
        let reload_event = ReloadEvent {
            event_type: "reload".to_owned(),
            path: to_url_path(&event.path),
        };
        let _ = broadcaster.send(reload_event);
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
            event_type: "reload".to_owned(),
            path: "/guide".to_owned(),
        };

        let json = serde_json::to_value(&event).unwrap();

        assert_eq!(json["type"], "reload");
        assert_eq!(json["path"], "/guide");
    }
}
