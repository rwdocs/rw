//! Live reload manager.
//!
//! Coordinates file watching and WebSocket broadcasting for live reload.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::broadcast;

use rw_site::Site;
use rw_storage::{Storage, StorageEventKind, WatchHandle};

use crate::handlers::to_url_path;

/// Type of reload event sent to WebSocket clients.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReloadEventType {
    /// Only page content changed (no navigation impact).
    Content,
    /// Site structure changed (new/removed/renamed pages).
    Structure,
}

/// Event sent to connected WebSocket clients when files change.
///
/// Clone is required by `tokio::sync::broadcast` which delivers a copy to each subscriber.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReloadEvent {
    /// Event type.
    #[serde(rename = "type")]
    event_type: ReloadEventType,
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
        let url_path = to_url_path(&event.path);

        match &event.kind {
            StorageEventKind::Modified { title: new_title } => {
                // Get old title from cached snapshot (no reload)
                let old_title = site.page_title(&event.path);

                // If page is known, always send content event
                if old_title.is_some() {
                    let _ = broadcaster.send(ReloadEvent {
                        event_type: ReloadEventType::Content,
                        path: url_path.clone(),
                    });
                }

                // If title changed, invalidate site and send structure event
                if old_title.as_deref() != Some(new_title) {
                    site.invalidate();
                    let _ = broadcaster.send(ReloadEvent {
                        event_type: ReloadEventType::Structure,
                        path: url_path,
                    });
                }
            }
            StorageEventKind::Created => {
                site.invalidate();
                if site.has_page(&event.path) {
                    let _ = broadcaster.send(ReloadEvent {
                        event_type: ReloadEventType::Structure,
                        path: url_path,
                    });
                }
            }
            StorageEventKind::Removed => {
                let known = site.has_page(&event.path);
                site.invalidate();
                if known {
                    let _ = broadcaster.send(ReloadEvent {
                        event_type: ReloadEventType::Structure,
                        path: url_path,
                    });
                }
            }
        }
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
    fn test_content_event_serialization() {
        let event = ReloadEvent {
            event_type: ReloadEventType::Content,
            path: "/guide".to_owned(),
        };

        let json = serde_json::to_value(&event).unwrap();

        assert_eq!(json["type"], "content");
        assert_eq!(json["path"], "/guide");
    }

    #[test]
    fn test_structure_event_serialization() {
        let event = ReloadEvent {
            event_type: ReloadEventType::Structure,
            path: "/guide".to_owned(),
        };

        let json = serde_json::to_value(&event).unwrap();

        assert_eq!(json["type"], "structure");
        assert_eq!(json["path"], "/guide");
    }
}
