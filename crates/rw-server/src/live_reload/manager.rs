//! Live reload manager.
//!
//! Coordinates file watching and WebSocket broadcasting for live reload.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use rw_site::SiteLoader;

use super::debouncer::{EventDebouncer, FsEvent, FsEventKind};

/// Event sent to connected WebSocket clients when files change.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReloadEvent {
    /// Event type (always "reload").
    #[serde(rename = "type")]
    event_type: String,
    /// Documentation path that changed.
    path: String,
}

/// Default debounce duration in milliseconds.
const DEFAULT_DEBOUNCE_MS: u64 = 100;

/// Manages file watching and broadcasting reload events.
pub(crate) struct LiveReloadManager {
    source_dir: PathBuf,
    watch_patterns: Vec<String>,
    site_loader: Arc<SiteLoader>,
    broadcaster: broadcast::Sender<ReloadEvent>,
    watcher: Option<RecommendedWatcher>,
    debounce_ms: u64,
}

impl LiveReloadManager {
    /// Create a new live reload manager.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Directory to watch for changes
    /// * `watch_patterns` - Glob patterns to match (e.g., `["**/*.md"]`)
    /// * `site_loader` - Site loader for cache invalidation and path resolution
    /// * `broadcaster` - Broadcast channel sender for reload events
    #[must_use]
    pub(crate) fn new(
        source_dir: PathBuf,
        watch_patterns: Option<Vec<String>>,
        site_loader: Arc<SiteLoader>,
        broadcaster: broadcast::Sender<ReloadEvent>,
    ) -> Self {
        Self {
            source_dir,
            watch_patterns: watch_patterns.unwrap_or_else(|| vec!["**/*.md".to_string()]),
            site_loader,
            broadcaster,
            watcher: None,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
        }
    }

    /// Set the debounce duration in milliseconds.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn with_debounce_ms(mut self, debounce_ms: u64) -> Self {
        self.debounce_ms = debounce_ms;
        self
    }

    /// Start the file watcher.
    ///
    /// Spawns a background task that watches for file changes and broadcasts
    /// reload events to connected WebSocket clients.
    ///
    /// # Errors
    ///
    /// Returns an error if the file watcher cannot be created.
    pub(crate) fn start(&mut self) -> Result<(), notify::Error> {
        let (tx, mut rx) = mpsc::channel::<Event>(100);
        let source_dir = self.source_dir.clone();

        // Create watcher with callback that sends events to channel
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // Use blocking_send since callback is sync
                let _ = tx.blocking_send(event);
            }
        })?;

        watcher.watch(&source_dir, RecursiveMode::Recursive)?;
        self.watcher = Some(watcher);

        // Create debouncer
        let debouncer = Arc::new(EventDebouncer::new(Duration::from_millis(self.debounce_ms)));
        let debouncer_for_record = Arc::clone(&debouncer);

        // Spawn task to record events into debouncer
        let watch_patterns = self.watch_patterns.clone();
        let source_dir_for_record = self.source_dir.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                Self::record_event(
                    &event,
                    &source_dir_for_record,
                    &watch_patterns,
                    &debouncer_for_record,
                );
            }
        });

        // Spawn task to process debounced events
        let site_loader = Arc::clone(&self.site_loader);
        let broadcaster = self.broadcaster.clone();
        let source_dir_for_process = self.source_dir.clone();
        let poll_interval = Duration::from_millis(50);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(poll_interval);

            loop {
                interval.tick().await;

                for fs_event in debouncer.drain_ready() {
                    Self::handle_fs_event(
                        &fs_event,
                        &source_dir_for_process,
                        &site_loader,
                        &broadcaster,
                    );
                }
            }
        });

        Ok(())
    }

    /// Record a raw filesystem event into the debouncer.
    fn record_event(
        event: &Event,
        source_dir: &Path,
        watch_patterns: &[String],
        debouncer: &EventDebouncer,
    ) {
        // Convert notify EventKind to FsEventKind
        let kind = match event.kind {
            EventKind::Create(_) => FsEventKind::Created,
            EventKind::Modify(_) => FsEventKind::Modified,
            EventKind::Remove(_) => FsEventKind::Removed,
            _ => return,
        };

        for path in &event.paths {
            // Check if path matches watch patterns
            if !Self::matches_patterns(path, source_dir, watch_patterns) {
                continue;
            }

            debouncer.record(path.clone(), kind);
            tracing::debug!(path = %path.display(), ?kind, "Recorded filesystem event");
        }
    }

    /// Handle a debounced filesystem event.
    fn handle_fs_event(
        fs_event: &FsEvent,
        source_dir: &Path,
        site_loader: &Arc<SiteLoader>,
        broadcaster: &broadcast::Sender<ReloadEvent>,
    ) {
        let start = Instant::now();

        // Resolve doc path based on event kind
        let doc_path = match fs_event.kind {
            FsEventKind::Modified => {
                // Content change only - use cached site, no traversal needed.
                // The page renderer will re-read the file on next request.
                Self::resolve_doc_path_cached(&fs_event.path, source_dir, site_loader)
            }
            FsEventKind::Created => {
                // Check if file already exists in cached site. If so, this is really
                // a modification (editors often save via "write temp + rename" which
                // appears as a Create event). Only do full reload for genuinely new files.
                if let Some(path) =
                    Self::resolve_doc_path_cached(&fs_event.path, source_dir, site_loader)
                {
                    // File exists in cached site - treat as modification
                    Some(path)
                } else {
                    // New file - must reload to add it to site structure
                    site_loader.invalidate();
                    Self::resolve_doc_path(&fs_event.path, source_dir, site_loader)
                }
            }
            FsEventKind::Removed => {
                // File deleted - invalidate and compute path from filename
                site_loader.invalidate();
                Self::compute_doc_path(&fs_event.path, source_dir)
            }
        };

        let Some(doc_path) = doc_path else {
            return;
        };

        // Broadcast reload event
        let reload_event = ReloadEvent {
            event_type: "reload".to_string(),
            path: doc_path.clone(),
        };
        let _ = broadcaster.send(reload_event);

        tracing::info!(
            path = %doc_path,
            kind = ?fs_event.kind,
            elapsed_ms = start.elapsed().as_secs_f64() * 1000.0,
            "Live reload event processed"
        );
    }

    /// Compute documentation path from filesystem path for deleted files.
    fn compute_doc_path(file_path: &Path, source_dir: &Path) -> Option<String> {
        let relative = file_path.strip_prefix(source_dir).ok()?;

        // Convert path components to URL segments
        let segments: Vec<_> = relative
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

    /// Check if a path matches any watch pattern.
    fn matches_patterns(path: &Path, source_dir: &Path, patterns: &[String]) -> bool {
        let Ok(relative) = path.strip_prefix(source_dir) else {
            return false;
        };

        let relative_str = relative.to_string_lossy();

        patterns
            .iter()
            .filter_map(|p| glob::Pattern::new(p).ok())
            .any(|glob_pattern| glob_pattern.matches(&relative_str))
    }

    /// Resolve file system path to documentation URL path.
    ///
    /// Triggers a site reload if cache is invalid (for Created events).
    fn resolve_doc_path(
        file_path: &Path,
        source_dir: &Path,
        site_loader: &Arc<SiteLoader>,
    ) -> Option<String> {
        let relative = file_path.strip_prefix(source_dir).ok()?;

        let site = site_loader.reload_if_needed();
        let page = site.get_page_by_source(relative)?;

        Some(page.path.clone())
    }

    /// Resolve file system path using cached site (no reload).
    ///
    /// Used for Modified events where site structure hasn't changed.
    fn resolve_doc_path_cached(
        file_path: &Path,
        source_dir: &Path,
        site_loader: &Arc<SiteLoader>,
    ) -> Option<String> {
        let relative = file_path.strip_prefix(source_dir).ok()?;

        let site = site_loader.get();
        let page = site.get_page_by_source(relative)?;

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
    use std::path::PathBuf;

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
    fn test_matches_patterns_md_files() {
        let source_dir = PathBuf::from("/docs");
        let patterns = vec!["**/*.md".to_string()];

        assert!(LiveReloadManager::matches_patterns(
            &PathBuf::from("/docs/guide.md"),
            &source_dir,
            &patterns
        ));
        assert!(LiveReloadManager::matches_patterns(
            &PathBuf::from("/docs/nested/page.md"),
            &source_dir,
            &patterns
        ));
        assert!(!LiveReloadManager::matches_patterns(
            &PathBuf::from("/docs/image.png"),
            &source_dir,
            &patterns
        ));
    }

    #[test]
    fn test_matches_patterns_outside_source_dir() {
        let source_dir = PathBuf::from("/docs");
        let patterns = vec!["**/*.md".to_string()];

        assert!(!LiveReloadManager::matches_patterns(
            &PathBuf::from("/other/guide.md"),
            &source_dir,
            &patterns
        ));
    }

    #[test]
    fn test_compute_doc_path_simple() {
        let source_dir = PathBuf::from("/docs");
        let file_path = PathBuf::from("/docs/guide.md");

        let result = LiveReloadManager::compute_doc_path(&file_path, &source_dir);
        assert_eq!(result, Some("/guide".to_string()));
    }

    #[test]
    fn test_compute_doc_path_nested() {
        let source_dir = PathBuf::from("/docs");
        let file_path = PathBuf::from("/docs/api/reference.md");

        let result = LiveReloadManager::compute_doc_path(&file_path, &source_dir);
        assert_eq!(result, Some("/api/reference".to_string()));
    }

    #[test]
    fn test_compute_doc_path_index() {
        let source_dir = PathBuf::from("/docs");
        let file_path = PathBuf::from("/docs/guide/index.md");

        let result = LiveReloadManager::compute_doc_path(&file_path, &source_dir);
        assert_eq!(result, Some("/guide".to_string()));
    }

    #[test]
    fn test_compute_doc_path_root_index() {
        let source_dir = PathBuf::from("/docs");
        let file_path = PathBuf::from("/docs/index.md");

        let result = LiveReloadManager::compute_doc_path(&file_path, &source_dir);
        assert_eq!(result, Some("/".to_string()));
    }

    #[test]
    fn test_compute_doc_path_outside_source() {
        let source_dir = PathBuf::from("/docs");
        let file_path = PathBuf::from("/other/guide.md");

        let result = LiveReloadManager::compute_doc_path(&file_path, &source_dir);
        assert_eq!(result, None);
    }
}
