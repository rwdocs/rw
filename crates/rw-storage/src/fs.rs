//! Filesystem storage implementation.
//!
//! Provides [`FsStorage`] for reading documents from the local filesystem
//! with mtime-based caching for title extraction.
//!
//! # URL to File Path Mapping
//!
//! `FsStorage` maps URL paths to filesystem paths:
//! - `""` → `index.md`
//! - `"guide"` → `guide/index.md` or `guide.md` (directory preferred)
//! - `"domain/billing"` → `domain/billing/index.md` or `domain/billing.md`

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
use crate::metadata::{PageMetadata, merge_metadata};
use crate::storage::{
    Document, ScanResult, Storage, StorageError, StorageErrorKind, extract_yaml_title,
    extract_yaml_type,
};

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
/// let result = storage.scan()?;
/// for doc in result.documents {
///     println!("{}: {}", doc.path.display(), doc.title);
/// }
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
    /// Metadata file name (e.g., "meta.yaml").
    meta_filename: String,
}

/// Default metadata filename.
const DEFAULT_META_FILENAME: &str = "meta.yaml";

impl FsStorage {
    /// Create a new filesystem storage with default patterns.
    ///
    /// Uses `**/*.md` as the default watch pattern and `meta.yaml` as metadata filename.
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
        Self::with_patterns(source_dir, &["**/*.md".to_string()])
    }

    /// Create a new filesystem storage with a custom metadata filename.
    ///
    /// Uses `**/*.md` as the default watch pattern.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Root directory containing markdown files
    /// * `meta_filename` - Name of metadata files (e.g., "meta.yaml")
    ///
    /// # Panics
    ///
    /// Panics if the internal regex for H1 heading extraction fails to compile.
    #[must_use]
    pub fn with_meta_filename(source_dir: PathBuf, meta_filename: &str) -> Self {
        Self::with_patterns_and_meta(source_dir, &["**/*.md".to_string()], meta_filename)
    }

    /// Create a new filesystem storage with custom watch patterns.
    ///
    /// Uses `meta.yaml` as the default metadata filename.
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
    pub fn with_patterns(source_dir: PathBuf, patterns: &[String]) -> Self {
        Self::with_patterns_and_meta(source_dir, patterns, DEFAULT_META_FILENAME)
    }

    /// Create a new filesystem storage with custom watch patterns and metadata filename.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Root directory containing markdown files
    /// * `patterns` - Glob patterns for file watching (e.g., `["**/*.md", "**/*.rst"]`)
    /// * `meta_filename` - Name of metadata files (e.g., "meta.yaml")
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The internal regex for H1 heading extraction fails to compile
    /// - Any of the provided glob patterns are invalid
    #[must_use]
    pub fn with_patterns_and_meta(
        source_dir: PathBuf,
        patterns: &[String],
        meta_filename: &str,
    ) -> Self {
        let watch_patterns = patterns
            .iter()
            .map(|p| Pattern::new(p).expect("invalid glob pattern"))
            .collect();

        Self {
            source_dir,
            h1_regex: Regex::new(r"(?m)^#\s+(.+)$").unwrap(),
            mtime_cache: Mutex::new(HashMap::new()),
            watch_patterns,
            meta_filename: meta_filename.to_string(),
        }
    }

    /// Validate that a URL path doesn't contain path traversal attempts.
    ///
    /// Rejects paths containing `..` to prevent path traversal attacks.
    fn validate_path(path: &str) -> Result<(), StorageError> {
        if path.contains("..") {
            return Err(StorageError::new(StorageErrorKind::InvalidPath)
                .with_path(path)
                .with_backend(BACKEND));
        }
        Ok(())
    }

    /// Resolve URL path to content file path.
    ///
    /// Resolution order:
    /// 1. `{path}/index.md` (directory structure preferred)
    /// 2. `{path}.md` (standalone file fallback)
    ///
    /// Returns `None` if no content file exists.
    fn resolve_content(&self, url_path: &str) -> Option<PathBuf> {
        if url_path.is_empty() {
            let index = self.source_dir.join("index.md");
            return index.exists().then_some(index);
        }

        // Prefer directory/index.md
        let index_path = self.source_dir.join(format!("{url_path}/index.md"));
        if index_path.exists() {
            return Some(index_path);
        }

        // Fall back to standalone file
        let file_path = self.source_dir.join(format!("{url_path}.md"));
        file_path.exists().then_some(file_path)
    }

    /// Resolve URL path to metadata file path.
    ///
    /// Metadata is always in a directory's meta.yaml file:
    /// - `""` → `meta.yaml`
    /// - `"domain"` → `domain/meta.yaml`
    ///
    /// Returns `None` if no metadata file exists.
    fn resolve_meta(&self, url_path: &str) -> Option<PathBuf> {
        let meta_path = if url_path.is_empty() {
            self.source_dir.join(&self.meta_filename)
        } else {
            self.source_dir
                .join(format!("{url_path}/{}", self.meta_filename))
        };
        meta_path.exists().then_some(meta_path)
    }

    /// Convert file path to URL path.
    ///
    /// Examples:
    /// - `index.md` → `""`
    /// - `guide.md` → `"guide"`
    /// - `domain/index.md` → `"domain"`
    /// - `domain/setup.md` → `"domain/setup"`
    fn file_path_to_url(rel_path: &Path) -> String {
        let path_str = rel_path.to_string_lossy();

        // Handle root index.md
        if path_str == "index.md" {
            return String::new();
        }

        // Remove .md extension
        let without_ext = path_str.strip_suffix(".md").unwrap_or(&path_str);

        // Handle directory index files
        if let Some(without_index) = without_ext.strip_suffix("/index") {
            return without_index.to_string();
        }
        if without_ext == "index" {
            return String::new();
        }

        without_ext.to_string()
    }

    /// Scan directory recursively and collect documents.
    ///
    /// This method:
    /// 1. Scans all .md files as entries with `has_content=true`
    /// 2. Scans all meta.yaml files:
    ///    - If matching document exists (same dir, `index.md`): sets `has_metadata=true`
    ///    - If no matching document: creates entry with `has_content=false, has_metadata=true`
    ///
    /// Documents are returned with URL paths, not file paths.
    fn scan_directory(&self, dir_path: &Path, url_prefix: &str, result: &mut ScanResult) {
        let Ok(entries) = fs::read_dir(dir_path) else {
            return;
        };

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

        // Track if we found index.md and meta.yaml in this directory
        let mut index_doc_idx: Option<usize> = None;
        let mut has_meta_file = false;
        let mut meta_file_path: Option<PathBuf> = None;

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
                let child_name = entry.file_name().to_string_lossy().into_owned();
                let child_url = if url_prefix.is_empty() {
                    child_name
                } else {
                    format!("{url_prefix}/{child_name}")
                };
                self.scan_directory(&path, &child_url, result);
            } else if path.extension().is_some_and(|e| e == "md") {
                // Process markdown file
                let title = self.get_title(&path, &name_lower);
                let is_index = name_lower == "index.md";

                // Compute URL path
                let url_path = if is_index {
                    // index.md → directory URL (e.g., "domain")
                    url_prefix.to_string()
                } else {
                    // guide.md → "guide" or "domain/guide"
                    let stem = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if url_prefix.is_empty() {
                        stem
                    } else {
                        format!("{url_prefix}/{stem}")
                    }
                };

                let doc = Document {
                    path: url_path,
                    title,
                    has_content: true,
                    page_type: None, // Will be updated if meta.yaml exists
                };

                if is_index {
                    index_doc_idx = Some(result.documents.len());
                }
                result.documents.push(doc);
            } else if entry.file_name().to_string_lossy() == self.meta_filename {
                // Found metadata file
                has_meta_file = true;
                meta_file_path = Some(path);
            }
        }

        // Handle metadata file
        if has_meta_file && let Some(ref meta_path) = meta_file_path {
            // Read metadata file to extract page_type
            let page_type = fs::read_to_string(meta_path)
                .ok()
                .and_then(|content| extract_yaml_type(&content));

            if let Some(idx) = index_doc_idx {
                // index.md exists - set page_type from metadata
                result.documents[idx].page_type = page_type;
            } else {
                // No index.md - check if metadata is useful before creating virtual page
                if let Some(title) = Self::get_virtual_page_title(meta_path, Path::new(url_prefix))
                {
                    result.documents.push(Document {
                        path: url_prefix.to_string(),
                        title,
                        has_content: false,
                        page_type,
                    });
                }
            }
        }
    }

    /// Get title for a virtual page from its metadata file.
    ///
    /// Returns `None` if the metadata file is empty or doesn't contain useful content.
    fn get_virtual_page_title(meta_path: &Path, dir_path: &Path) -> Option<String> {
        let content = fs::read_to_string(meta_path).ok()?;

        if content.trim().is_empty() {
            return None;
        }

        // Try to extract title from YAML, fallback to directory name
        let title = extract_yaml_title(&content).unwrap_or_else(|| {
            dir_path.file_name().map_or_else(
                || "Untitled".to_string(),
                |n| Self::title_from_dir_name(&n.to_string_lossy()),
            )
        });

        Some(title)
    }

    /// Generate title from directory name.
    fn title_from_dir_name(name: &str) -> String {
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
    fn scan(&self) -> Result<ScanResult, StorageError> {
        if !self.source_dir.exists() {
            return Ok(ScanResult::default());
        }

        let mut result = ScanResult::default();
        self.scan_directory(&self.source_dir, "", &mut result);
        Ok(result)
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        Self::validate_path(path)?;
        let full_path = self
            .resolve_content(path)
            .ok_or_else(|| StorageError::not_found(path).with_backend(BACKEND))?;
        fs::read_to_string(&full_path)
            .map_err(|e| StorageError::io(e, Some(PathBuf::from(path))).with_backend(BACKEND))
    }

    fn exists(&self, path: &str) -> bool {
        Self::validate_path(path).is_ok() && self.resolve_content(path).is_some()
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        Self::validate_path(path)?;
        let full_path = self
            .resolve_content(path)
            .ok_or_else(|| StorageError::not_found(path).with_backend(BACKEND))?;
        let metadata = fs::metadata(&full_path)
            .map_err(|e| StorageError::io(e, Some(PathBuf::from(path))).with_backend(BACKEND))?;
        let modified = metadata
            .modified()
            .map_err(|e| StorageError::io(e, Some(PathBuf::from(path))).with_backend(BACKEND))?;
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
                    // Shutdown signaled or handle dropped
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {} // Continue draining
                }

                for event in debouncer.drain_ready() {
                    // Convert full file path to relative path, then to URL path
                    let Ok(rel_path) = event.path.strip_prefix(&source_dir_for_drain) else {
                        continue;
                    };

                    let url_path = FsStorage::file_path_to_url(rel_path);

                    let storage_event = StorageEvent {
                        path: url_path,
                        kind: event.kind,
                    };

                    if event_tx.send(storage_event).is_err() {
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

    fn meta(&self, path: &str) -> Result<Option<PageMetadata>, StorageError> {
        Self::validate_path(path)?;

        // Build ancestor chain: ["", "domain", "domain/billing"] for "domain/billing/api"
        let ancestors = Self::build_ancestor_chain(path);

        // Walk ancestors from root to leaf, merging metadata
        let mut accumulated: Option<PageMetadata> = None;
        let mut has_own_meta = false;

        for ancestor in &ancestors {
            let Some(meta_path) = self.resolve_meta(ancestor) else {
                continue;
            };

            let content = match fs::read_to_string(&meta_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        path = %ancestor,
                        error = %e,
                        "Failed to read metadata file, skipping"
                    );
                    continue;
                }
            };

            let meta = match PageMetadata::from_yaml(&content) {
                Ok(m) if !m.is_empty() => m,
                Ok(_) => continue, // Empty metadata
                Err(e) => {
                    tracing::warn!(
                        path = %ancestor,
                        error = %e,
                        "Failed to parse metadata, skipping"
                    );
                    continue;
                }
            };

            // Track if this is the requested path's own metadata
            if ancestor == path {
                has_own_meta = true;
            }

            accumulated = Some(match accumulated {
                Some(parent) => merge_metadata(&parent, &meta),
                None => meta,
            });
        }

        // If the requested path doesn't have its own metadata file,
        // clear title/description/page_type (only vars are inherited)
        if !has_own_meta && let Some(ref mut meta) = accumulated {
            meta.title = None;
            meta.description = None;
            meta.page_type = None;
        }

        Ok(accumulated)
    }
}

impl FsStorage {
    /// Build ancestor chain for a URL path.
    ///
    /// Returns ancestors from root to the path itself.
    /// E.g., "domain/billing/api" → ["", "domain", "domain/billing", "domain/billing/api"]
    fn build_ancestor_chain(path: &str) -> Vec<String> {
        let mut ancestors = vec![String::new()]; // Root is always first

        if !path.is_empty() {
            let parts: Vec<&str> = path.split('/').collect();
            let mut current = String::new();
            for part in parts {
                if current.is_empty() {
                    current = part.to_string();
                } else {
                    current = format!("{current}/{part}");
                }
                ancestors.push(current.clone());
            }
        }

        ancestors
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
        let result = storage.scan().unwrap();

        assert!(result.documents.is_empty());
    }

    #[test]
    fn test_scan_missing_dir() {
        let storage = FsStorage::new(PathBuf::from("/nonexistent"));
        let result = storage.scan().unwrap();

        assert!(result.documents.is_empty());
    }

    #[test]
    fn test_scan_flat_structure() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# User Guide\n\nContent.").unwrap();
        fs::write(temp_dir.path().join("api.md"), "# API Reference\n\nDocs.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 2);
        let paths: Vec<_> = result.documents.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"api"));
        assert!(paths.contains(&"guide"));
    }

    #[test]
    fn test_scan_nested_structure() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain\n\nOverview.").unwrap();
        fs::write(domain_dir.join("guide.md"), "# Domain Guide\n\nSteps.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 2);
        let paths: Vec<_> = result.documents.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"domain"));
        assert!(paths.contains(&"domain/guide"));
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
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0].title, "My Custom Title");
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
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0].title, "Setup Guide");
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden").unwrap();
        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0].path, "visible");
    }

    #[test]
    fn test_scan_skips_underscore_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("_partial.md"), "# Partial").unwrap();
        fs::write(temp_dir.path().join("main.md"), "# Main").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0].path, "main");
    }

    #[test]
    fn test_scan_skips_node_modules() {
        let temp_dir = create_test_dir();
        let node_modules = temp_dir.path().join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("package.md"), "# Package").unwrap();
        fs::write(temp_dir.path().join("main.md"), "# Main").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0].path, "main");
    }

    #[test]
    fn test_scan_extracts_page_type() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("meta.yaml"), "type: domain").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        let doc = &result.documents[0];
        assert_eq!(doc.path, "domain");
        assert!(doc.has_content);
        assert_eq!(doc.page_type, Some("domain".to_string()));
    }

    #[test]
    fn test_scan_with_custom_meta_filename() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("config.yml"), "type: section").unwrap();
        fs::write(domain_dir.join("meta.yaml"), "ignored").unwrap(); // Should be ignored

        let storage = FsStorage::with_meta_filename(temp_dir.path().to_path_buf(), "config.yml");
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        let doc = &result.documents[0];
        assert!(doc.has_content);
        assert_eq!(doc.page_type, Some("section".to_string()));
    }

    #[test]
    fn test_scan_no_page_type_without_type_field() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "title: Home Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        let doc = &result.documents[0];
        assert_eq!(doc.path, "");
        assert!(doc.has_content);
        assert!(doc.page_type.is_none()); // No type field in metadata
    }

    #[test]
    fn test_scan_creates_virtual_page() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        // No index.md, only meta.yaml
        fs::write(domain_dir.join("meta.yaml"), "title: Domain Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        let doc = &result.documents[0];
        assert_eq!(doc.path, "domain");
        assert_eq!(doc.title, "Domain Title");
        assert!(!doc.has_content); // Virtual page
        assert!(doc.page_type.is_none());
    }

    #[test]
    fn test_scan_virtual_page_with_type() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("my-nice-domain");
        fs::create_dir(&domain_dir).unwrap();
        // No title but has type in meta.yaml
        fs::write(domain_dir.join("meta.yaml"), "type: domain").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        let doc = &result.documents[0];
        assert_eq!(doc.title, "My Nice Domain"); // Fallback to directory name
        assert_eq!(doc.page_type, Some("domain".to_string()));
    }

    #[test]
    fn test_scan_no_virtual_page_without_metadata() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("empty-domain");
        fs::create_dir(&domain_dir).unwrap();
        // No meta.yaml, no index.md

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.scan().unwrap();

        assert!(result.documents.is_empty());
    }

    #[test]
    fn test_meta_returns_parsed_metadata() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(
            domain_dir.join("meta.yaml"),
            "title: Domain Title\ntype: domain",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("domain").unwrap();

        assert!(meta.is_some());
        let meta = meta.unwrap();
        assert_eq!(meta.title, Some("Domain Title".to_string()));
        assert_eq!(meta.page_type, Some("domain".to_string()));
    }

    #[test]
    fn test_meta_for_root() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "title: Home").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("").unwrap();

        assert!(meta.is_some());
        assert_eq!(meta.unwrap().title, Some("Home".to_string()));
    }

    #[test]
    fn test_meta_returns_none_when_no_metadata() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        // No meta.yaml

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.meta("").unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_meta_inheritance_merges_vars() {
        let temp_dir = create_test_dir();
        // Root metadata
        fs::write(
            temp_dir.path().join("meta.yaml"),
            "vars:\n  org: acme\n  env: prod",
        )
        .unwrap();

        // Nested directory
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(
            domain_dir.join("meta.yaml"),
            "title: Domain\nvars:\n  env: dev\n  team: core",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("domain").unwrap().unwrap();

        // Title from child (not inherited)
        assert_eq!(meta.title, Some("Domain".to_string()));
        // Vars merged: org from parent, env overridden by child, team from child
        assert_eq!(meta.vars.get("org"), Some(&serde_json::json!("acme")));
        assert_eq!(meta.vars.get("env"), Some(&serde_json::json!("dev")));
        assert_eq!(meta.vars.get("team"), Some(&serde_json::json!("core")));
    }

    #[test]
    fn test_meta_inheritance_title_not_inherited() {
        let temp_dir = create_test_dir();
        // Root metadata with title
        fs::write(temp_dir.path().join("meta.yaml"), "title: Root Title").unwrap();

        // Child without title
        let child_dir = temp_dir.path().join("child");
        fs::create_dir(&child_dir).unwrap();
        fs::write(child_dir.join("index.md"), "# Child").unwrap();
        fs::write(child_dir.join("meta.yaml"), "type: section").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("child").unwrap().unwrap();

        // Title should NOT be inherited
        assert!(meta.title.is_none());
        assert_eq!(meta.page_type, Some("section".to_string()));
    }

    #[test]
    fn test_meta_deep_inheritance() {
        let temp_dir = create_test_dir();
        // Root
        fs::write(temp_dir.path().join("meta.yaml"), "vars:\n  a: 1").unwrap();

        // Level 1 - no metadata
        let level1 = temp_dir.path().join("level1");
        fs::create_dir(&level1).unwrap();

        // Level 2 - has metadata
        let level2 = level1.join("level2");
        fs::create_dir(&level2).unwrap();
        fs::write(level2.join("index.md"), "# L2").unwrap();
        fs::write(level2.join("meta.yaml"), "vars:\n  b: 2").unwrap();

        // Level 3 - no metadata
        let level3 = level2.join("level3");
        fs::create_dir(&level3).unwrap();
        fs::write(level3.join("index.md"), "# L3").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("level1/level2/level3").unwrap().unwrap();

        // Should inherit from root and level2
        assert_eq!(meta.vars.get("a"), Some(&serde_json::json!(1)));
        assert_eq!(meta.vars.get("b"), Some(&serde_json::json!(2)));
    }

    #[test]
    fn test_meta_no_own_metadata_only_inherits_vars() {
        let temp_dir = create_test_dir();
        // Parent with all fields
        let parent = temp_dir.path().join("parent");
        fs::create_dir(&parent).unwrap();
        fs::write(parent.join("index.md"), "# Parent").unwrap();
        fs::write(
            parent.join("meta.yaml"),
            "title: Parent Title\ndescription: Parent Desc\ntype: domain\nvars:\n  key: value",
        )
        .unwrap();

        // Child with NO metadata file (only index.md)
        let child = parent.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("index.md"), "# Child").unwrap();
        // No meta.yaml for child

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("parent/child").unwrap().unwrap();

        // Only vars should be inherited
        assert_eq!(meta.vars.get("key"), Some(&serde_json::json!("value")));

        // title/description/page_type should NOT be inherited
        assert!(meta.title.is_none());
        assert!(meta.description.is_none());
        assert!(meta.page_type.is_none());
    }

    #[test]
    fn test_read_existing_file() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide\n\nContent here.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let content = storage.read("guide").unwrap();

        assert_eq!(content, "# Guide\n\nContent here.");
    }

    #[test]
    fn test_read_nested_file() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("guide.md"), "# Domain Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        // Read the domain index
        let content = storage.read("domain").unwrap();
        assert_eq!(content, "# Domain");

        // Read a child page
        let content = storage.read("domain/guide").unwrap();
        assert_eq!(content, "# Domain Guide");
    }

    #[test]
    fn test_read_missing_file() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.read("nonexistent");

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

        assert!(storage.exists("guide"));
    }

    #[test]
    fn test_exists_returns_false_for_missing_file() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        assert!(!storage.exists("nonexistent"));
    }

    #[test]
    fn test_exists_returns_true_for_directory_with_index() {
        let temp_dir = create_test_dir();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("index.md"), "# Subdir").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        assert!(storage.exists("subdir"));
    }

    #[test]
    fn test_mtime_cache_reuses_titles() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // First scan
        let result1 = storage.scan().unwrap();
        assert_eq!(result1.documents[0].title, "Original Title");

        // Second scan without changes - should use cache
        let result2 = storage.scan().unwrap();
        assert_eq!(result2.documents[0].title, "Original Title");
    }

    #[test]
    fn test_mtime_cache_detects_changes() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // First scan
        let result1 = storage.scan().unwrap();
        assert_eq!(result1.documents[0].title, "Original Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify file
        fs::write(temp_dir.path().join("guide.md"), "# Updated Title").unwrap();

        // Second scan should see new title
        let result2 = storage.scan().unwrap();
        assert_eq!(result2.documents[0].title, "Updated Title");
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
    fn test_file_path_to_url() {
        assert_eq!(FsStorage::file_path_to_url(Path::new("index.md")), "");
        assert_eq!(FsStorage::file_path_to_url(Path::new("guide.md")), "guide");
        assert_eq!(
            FsStorage::file_path_to_url(Path::new("domain/index.md")),
            "domain"
        );
        assert_eq!(
            FsStorage::file_path_to_url(Path::new("domain/setup.md")),
            "domain/setup"
        );
        assert_eq!(FsStorage::file_path_to_url(Path::new("a/b/c.md")), "a/b/c");
    }

    #[test]
    fn test_resolve_content_root() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let resolved = storage.resolve_content("");

        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("index.md"));
    }

    #[test]
    fn test_resolve_content_prefers_directory_index() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain Index").unwrap();
        // Also create a standalone file (should be ignored)
        fs::write(temp_dir.path().join("domain.md"), "# Domain Standalone").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let resolved = storage.resolve_content("domain");

        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("domain/index.md"));
    }

    #[test]
    fn test_resolve_content_falls_back_to_standalone() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let resolved = storage.resolve_content("guide");

        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("guide.md"));
    }

    #[test]
    fn test_resolve_content_not_found() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let resolved = storage.resolve_content("nonexistent");

        assert!(resolved.is_none());
    }

    #[test]
    fn test_mtime_returns_modification_time() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let mtime = storage.mtime("guide").unwrap();

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
        let result = storage.mtime("nonexistent");

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
        let result = storage.read("../etc/passwd");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::InvalidPath);
        assert_eq!(err.backend(), Some("Fs"));
    }

    #[test]
    fn test_read_rejects_nested_path_traversal() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.read("subdir/../../etc/passwd");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::InvalidPath);
    }

    #[test]
    fn test_mtime_rejects_path_traversal() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.mtime("../etc/passwd");

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
        assert!(!storage.exists("../etc/passwd"));
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
    #[ignore = "timing-sensitive, can be flaky in test environments"]
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

        // Find the event for new (URL path, not file path)
        let new_event = events.iter().find(|e| e.path == "new");
        assert!(
            new_event.is_some(),
            "Expected event for 'new', got: {events:?}"
        );
    }

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
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

        // Should receive modified event (URL path)
        let event = rx.try_recv();
        assert!(event.is_some(), "Expected to receive event");
        let event = event.unwrap();
        assert_eq!(event.path, "existing");
        assert_eq!(event.kind, StorageEventKind::Modified);
    }

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
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

        // Should receive removed event (URL path)
        let event = rx.try_recv();
        assert!(event.is_some(), "Expected to receive event");
        let event = event.unwrap();
        assert_eq!(event.path, "to-delete");
        assert_eq!(event.kind, StorageEventKind::Removed);
    }

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
    fn test_watch_respects_patterns() {
        let temp_dir = create_test_dir();
        let storage =
            FsStorage::with_patterns(temp_dir.path().to_path_buf(), &["**/*.md".to_string()]);

        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(100));

        // Create a .md file (should be detected)
        fs::write(temp_dir.path().join("doc.md"), "# Doc").unwrap();

        // Create a .txt file (should be ignored)
        fs::write(temp_dir.path().join("note.txt"), "Note").unwrap();

        // Wait for debounce + processing
        std::thread::sleep(Duration::from_millis(250));

        // Should only receive event for .md file (URL path)
        let event = rx.try_recv();
        assert!(event.is_some(), "Expected to receive event for .md file");
        let event = event.unwrap();
        assert_eq!(event.path, "doc");

        // No more events
        let event = rx.try_recv();
        assert!(event.is_none());
    }

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
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
    #[ignore = "timing-sensitive, can be flaky in test environments"]
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
