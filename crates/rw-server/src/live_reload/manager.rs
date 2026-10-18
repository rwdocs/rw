//! Live reload manager.
//!
//! Coordinates file watching and WebSocket broadcasting for live reload.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::broadcast;

use rw_site::{Site, to_url_path};
use rw_storage::{Storage, StorageEventKind, WatchHandle};

/// Event sent to connected WebSocket clients.
///
/// Clone is required by `tokio::sync::broadcast` which delivers a copy to
/// each subscriber.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum ReloadEvent {
    /// Only page content changed (no navigation impact).
    Content { path: String },
    /// Site structure changed (new/removed/renamed pages).
    Structure { path: String },
    /// A comment on some page changed; every viewer refetches its own
    /// current page's comments. Carries no path — it is a generic signal.
    Comments,
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
            for event in &rx {
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
            StorageEventKind::Modified { meta } => {
                // Any metadata field can reach nav, sections, breadcrumbs, other
                // pages' link resolution, or the page's own meta, and all of
                // them are read from the snapshot. `None` also covers a site
                // that has not loaded since startup, while a viewer from before
                // a restart still shows this page.
                let structure_changed = site.page_meta(&event.path).is_none_or(|old| *old != *meta);

                // Invalidate before broadcasting: a viewer refetches on receipt
                // and must not read the old metadata.
                if structure_changed {
                    site.invalidate();
                }
                // Unconditional: the viewer only acts on its own page.
                let _ = broadcaster.send(ReloadEvent::Content {
                    path: url_path.clone(),
                });
                if structure_changed {
                    let _ = broadcaster.send(ReloadEvent::Structure { path: url_path });
                }
            }
            StorageEventKind::Created => {
                // Broadcast unconditionally. Do NOT gate this on a
                // snapshot read (e.g., `site.has_page(&event.path)`):
                // any read here triggers a reload that races the
                // watcher event we just received, and on transient
                // `storage.scan()` failure returns the pre-event
                // snapshot — silently dropping the broadcast. See the
                // `created_broadcasts_*` tests below.
                site.invalidate();
                let _ = broadcaster.send(ReloadEvent::Structure { path: url_path });
            }
            StorageEventKind::Removed => {
                // A removed directory need not be a page itself.
                let known = site.has_page_under(&event.path).unwrap_or(false);
                site.invalidate();
                if known {
                    let _ = broadcaster.send(ReloadEvent::Structure { path: url_path });
                }
            }
        }
    }

    /// Get a receiver for reload events.
    #[must_use]
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<ReloadEvent> {
        self.broadcaster.subscribe()
    }

    /// Broadcast a generic "comments changed" event. Every connected viewer
    /// refetches its own current page's comments. A send error means no
    /// subscribers are connected, which is fine — ignore it.
    pub(crate) fn notify_comments_changed(&self) {
        let _ = self.broadcaster.send(ReloadEvent::Comments);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    use rw_site::{Meta, PageRendererConfig};
    use rw_storage::{MockStorage, StorageErrorKind, StorageEvent};

    #[test]
    fn test_content_event_serialization() {
        let event = ReloadEvent::Content {
            path: "/guide".to_owned(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content");
        assert_eq!(json["path"], "/guide");
    }

    #[test]
    fn test_structure_event_serialization() {
        let event = ReloadEvent::Structure {
            path: "/guide".to_owned(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "structure");
        assert_eq!(json["path"], "/guide");
    }

    #[test]
    fn test_comments_event_serialization() {
        let event = ReloadEvent::Comments;
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "comments");
        assert!(
            json.get("path").is_none(),
            "comments event must not carry a path: {json}"
        );
    }

    #[test]
    fn notify_comments_changed_broadcasts_comments_event() {
        let storage = Arc::new(MockStorage::new());
        let site = loaded_site(&storage);
        let (tx, mut rx) = broadcast::channel(8);
        let manager = LiveReloadManager::new(site, tx);
        manager.notify_comments_changed();
        assert_matches!(rx.try_recv().unwrap(), ReloadEvent::Comments);
    }

    // Returns a `Site` with `has_loaded=true`, so any subsequent
    // `reload_if_needed` failure is swallowed and the stale snapshot is
    // returned — the production behavior the Created handler must cope with.
    fn loaded_site(storage: &Arc<MockStorage>) -> Arc<Site> {
        let site = Arc::new(Site::new(
            Arc::clone(storage) as Arc<dyn Storage>,
            Arc::new(rw_cache::NullCache),
            PageRendererConfig::default(),
        ));
        // Force has_loaded=true so reload_if_needed swallows later errors.
        site.navigation(None).expect("initial load");
        site
    }

    #[test]
    fn created_broadcasts_structure_even_when_post_event_reload_would_fail() {
        // The Created handler must broadcast Structure without depending on
        // any post-invalidate snapshot read. Setup arranges a state where any
        // such read would silently swallow a scan error and return the
        // (pre-event) stale snapshot — exactly the race that drops the
        // broadcast.
        let storage = Arc::new(MockStorage::new());
        let site = loaded_site(&storage);

        let (tx, mut rx) = broadcast::channel(8);

        storage.set_scan_error(Some(StorageErrorKind::Unavailable));

        LiveReloadManager::handle_storage_event(
            &StorageEvent {
                path: "foo".into(),
                kind: StorageEventKind::Created,
            },
            &site,
            &tx,
        );

        let event = rx
            .try_recv()
            .expect("Created should broadcast Structure even when reload would fail");
        assert_matches!(event, ReloadEvent::Structure { ref path } if path == "/foo");
        assert!(
            rx.try_recv().is_err(),
            "Created should produce exactly one Structure broadcast",
        );

        // Confirm the broadcast did not depend on a successful scan: a
        // direct read still surfaces the stale snapshot (the scan error
        // is swallowed and `foo` is absent), so any future re-introduction
        // of a post-invalidate `has_page` re-check would re-trigger the bug.
        assert!(
            !site
                .has_page("foo")
                .expect("scan error is swallowed after initial load")
        );
    }

    fn modified(path: &str, title: &str) -> StorageEvent {
        modified_meta(path, &format!("title: {title}"))
    }

    fn modified_meta(path: &str, meta_yaml: &str) -> StorageEvent {
        StorageEvent {
            path: path.into(),
            kind: StorageEventKind::Modified {
                meta: Meta::resolve(None, Some(meta_yaml), path),
            },
        }
    }

    fn drain(rx: &mut broadcast::Receiver<ReloadEvent>) -> Vec<String> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .map(|event| serde_json::to_string(&event).unwrap())
            .collect()
    }

    #[test]
    fn modified_body_broadcasts_content_only() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&modified("guide", "Guide"), &site, &tx);

        assert_eq!(drain(&mut rx), [r#"{"type":"content","path":"/guide"}"#]);
        site.navigation(None).unwrap();
        assert_eq!(storage.scan_count(), scans, "a body edit must not rescan");
    }

    #[test]
    fn modified_title_broadcasts_content_and_structure() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = loaded_site(&storage);
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&modified("guide", "New"), &site, &tx);

        assert_eq!(
            drain(&mut rx),
            [
                r#"{"type":"content","path":"/guide"}"#,
                r#"{"type":"structure","path":"/guide"}"#,
            ]
        );
    }

    #[test]
    fn modified_kind_invalidates_and_broadcasts_structure() {
        let storage = Arc::new(MockStorage::new().with_document_meta("guide", "title: Guide"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(
            &modified_meta("guide", "title: Guide\nkind: system"),
            &site,
            &tx,
        );

        assert_eq!(
            drain(&mut rx),
            [
                r#"{"type":"content","path":"/guide"}"#,
                r#"{"type":"structure","path":"/guide"}"#,
            ]
        );
        site.navigation(None).unwrap();
        assert_eq!(storage.scan_count(), scans + 1, "site must be invalidated");
    }

    #[test]
    fn modified_before_first_load_still_reloads_the_page() {
        // A viewer left open across a server restart reconnects its socket
        // without refetching, so the site may never have loaded. Its page
        // must still reload on the next edit.
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = Arc::new(Site::new(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::new(rw_cache::NullCache),
            PageRendererConfig::default(),
        ));
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&modified("guide", "Guide"), &site, &tx);

        assert_eq!(
            drain(&mut rx),
            [
                r#"{"type":"content","path":"/guide"}"#,
                r#"{"type":"structure","path":"/guide"}"#,
            ]
        );
    }

    fn removed(path: &str) -> StorageEvent {
        StorageEvent {
            path: path.into(),
            kind: StorageEventKind::Removed,
        }
    }

    #[test]
    fn removed_directory_without_its_own_page_broadcasts_structure() {
        let storage = Arc::new(MockStorage::new().with_document("notes/a", "A"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&removed("notes"), &site, &tx);

        assert_eq!(drain(&mut rx), [r#"{"type":"structure","path":"/notes"}"#]);
        site.navigation(None).unwrap();
        assert_eq!(storage.scan_count(), scans + 1, "site must be invalidated");
    }

    #[test]
    fn removed_unknown_path_invalidates_without_broadcast() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&removed("image.png"), &site, &tx);

        assert_eq!(drain(&mut rx), [] as [String; 0]);
        site.navigation(None).unwrap();
        assert_eq!(storage.scan_count(), scans + 1, "site must be invalidated");
    }

    #[test]
    fn created_broadcasts_structure_on_successful_path() {
        // Locks in the simple-path behavior: a Created event with no
        // backend failure still produces a single Structure broadcast.
        let storage = Arc::new(MockStorage::new());
        let site = loaded_site(&storage);

        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(
            &StorageEvent {
                path: "foo".into(),
                kind: StorageEventKind::Created,
            },
            &site,
            &tx,
        );

        let event = rx.try_recv().expect("Created should broadcast Structure");
        assert_matches!(event, ReloadEvent::Structure { ref path } if path == "/foo");
        assert!(
            rx.try_recv().is_err(),
            "Created should produce exactly one Structure broadcast",
        );
    }
}
