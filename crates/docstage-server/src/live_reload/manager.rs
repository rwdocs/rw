//! Live reload manager.
//!
//! Coordinates file watching and WebSocket broadcasting for live reload.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use docstage_core::SiteLoader;

/// Event sent to connected WebSocket clients when files change.
#[derive(Clone, Debug, Serialize)]
pub struct ReloadEvent {
    /// Event type (always "reload").
    #[serde(rename = "type")]
    pub event_type: String,
    /// Documentation path that changed.
    pub path: String,
}

/// Manages file watching and broadcasting reload events.
pub struct LiveReloadManager {
    source_dir: PathBuf,
    watch_patterns: Vec<String>,
    site_loader: Arc<RwLock<SiteLoader>>,
    broadcaster: broadcast::Sender<ReloadEvent>,
    watcher: Option<RecommendedWatcher>,
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
    pub fn new(
        source_dir: PathBuf,
        watch_patterns: Option<Vec<String>>,
        site_loader: Arc<RwLock<SiteLoader>>,
        broadcaster: broadcast::Sender<ReloadEvent>,
    ) -> Self {
        Self {
            source_dir,
            watch_patterns: watch_patterns.unwrap_or_else(|| vec!["**/*.md".to_string()]),
            site_loader,
            broadcaster,
            watcher: None,
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
    pub fn start(&mut self) -> Result<(), notify::Error> {
        let (tx, mut rx) = mpsc::channel::<Event>(100);
        let source_dir = self.source_dir.clone();

        // Create watcher with callback that sends events to channel
        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // Use blocking_send since callback is sync
                let _ = tx.blocking_send(event);
            }
        })?;

        // Store watcher to keep it alive
        let mut watcher = watcher;
        watcher.watch(&source_dir, RecursiveMode::Recursive)?;
        self.watcher = Some(watcher);

        // Spawn task to process events
        let watch_patterns = self.watch_patterns.clone();
        let site_loader = Arc::clone(&self.site_loader);
        let broadcaster = self.broadcaster.clone();
        let source_dir_clone = self.source_dir.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                Self::handle_event(
                    &event,
                    &source_dir_clone,
                    &watch_patterns,
                    &site_loader,
                    &broadcaster,
                );
            }
        });

        Ok(())
    }

    /// Handle a file system event.
    fn handle_event(
        event: &Event,
        source_dir: &Path,
        watch_patterns: &[String],
        site_loader: &Arc<RwLock<SiteLoader>>,
        broadcaster: &broadcast::Sender<ReloadEvent>,
    ) {
        // Filter to only modify/create events
        use notify::EventKind;
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {}
            _ => return,
        }

        for path in &event.paths {
            // Check if path matches watch patterns
            if !Self::matches_patterns(path, source_dir, watch_patterns) {
                continue;
            }

            // Invalidate site cache first
            site_loader.write().unwrap().invalidate();

            // Resolve doc path
            if let Some(doc_path) = Self::resolve_doc_path(path, source_dir, site_loader) {
                // Broadcast reload event
                let reload_event = ReloadEvent {
                    event_type: "reload".to_string(),
                    path: doc_path,
                };
                let _ = broadcaster.send(reload_event);
            }
        }
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
    fn resolve_doc_path(
        file_path: &Path,
        source_dir: &Path,
        site_loader: &Arc<RwLock<SiteLoader>>,
    ) -> Option<String> {
        let relative = file_path.strip_prefix(source_dir).ok()?;

        let mut loader = site_loader.write().unwrap();
        let site = loader.load(true);
        let page = site.get_page_by_source(relative)?;

        Some(page.path.clone())
    }

    /// Get a receiver for reload events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ReloadEvent> {
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
}
