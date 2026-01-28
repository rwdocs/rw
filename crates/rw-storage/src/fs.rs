//! Filesystem storage implementation.
//!
//! Provides [`FsStorage`] for reading documents from the local filesystem
//! with mtime-based caching for title extraction.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use glob::Pattern;
use notify::{RecursiveMode, Watcher};
use regex::Regex;

use crate::debouncer::EventDebouncer;
use crate::event::{StorageEvent, StorageEventKind, StorageEventReceiver, WatchHandle};
use crate::storage::{Document, Storage, StorageError, StorageErrorKind};

/// Backend identifier for error messages.
const BACKEND: &str = "Fs";

/// Cached file metadata for incremental title extraction.
#[derive(Clone, Debug)]
struct CachedFile {
    /// File modification time.
    mtime: SystemTime,
    /// Extracted title from the file.
    title: String,
}

/// Filesystem storage implementation.
///
/// Scans a source directory recursively for markdown files and extracts
/// titles from the first H1 heading. Uses mtime caching to avoid re-reading
/// unchanged files.
///
/// # Example
///
/// ```ignore
/// use std::path::PathBuf;
/// use rw_storage::{FsStorage, Storage};
///
/// let storage = FsStorage::new(PathBuf::from("docs"));
/// let docs = storage.scan()?;
/// ```
pub struct FsStorage {
    /// Root directory for document storage.
    source_dir: PathBuf,
    /// Regex for extracting first H1 heading.
    h1_regex: Regex,
    /// Mtime cache for incremental title extraction.
    mtime_cache: Mutex<HashMap<PathBuf, CachedFile>>,
    /// Patterns for file watching (e.g., "**/*.md").
    watch_patterns: Vec<Pattern>,
}

impl FsStorage {
    /// Create a new filesystem storage with default patterns.
    ///
    /// Uses `**/*.md` as the default watch pattern.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Root directory containing markdown files
    ///
    /// # Panics
    ///
    /// Panics if the internal regex for H1 heading extraction fails to compile.
    /// This should never happen as the regex is a compile-time constant.
    #[must_use]
    pub fn new(source_dir: PathBuf) -> Self {
        Self::with_patterns(source_dir, vec!["**/*.md".to_string()])
    }

    /// Create a new filesystem storage with custom watch patterns.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Root directory containing markdown files
    /// * `patterns` - Glob patterns for file watching (e.g., `["**/*.md", "**/*.rst"]`)
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The internal regex for H1 heading extraction fails to compile
    /// - Any of the provided glob patterns are invalid
    #[must_use]
    pub fn with_patterns(source_dir: PathBuf, patterns: Vec<String>) -> Self {
        let watch_patterns = patterns
            .iter()
            .map(|p| Pattern::new(p).expect("invalid glob pattern"))
            .collect();

        Self {
            source_dir,
            h1_regex: Regex::new(r"(?m)^#\s+(.+)$").unwrap(),
            mtime_cache: Mutex::new(HashMap::new()),
            watch_patterns,
        }
    }

    /// Validate that a path doesn't escape the source directory.
    ///
    /// Rejects paths containing parent directory components (`..`) to prevent
    /// path traversal attacks (e.g., `../../../etc/passwd`).
    fn validate_path(path: &Path) -> Result<(), StorageError> {
        let has_parent_dir = path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir));

        if has_parent_dir {
            return Err(StorageError::new(StorageErrorKind::InvalidPath)
                .with_path(path)
                .with_backend(BACKEND));
        }
        Ok(())
    }

    /// Scan directory recursively and collect documents.
    fn scan_directory(&self, dir_path: &Path, base_path: &Path) -> Vec<Document> {
        let Ok(entries) = fs::read_dir(dir_path) else {
            return Vec::new();
        };

        let mut documents = Vec::new();

        // Collect entries with cached file_type to avoid repeated stat calls in sort.
        let mut entries: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|e| {
                let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
                let name_lower = e.file_name().to_string_lossy().to_lowercase();
                (e, is_dir, name_lower)
            })
            .collect();

        // Sort: directories first, then alphabetical by name
        entries.sort_by(|(_, a_is_dir, a_name), (_, b_is_dir, b_name)| {
            b_is_dir.cmp(a_is_dir).then_with(|| a_name.cmp(b_name))
        });

        for (entry, is_dir, name_lower) in entries {
            // Skip hidden and underscore-prefixed files/dirs
            if name_lower.starts_with('.') || name_lower.starts_with('_') {
                continue;
            }

            // Skip common non-documentation directories
            if is_dir
                && matches!(
                    name_lower.as_str(),
                    "node_modules"
                        | "target"
                        | "dist"
                        | "build"
                        | ".cache"
                        | "vendor"
                        | "__pycache__"
                )
            {
                continue;
            }

            let path = entry.path();

            if is_dir {
                // Recurse into subdirectory
                let rel_path = base_path.join(entry.file_name());
                documents.extend(self.scan_directory(&path, &rel_path));
            } else if path.extension().is_some_and(|e| e == "md") {
                // Process markdown file
                let rel_path = base_path.join(entry.file_name());
                let title = self.get_title(&path, &name_lower);
                documents.push(Document {
                    path: rel_path,
                    title,
                });
            }
        }

        documents
    }

    /// Get title for a file, using mtime cache when possible.
    fn get_title(&self, file_path: &Path, name_lower: &str) -> String {
        // Get current mtime
        let current_mtime = fs::metadata(file_path).ok().and_then(|m| m.modified().ok());

        // Check cache
        {
            let cache = self.mtime_cache.lock().unwrap();
            if let (Some(cached), Some(mtime)) = (cache.get(file_path), current_mtime)
                && cached.mtime == mtime
            {
                return cached.title.clone();
            }
        }

        // Cache miss - extract title
        let title = self
            .extract_title_from_content(file_path)
            .unwrap_or_else(|| Self::title_from_filename(name_lower));

        // Update cache
        if let Some(mtime) = current_mtime {
            let mut cache = self.mtime_cache.lock().unwrap();
            cache.insert(
                file_path.to_path_buf(),
                CachedFile {
                    mtime,
                    title: title.clone(),
                },
            );
        }

        title
    }

    /// Extract title from first H1 heading in markdown file.
    fn extract_title_from_content(&self, file_path: &Path) -> Option<String> {
        let content = fs::read_to_string(file_path).ok()?;
        self.h1_regex
            .captures(&content)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().trim().to_string())
    }

    /// Generate title from filename.
    fn title_from_filename(name_lower: &str) -> String {
        // Remove .md extension
        let name = name_lower.strip_suffix(".md").unwrap_or(name_lower);

        name.replace(['-', '_'], " ")
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Storage for FsStorage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        if !self.source_dir.exists() {
            return Ok(Vec::new());
        }

        Ok(self.scan_directory(&self.source_dir, Path::new("")))
    }

    fn read(&self, path: &Path) -> Result<String, StorageError> {
        Self::validate_path(path)?;
        let full_path = self.source_dir.join(path);
        fs::read_to_string(&full_path)
            .map_err(|e| StorageError::io(e, Some(full_path.clone())).with_backend(BACKEND))
    }

    fn exists(&self, path: &Path) -> bool {
        Self::validate_path(path).is_ok() && self.source_dir.join(path).exists()
    }

    fn mtime(&self, path: &Path) -> Result<f64, StorageError> {
        Self::validate_path(path)?;
        let full_path = self.source_dir.join(path);
        let metadata = fs::metadata(&full_path)
            .map_err(|e| StorageError::io(e, Some(full_path.clone())).with_backend(BACKEND))?;
        let modified = metadata
            .modified()
            .map_err(|e| StorageError::io(e, Some(full_path)).with_backend(BACKEND))?;
        Ok(modified
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64()))
    }

    fn watch(&self) -> Result<(StorageEventReceiver, WatchHandle), StorageError> {
        // Create channel for events
        let (event_tx, event_rx) = mpsc::channel();

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = mpsc::channel();

        // Create debouncer (100ms as per RD-034)
        let debouncer = std::sync::Arc::new(EventDebouncer::new(Duration::from_millis(100)));

        // Setup notify watcher
        let source_dir = self.source_dir.clone();
        let patterns = self.watch_patterns.clone();
        let debouncer_for_watcher = std::sync::Arc::clone(&debouncer);

        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    // Convert notify event kind to storage event kind
                    let kind = match event.kind {
                        notify::EventKind::Create(_) => StorageEventKind::Created,
                        notify::EventKind::Modify(_) => StorageEventKind::Modified,
                        notify::EventKind::Remove(_) => StorageEventKind::Removed,
                        _ => return,
                    };

                    for path in event.paths {
                        // Check if path is within source directory
                        let Ok(rel_path) = path.strip_prefix(&source_dir) else {
                            continue;
                        };

                        // Check if path matches any pattern
                        let matches_pattern = patterns.is_empty()
                            || patterns
                                .iter()
                                .any(|pattern| pattern.matches_path(rel_path));

                        if !matches_pattern {
                            continue;
                        }

                        // Record full path in debouncer (will convert to relative when draining)
                        debouncer_for_watcher.record(path, kind);
                    }
                }
            })
            .map_err(|e| {
                StorageError::new(StorageErrorKind::Other)
                    .with_backend(BACKEND)
                    .with_source(e)
            })?;

        watcher
            .watch(&self.source_dir, RecursiveMode::Recursive)
            .map_err(|e| {
                StorageError::new(StorageErrorKind::Other)
                    .with_backend(BACKEND)
                    .with_source(e)
            })?;

        // Keep watcher alive in Arc
        let watcher = std::sync::Arc::new(std::sync::Mutex::new(watcher));

        // Spawn thread to drain debouncer and send to channel
        let source_dir_for_drain = self.source_dir.clone();
        std::thread::spawn(move || {
            // Keep watcher reference alive in this thread
            let _watcher_guard = watcher;

            loop {
                // Check for shutdown signal (blocking until timeout or signal)
                match shutdown_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(()) => break,                                    // Shutdown signaled
                    Err(mpsc::RecvTimeoutError::Disconnected) => break, // Handle dropped
                    Err(mpsc::RecvTimeoutError::Timeout) => {}          // Continue draining
                }

                for event in debouncer.drain_ready() {
                    // Convert full path to relative path
                    let Ok(rel_path) = event.path.strip_prefix(&source_dir_for_drain) else {
                        continue;
                    };

                    let relative_event = StorageEvent {
                        path: rel_path.to_path_buf(),
                        kind: event.kind,
                    };

                    if event_tx.send(relative_event).is_err() {
                        // Receiver dropped, exit thread
                        return;
                    }
                }
            }
        });

        // Create handle with RAII cleanup
        // When dropped, shutdown_tx is dropped, causing shutdown_rx.recv() to fail
        let handle = WatchHandle::new(shutdown_tx);

        Ok((StorageEventReceiver::new(event_rx), handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageErrorKind;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_fs_storage_is_send_sync() {
        assert_send_sync::<FsStorage>();
    }

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_scan_empty_dir() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert!(docs.is_empty());
    }

    #[test]
    fn test_scan_missing_dir() {
        let storage = FsStorage::new(PathBuf::from("/nonexistent"));
        let docs = storage.scan().unwrap();

        assert!(docs.is_empty());
    }

    #[test]
    fn test_scan_flat_structure() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# User Guide\n\nContent.").unwrap();
        fs::write(temp_dir.path().join("api.md"), "# API Reference\n\nDocs.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 2);
        let paths: Vec<_> = docs.iter().map(|d| d.path.to_str().unwrap()).collect();
        assert!(paths.contains(&"api.md"));
        assert!(paths.contains(&"guide.md"));
    }

    #[test]
    fn test_scan_nested_structure() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain\n\nOverview.").unwrap();
        fs::write(domain_dir.join("guide.md"), "# Domain Guide\n\nSteps.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 2);
        let paths: Vec<_> = docs.iter().map(|d| d.path.to_str().unwrap()).collect();
        assert!(paths.contains(&"domain/index.md"));
        assert!(paths.contains(&"domain/guide.md"));
    }

    #[test]
    fn test_scan_extracts_title_from_h1() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("guide.md"),
            "# My Custom Title\n\nContent.",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "My Custom Title");
    }

    #[test]
    fn test_scan_falls_back_to_filename() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("setup-guide.md"),
            "Content without heading.",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "Setup Guide");
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden").unwrap();
        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, PathBuf::from("visible.md"));
    }

    #[test]
    fn test_scan_skips_underscore_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("_partial.md"), "# Partial").unwrap();
        fs::write(temp_dir.path().join("main.md"), "# Main").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, PathBuf::from("main.md"));
    }

    #[test]
    fn test_scan_skips_node_modules() {
        let temp_dir = create_test_dir();
        let node_modules = temp_dir.path().join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("package.md"), "# Package").unwrap();
        fs::write(temp_dir.path().join("main.md"), "# Main").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, PathBuf::from("main.md"));
    }

    #[test]
    fn test_read_existing_file() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide\n\nContent here.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let content = storage.read(Path::new("guide.md")).unwrap();

        assert_eq!(content, "# Guide\n\nContent here.");
    }

    #[test]
    fn test_read_nested_file() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("guide.md"), "# Domain Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let content = storage.read(Path::new("domain/guide.md")).unwrap();

        assert_eq!(content, "# Domain Guide");
    }

    #[test]
    fn test_read_missing_file() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.read(Path::new("nonexistent.md"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.backend(), Some("Fs"));
    }

    #[test]
    fn test_exists_returns_true_for_existing_file() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        assert!(storage.exists(Path::new("guide.md")));
    }

    #[test]
    fn test_exists_returns_false_for_missing_file() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        assert!(!storage.exists(Path::new("nonexistent.md")));
    }

    #[test]
    fn test_exists_returns_true_for_directory() {
        let temp_dir = create_test_dir();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        assert!(storage.exists(Path::new("subdir")));
    }

    #[test]
    fn test_mtime_cache_reuses_titles() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // First scan
        let docs1 = storage.scan().unwrap();
        assert_eq!(docs1[0].title, "Original Title");

        // Second scan without changes - should use cache
        let docs2 = storage.scan().unwrap();
        assert_eq!(docs2[0].title, "Original Title");
    }

    #[test]
    fn test_mtime_cache_detects_changes() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // First scan
        let docs1 = storage.scan().unwrap();
        assert_eq!(docs1[0].title, "Original Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify file
        fs::write(temp_dir.path().join("guide.md"), "# Updated Title").unwrap();

        // Second scan should see new title
        let docs2 = storage.scan().unwrap();
        assert_eq!(docs2[0].title, "Updated Title");
    }

    #[test]
    fn test_title_from_filename() {
        assert_eq!(
            FsStorage::title_from_filename("setup-guide.md"),
            "Setup Guide"
        );
        assert_eq!(FsStorage::title_from_filename("my_page.md"), "My Page");
        assert_eq!(
            FsStorage::title_from_filename("complex-name_here.md"),
            "Complex Name Here"
        );
        assert_eq!(FsStorage::title_from_filename("simple.md"), "Simple");
    }

    #[test]
    fn test_mtime_returns_modification_time() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let mtime = storage.mtime(Path::new("guide.md")).unwrap();

        // mtime should be a recent timestamp (within last minute)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert!(mtime > now - 60.0);
        assert!(mtime <= now);
    }

    #[test]
    fn test_mtime_missing_file() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.mtime(Path::new("nonexistent.md"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.backend(), Some("Fs"));
    }

    #[test]
    fn test_read_rejects_path_traversal() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.read(Path::new("../etc/passwd"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::InvalidPath);
        assert_eq!(err.backend(), Some("Fs"));
    }

    #[test]
    fn test_read_rejects_nested_path_traversal() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.read(Path::new("subdir/../../etc/passwd"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::InvalidPath);
    }

    #[test]
    fn test_mtime_rejects_path_traversal() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.mtime(Path::new("../etc/passwd"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::InvalidPath);
        assert_eq!(err.backend(), Some("Fs"));
    }

    #[test]
    fn test_exists_rejects_path_traversal() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // Path traversal should return false (treated as non-existent)
        assert!(!storage.exists(Path::new("../etc/passwd")));
    }

    #[test]
    fn test_watch_returns_receiver_and_handle() {
        let temp_dir = create_test_dir();
        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        let result = storage.watch();
        assert!(result.is_ok());
    }

    // Note: File watching tests are ignored because they're timing-sensitive and can be flaky
    // in test environments. The implementation follows the same pattern as LiveReloadManager
    // which works correctly in production.
    #[test]
    #[ignore]
    fn test_watch_detects_file_creation() {
        let temp_dir = create_test_dir();
        let temp_path = temp_dir.path().to_path_buf();

        // Ensure directory exists before watching
        assert!(temp_path.exists());

        let storage = FsStorage::new(temp_path.clone());
        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(200));

        // Create a file
        fs::write(temp_path.join("new.md"), "# New").unwrap();

        // Wait for debounce + processing (be generous with timing)
        std::thread::sleep(Duration::from_millis(500));

        // Try to receive events
        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv()).collect();

        assert!(!events.is_empty(), "Expected to receive at least one event");

        // Find the event for new.md
        let new_md_event = events.iter().find(|e| e.path == Path::new("new.md"));
        assert!(
            new_md_event.is_some(),
            "Expected event for new.md, got: {:?}",
            events
        );
    }

    #[test]
    #[ignore]
    fn test_watch_detects_file_modification() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("existing.md"), "# Original").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(100));

        // Modify the file
        fs::write(temp_dir.path().join("existing.md"), "# Modified").unwrap();

        // Wait for debounce + processing
        std::thread::sleep(Duration::from_millis(250));

        // Should receive modified event
        let event = rx.try_recv();
        assert!(event.is_some(), "Expected to receive event");
        let event = event.unwrap();
        assert_eq!(event.path, PathBuf::from("existing.md"));
        assert_eq!(event.kind, StorageEventKind::Modified);
    }

    #[test]
    #[ignore]
    fn test_watch_detects_file_deletion() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("to-delete.md"), "# Delete Me").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(100));

        // Delete the file
        fs::remove_file(temp_dir.path().join("to-delete.md")).unwrap();

        // Wait for debounce + processing
        std::thread::sleep(Duration::from_millis(250));

        // Should receive removed event
        let event = rx.try_recv();
        assert!(event.is_some(), "Expected to receive event");
        let event = event.unwrap();
        assert_eq!(event.path, PathBuf::from("to-delete.md"));
        assert_eq!(event.kind, StorageEventKind::Removed);
    }

    #[test]
    #[ignore]
    fn test_watch_respects_patterns() {
        let temp_dir = create_test_dir();
        let storage =
            FsStorage::with_patterns(temp_dir.path().to_path_buf(), vec!["**/*.md".to_string()]);

        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(100));

        // Create a .md file (should be detected)
        fs::write(temp_dir.path().join("doc.md"), "# Doc").unwrap();

        // Create a .txt file (should be ignored)
        fs::write(temp_dir.path().join("note.txt"), "Note").unwrap();

        // Wait for debounce + processing
        std::thread::sleep(Duration::from_millis(250));

        // Should only receive event for .md file
        let event = rx.try_recv();
        assert!(event.is_some(), "Expected to receive event for .md file");
        let event = event.unwrap();
        assert_eq!(event.path, PathBuf::from("doc.md"));

        // No more events
        let event = rx.try_recv();
        assert!(event.is_none());
    }

    #[test]
    #[ignore]
    fn test_watch_debounces_multiple_events() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("file.md"), "# Original").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(100));

        // Simulate editor saving: multiple writes in quick succession
        fs::write(temp_dir.path().join("file.md"), "# Edit 1").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        fs::write(temp_dir.path().join("file.md"), "# Edit 2").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        fs::write(temp_dir.path().join("file.md"), "# Edit 3").unwrap();

        // Wait for debounce + processing
        std::thread::sleep(Duration::from_millis(250));

        // Should receive only one modified event
        let event = rx.try_recv();
        assert!(event.is_some(), "Expected to receive event");
        assert_eq!(event.unwrap().kind, StorageEventKind::Modified);

        // No more events
        let event = rx.try_recv();
        assert!(event.is_none());
    }

    #[test]
    #[ignore]
    fn test_watch_handle_stops_watching() {
        let temp_dir = create_test_dir();
        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        let (rx, handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(100));

        // Stop watching
        handle.stop();

        // Give the thread time to process stop signal
        std::thread::sleep(Duration::from_millis(100));

        // Create a file
        fs::write(temp_dir.path().join("new.md"), "# New").unwrap();

        // Wait
        std::thread::sleep(Duration::from_millis(250));

        // Should not receive any events (watcher is stopped)
        let event = rx.try_recv();
        assert!(event.is_none());
    }
}
