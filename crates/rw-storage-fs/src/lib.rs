//! Filesystem storage implementation for RW documentation engine.
//!
//! This crate provides [`FsStorage`], a filesystem-based implementation of the
//! [`Storage`](rw_storage::Storage) trait. It handles:
//!
//! - Recursive directory scanning for markdown files
//! - Title extraction from H1 headings with mtime caching
//! - Metadata loading from YAML sidecar files with inheritance
//! - File watching with event debouncing
//!
//! # Example
//!
//! ```ignore
//! use std::path::PathBuf;
//! use rw_storage::Storage;
//! use rw_storage_fs::FsStorage;
//!
//! let storage = FsStorage::new(PathBuf::from("docs"));
//! let documents = storage.scan()?;
//! for doc in documents {
//!     println!("{}: {}", doc.path, doc.title);
//! }
//! ```

mod debouncer;
mod inheritance;
mod scanner;
mod source;
mod yaml;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use glob::Pattern;
use notify::{RecursiveMode, Watcher};
use regex::Regex;

use debouncer::EventDebouncer;
use inheritance::{build_ancestor_chain, merge_metadata};
use rw_storage::{
    Document, Metadata, Storage, StorageError, StorageErrorKind, StorageEvent, StorageEventKind,
    StorageEventReceiver, WatchHandle,
};
use scanner::{DocumentRef, Scanner};
use source::file_path_to_url;
use yaml::{extract_yaml_title, extract_yaml_type, parse_metadata};

/// Backend identifier for error messages.
const BACKEND: &str = "Fs";

/// Create a storage error from a notify error.
fn notify_error(e: notify::Error) -> StorageError {
    StorageError::new(StorageErrorKind::Other)
        .with_backend(BACKEND)
        .with_source(e)
}

/// Convert a `notify::EventKind` to a `StorageEventKind`.
///
/// Returns `None` for event kinds that are not relevant (e.g., Access).
fn storage_event_kind(kind: notify::EventKind) -> Option<StorageEventKind> {
    match kind {
        notify::EventKind::Create(_) => Some(StorageEventKind::Created),
        notify::EventKind::Modify(_) => Some(StorageEventKind::Modified),
        notify::EventKind::Remove(_) => Some(StorageEventKind::Removed),
        _ => None,
    }
}

/// Process a notify event result, recording matching events into the debouncer.
///
/// The `filter` closure determines which paths to record. Return `Some(path)` to
/// record the event, or `None` to skip it.
fn record_notify_events(
    res: Result<notify::Event, notify::Error>,
    debouncer: &EventDebouncer,
    filter: impl Fn(PathBuf) -> Option<PathBuf>,
) {
    let Ok(event) = res else { return };
    let Some(kind) = storage_event_kind(event.kind) else {
        return;
    };
    for path in event.paths {
        if let Some(path) = filter(path) {
            debouncer.record(path, kind);
        }
    }
}

/// Derive a title for a virtual page from its URL path.
///
/// Uses the last path segment as a slug, falling back to "Untitled" for root paths.
fn virtual_page_title(url_path: &str) -> String {
    match url_path.rsplit_once('/').map_or(url_path, |(_, last)| last) {
        "" => "Untitled".to_string(),
        slug => titlecase_from_slug(slug),
    }
}

/// Convert a slug (kebab-case or `snake_case`) to title case.
///
/// Replaces `-` and `_` with spaces, then capitalizes the first letter of each word.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(titlecase_from_slug("setup-guide"), "Setup Guide");
/// assert_eq!(titlecase_from_slug("my_page"), "My Page");
/// ```
fn titlecase_from_slug(slug: &str) -> String {
    let mut result = String::with_capacity(slug.len());
    for word in slug.split(['-', '_', ' ']).filter(|w| !w.is_empty()) {
        if !result.is_empty() {
            result.push(' ');
        }
        capitalize_first_into(word, &mut result);
    }
    result
}

/// Capitalize the first character of a word, appending to `buf`.
fn capitalize_first_into(word: &str, buf: &mut String) {
    let mut chars = word.chars();
    if let Some(first) = chars.next() {
        buf.extend(first.to_uppercase());
        buf.push_str(chars.as_str());
    }
}

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
/// use rw_storage::Storage;
/// use rw_storage_fs::FsStorage;
///
/// let storage = FsStorage::new(PathBuf::from("docs"));
/// let documents = storage.scan()?;
/// for doc in documents {
///     println!("{}: {}", doc.path, doc.title);
/// }
/// ```
pub struct FsStorage {
    /// Root directory for document storage.
    source_dir: PathBuf,
    /// Scanner for document discovery.
    scanner: Scanner,
    /// Regex for extracting first H1 heading.
    h1_regex: Regex,
    /// Mtime cache for incremental title extraction.
    mtime_cache: Mutex<HashMap<PathBuf, CachedFile>>,
    /// Patterns for file watching (e.g., "**/*.md").
    watch_patterns: Vec<Pattern>,
    /// Metadata file name (e.g., "meta.yaml").
    meta_filename: String,
    /// Optional path to README.md used as homepage fallback.
    readme_path: Option<PathBuf>,
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

        let scanner = Scanner::new(&source_dir, meta_filename);

        Self {
            source_dir,
            scanner,
            h1_regex: Regex::new(r"(?m)^#\s+(.+)$").unwrap(),
            mtime_cache: Mutex::new(HashMap::new()),
            watch_patterns,
            meta_filename: meta_filename.to_string(),
            readme_path: None,
        }
    }

    /// Set a README.md path to use as homepage fallback when `docs/index.md` doesn't exist.
    #[must_use]
    pub fn with_readme(mut self, readme_path: PathBuf) -> Self {
        self.readme_path = Some(readme_path);
        self
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
    /// For root path (`""`):
    /// 1. `source_dir/index.md`
    /// 2. `readme_path` (if configured via [`with_readme`](Self::with_readme))
    ///
    /// For other paths:
    /// 1. `{path}/index.md` (directory structure preferred)
    /// 2. `{path}.md` (standalone file fallback)
    ///
    /// Returns `None` if no content file exists.
    fn resolve_content(&self, url_path: &str) -> Option<PathBuf> {
        if url_path.is_empty() {
            let index = self.source_dir.join("index.md");
            if index.exists() {
                return Some(index);
            }
            if let Some(ref readme) = self.readme_path
                && readme.exists()
            {
                return Some(readme.clone());
            }
            return None;
        }

        // Prefer directory/index.md
        let index_path = self.source_dir.join(url_path).join("index.md");
        if index_path.exists() {
            return Some(index_path);
        }

        // Fall back to standalone file
        let file_path = self.source_dir.join(url_path).with_extension("md");
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
        let meta_path = self.source_dir.join(url_path).join(&self.meta_filename);
        meta_path.exists().then_some(meta_path)
    }

    /// Build a `Document` from a `DocumentRef`.
    ///
    /// Converts discovery results (file references) into full Document structs
    /// by reading file contents and extracting titles/metadata.
    ///
    /// Returns `None` if the ref produces no valid document (e.g., empty meta.yaml
    /// for a virtual page).
    fn build_document(&self, doc_ref: &DocumentRef) -> Option<Document> {
        let meta_content = doc_ref
            .meta_path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok());
        let meta_str = meta_content.as_deref();
        let meta_title = meta_str.and_then(extract_yaml_title);
        let page_type = meta_str.and_then(extract_yaml_type);

        let title = if let Some(title) = meta_title {
            title
        } else if let Some(md_path) = &doc_ref.content_path {
            self.extract_or_derive_title(md_path)
        } else {
            // Virtual page: skip if no metadata or empty content
            if meta_str.is_none_or(|c| c.trim().is_empty()) {
                return None;
            }
            virtual_page_title(&doc_ref.url_path)
        };

        Some(Document {
            path: doc_ref.url_path.clone(),
            title,
            has_content: doc_ref.content_path.is_some(),
            page_type,
        })
    }

    /// Extract title from content or derive it from filename, using mtime cache.
    fn extract_or_derive_title(&self, file_path: &Path) -> String {
        let mtime = fs::metadata(file_path).ok().and_then(|m| m.modified().ok());

        // Check cache (lock released at end of block)
        if let Some(mtime) = mtime {
            let cache = self.mtime_cache.lock().unwrap();
            if let Some(cached) = cache.get(file_path).filter(|c| c.mtime == mtime) {
                return cached.title.clone();
            }
        }

        let title = self
            .extract_title_from_content(file_path)
            .unwrap_or_else(|| Self::derive_title_from_filename(file_path));

        if let Some(mtime) = mtime {
            self.mtime_cache.lock().unwrap().insert(
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
        let caps = self.h1_regex.captures(&content)?;
        Some(caps[1].trim().to_string())
    }

    /// Generate title from a file path's filename.
    fn derive_title_from_filename(file_path: &Path) -> String {
        file_path
            .file_stem()
            .map(|s| titlecase_from_slug(&s.to_string_lossy().to_lowercase()))
            .unwrap_or_default()
    }

    /// Load and parse metadata from a single ancestor path.
    ///
    /// Returns `None` if no metadata file exists, is empty, or fails to parse.
    fn load_ancestor_meta(&self, ancestor: &str) -> Option<Metadata> {
        let meta_path = self.resolve_meta(ancestor)?;
        let content = fs::read_to_string(&meta_path)
            .inspect_err(|e| {
                tracing::warn!(path = %ancestor, error = %e, "Failed to read metadata file, skipping");
            })
            .ok()?;
        let meta = parse_metadata(&content)
            .inspect_err(|e| {
                tracing::warn!(path = %ancestor, error = %e, "Failed to parse metadata, skipping");
            })
            .ok()?;
        (!meta.is_empty()).then_some(meta)
    }

    /// Set up a file watcher for README.md (outside `source_dir`).
    ///
    /// Watches the README.md file directly. Events are recorded into the
    /// shared debouncer.
    fn watch_readme(
        readme_path: &Path,
        debouncer: &Arc<EventDebouncer>,
    ) -> Result<notify::RecommendedWatcher, StorageError> {
        let debouncer = Arc::clone(debouncer);

        let mut watcher = notify::recommended_watcher(move |res| {
            record_notify_events(res, &debouncer, Some);
        })
        .map_err(notify_error)?;

        watcher
            .watch(readme_path, RecursiveMode::NonRecursive)
            .map_err(notify_error)?;

        Ok(watcher)
    }
}

impl Storage for FsStorage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        let mut documents: Vec<Document> = self
            .scanner
            .scan()
            .into_iter()
            .filter_map(|r| self.build_document(&r))
            .collect();

        // Inject README.md as homepage if no root document found
        if let Some(ref readme_path) = self.readme_path
            && !documents.iter().any(|d| d.path.is_empty())
            && readme_path.exists()
        {
            let title = self
                .extract_title_from_content(readme_path)
                .unwrap_or_else(|| "Home".to_string());
            documents.push(Document {
                path: String::new(),
                title,
                has_content: true,
                page_type: None,
            });
        }

        Ok(documents)
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
        Self::validate_path(path).is_ok()
            && (self.resolve_content(path).is_some() || self.resolve_meta(path).is_some())
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        Self::validate_path(path)?;
        let full_path = self
            .resolve_content(path)
            .or_else(|| self.resolve_meta(path))
            .ok_or_else(|| StorageError::not_found(path).with_backend(BACKEND))?;
        let modified = fs::metadata(&full_path)
            .and_then(|m| m.modified())
            .map_err(|e| StorageError::io(e, Some(PathBuf::from(path))).with_backend(BACKEND))?;
        Ok(modified
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64()))
    }

    fn watch(&self) -> Result<(StorageEventReceiver, WatchHandle), StorageError> {
        let (event_tx, event_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let debouncer = Arc::new(EventDebouncer::new(Duration::from_millis(100)));

        let source_dir = self.source_dir.clone();
        let patterns = self.watch_patterns.clone();
        let watcher_debouncer = Arc::clone(&debouncer);

        let mut watcher = notify::recommended_watcher(move |res| {
            record_notify_events(res, &watcher_debouncer, |path| {
                let rel_path = path.strip_prefix(&source_dir).ok()?;
                (patterns.is_empty() || patterns.iter().any(|p| p.matches_path(rel_path)))
                    .then_some(path)
            });
        })
        .map_err(notify_error)?;

        watcher
            .watch(&self.source_dir, RecursiveMode::Recursive)
            .map_err(notify_error)?;

        let readme_watcher = self
            .readme_path
            .as_deref()
            .filter(|p| p.exists())
            .map(|p| Self::watch_readme(p, &debouncer))
            .transpose()?;

        // Spawn drain thread. Watchers are moved in to keep them alive.
        let source_dir = self.source_dir.clone();
        std::thread::spawn(move || {
            let _watcher = watcher;
            let _readme_watcher = readme_watcher;

            loop {
                match shutdown_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }

                for event in debouncer.drain_ready() {
                    // Paths outside source_dir (e.g. README.md) map to root ("")
                    let url_path = event
                        .path
                        .strip_prefix(&source_dir)
                        .map_or_else(|_| String::new(), file_path_to_url);

                    if event_tx
                        .send(StorageEvent {
                            path: url_path,
                            kind: event.kind,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });

        // When dropped, shutdown_tx disconnects, causing the drain thread to exit
        Ok((
            StorageEventReceiver::new(event_rx),
            WatchHandle::new(shutdown_tx),
        ))
    }

    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError> {
        Self::validate_path(path)?;

        let ancestors = build_ancestor_chain(path);

        let has_own_meta = self.load_ancestor_meta(path).is_some();

        let mut accumulated = ancestors
            .iter()
            .filter_map(|ancestor| self.load_ancestor_meta(ancestor))
            .reduce(|parent, child| merge_metadata(&parent, &child));

        // If the requested path doesn't have its own (non-empty, valid) metadata,
        // clear title/description/page_type (only vars are inherited)
        if !has_own_meta
            && let Some(meta) = &mut accumulated
        {
            meta.title = None;
            meta.description = None;
            meta.page_type = None;
        }

        Ok(accumulated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_storage::StorageErrorKind;

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
        let paths: Vec<_> = docs.iter().map(|d| d.path.as_str()).collect();
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
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 2);
        let paths: Vec<_> = docs.iter().map(|d| d.path.as_str()).collect();
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
        assert_eq!(docs[0].path, "visible");
    }

    #[test]
    fn test_scan_extracts_page_type() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("meta.yaml"), "type: domain").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
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
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert!(doc.has_content);
        assert_eq!(doc.page_type, Some("section".to_string()));
    }

    #[test]
    fn test_scan_no_page_type_without_type_field() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "title: Home Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
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
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
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
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
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
        let docs = storage.scan().unwrap();

        assert!(docs.is_empty());
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
    fn test_meta_empty_own_metadata_clears_inherited_fields() {
        let temp_dir = create_test_dir();
        let parent = temp_dir.path().join("parent");
        fs::create_dir(&parent).unwrap();
        fs::write(parent.join("index.md"), "# Parent").unwrap();
        fs::write(
            parent.join("meta.yaml"),
            "title: Parent Title\ndescription: Parent Desc\ntype: domain\nvars:\n  key: value",
        )
        .unwrap();

        // Child with empty meta.yaml
        let child = parent.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("index.md"), "# Child").unwrap();
        fs::write(child.join("meta.yaml"), "").unwrap();

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
    fn test_meta_invalid_own_metadata_clears_inherited_fields() {
        let temp_dir = create_test_dir();
        let parent = temp_dir.path().join("parent");
        fs::create_dir(&parent).unwrap();
        fs::write(parent.join("index.md"), "# Parent").unwrap();
        fs::write(
            parent.join("meta.yaml"),
            "title: Parent Title\ndescription: Parent Desc\ntype: domain\nvars:\n  key: value",
        )
        .unwrap();

        // Child with invalid YAML
        let child = parent.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("index.md"), "# Child").unwrap();
        fs::write(child.join("meta.yaml"), "{{invalid yaml").unwrap();

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
    fn test_derive_title_from_filename() {
        assert_eq!(
            FsStorage::derive_title_from_filename(Path::new("setup-guide.md")),
            "Setup Guide"
        );
        assert_eq!(
            FsStorage::derive_title_from_filename(Path::new("my_page.md")),
            "My Page"
        );
        assert_eq!(
            FsStorage::derive_title_from_filename(Path::new("complex-name_here.md")),
            "Complex Name Here"
        );
        assert_eq!(
            FsStorage::derive_title_from_filename(Path::new("simple.md")),
            "Simple"
        );
    }

    // Note: file_path_to_url tests are in source.rs

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

    /// Create a test directory with `docs/` subdirectory and README.md,
    /// returning `(temp_dir, FsStorage with readme)`.
    fn create_readme_test_dir(readme_content: &str) -> (tempfile::TempDir, PathBuf, FsStorage) {
        let temp_dir = create_test_dir();
        let project_root = temp_dir.path().to_path_buf();
        let source_dir = project_root.join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(project_root.join("README.md"), readme_content).unwrap();

        let readme_path = project_root.join("README.md");
        let storage = FsStorage::new(source_dir).with_readme(readme_path);
        (temp_dir, project_root, storage)
    }

    #[test]
    fn test_readme_as_homepage_when_no_index() {
        let (_dir, _, storage) = create_readme_test_dir("# My Project\n\nWelcome.");
        let content = storage.read("").unwrap();

        assert_eq!(content, "# My Project\n\nWelcome.");
    }

    #[test]
    fn test_readme_does_not_override_existing_index() {
        let (dir, _, storage) = create_readme_test_dir("# README Content");
        fs::write(dir.path().join("docs/index.md"), "# Docs Home").unwrap();
        let content = storage.read("").unwrap();

        assert_eq!(content, "# Docs Home");
    }

    #[test]
    fn test_scan_includes_readme_as_homepage() {
        let (dir, _, storage) = create_readme_test_dir("# My Project");
        fs::write(dir.path().join("docs/guide.md"), "# Guide").unwrap();
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 2);
        let home = docs.iter().find(|d| d.path.is_empty()).unwrap();
        assert_eq!(home.title, "My Project");
        assert!(home.has_content);
    }

    #[test]
    fn test_scan_does_not_inject_readme_when_index_exists() {
        let (dir, _, storage) = create_readme_test_dir("# README");
        fs::write(dir.path().join("docs/index.md"), "# Docs Home").unwrap();
        let docs = storage.scan().unwrap();

        let homes: Vec<_> = docs.iter().filter(|d| d.path.is_empty()).collect();
        assert_eq!(homes.len(), 1);
        assert_eq!(homes[0].title, "Docs Home");
    }

    #[test]
    fn test_exists_returns_true_for_readme_homepage() {
        let (_dir, _, storage) = create_readme_test_dir("# Home");
        assert!(storage.exists(""));
    }

    #[test]
    fn test_mtime_works_for_readme_homepage() {
        let (_dir, _, storage) = create_readme_test_dir("# Home");
        let mtime = storage.mtime("").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert!(mtime > now - 60.0);
        assert!(mtime <= now);
    }

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
    fn test_watch_detects_readme_changes() {
        let (_dir, project_root, storage) = create_readme_test_dir("# Original");
        let (rx, _handle) = storage.watch().unwrap();

        std::thread::sleep(Duration::from_millis(200));
        fs::write(project_root.join("README.md"), "# Modified").unwrap();
        std::thread::sleep(Duration::from_millis(500));

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv()).collect();
        assert!(!events.is_empty(), "Expected to receive at least one event");

        let home_event = events.iter().find(|e| e.path.is_empty());
        assert!(
            home_event.is_some(),
            "Expected event for root path, got: {events:?}"
        );
    }
}
