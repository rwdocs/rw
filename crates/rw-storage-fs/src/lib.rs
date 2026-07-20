//! Filesystem storage implementation for RW documentation engine.
//!
//! This crate provides [`FsStorage`], a filesystem-based implementation of the
//! [`Storage`](rw_storage::Storage) trait. It handles:
//!
//! - Recursive directory scanning for markdown files
//! - Metadata extraction (title, description, kind) with mtime caching
//! - Metadata loading from YAML sidecar files
//! - File watching with event debouncing
//!
//! # Example
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use std::path::PathBuf;
//! use rw_storage::Storage;
//! use rw_storage_fs::FsStorage;
//!
//! let storage = FsStorage::new(PathBuf::from("docs"));
//! let documents = storage.scan()?;
//! for doc in documents {
//!     println!("{}: {}", doc.path, doc.title);
//! }
//! # Ok(())
//! # }
//! ```

mod debouncer;
mod scanner;
mod source;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use glob::Pattern;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use rw_meta::Meta;
use rw_sections::Namespace;
use rw_vcs::{Vcs, fs_mtime};

use debouncer::{DebouncedEvent, EventDebouncer, RawEventKind};
use rw_storage::{
    Document, Metadata, MetadataError, Storage, StorageError, StorageErrorKind, StorageEvent,
    StorageEventKind, StorageEventReceiver, WatchHandle,
};
use scanner::{DocumentRef, Scanner};
use source::{Classification, PathResolver, file_path_to_url};

/// Backend identifier for error messages.
const BACKEND: &str = "Fs";

/// Convert a `notify::EventKind` to a `RawEventKind`.
///
/// Returns `None` for event kinds that are not relevant (e.g., Access).
fn storage_event_kind(kind: notify::EventKind) -> Option<RawEventKind> {
    match kind {
        notify::EventKind::Create(_) => Some(RawEventKind::Created),
        notify::EventKind::Modify(_) => Some(RawEventKind::Modified),
        notify::EventKind::Remove(_) => Some(RawEventKind::Removed),
        _ => None,
    }
}

/// Cached resolved metadata for incremental extraction.
#[derive(Debug)]
struct CachedMeta {
    /// Markdown file modification time.
    md_mtime: SystemTime,
    /// Meta YAML file modification time (`None` if no meta.yaml exists).
    meta_mtime: Option<SystemTime>,
    /// Resolved metadata.
    meta: Meta,
}

/// Filesystem storage implementation.
///
/// Scans a source directory recursively for markdown files and extracts
/// metadata (title, description, kind) using `rw_meta`. Uses mtime caching
/// to avoid re-reading unchanged files.
///
/// # Example
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use std::path::PathBuf;
/// use rw_storage::Storage;
/// use rw_storage_fs::FsStorage;
///
/// let storage = FsStorage::new(PathBuf::from("docs"));
/// let documents = storage.scan()?;
/// for doc in documents {
///     println!("{}: {}", doc.path, doc.title);
/// }
/// # Ok(())
/// # }
/// ```
pub struct FsStorage {
    /// Cloned into the watch drain thread, which resolves through the same rules.
    resolver: PathResolver,
    /// Scanner for document discovery.
    scanner: Scanner,
    /// Mtime cache for incremental metadata extraction.
    mtime_cache: RwLock<HashMap<PathBuf, CachedMeta>>,
    /// Glob patterns for file watching (`**/*.md` and metadata files).
    watch_patterns: Vec<Pattern>,
    /// How this storage computes modification times (filesystem or git).
    mtime: MtimeStrategy,
}

/// Selects how [`FsStorage`] computes a page's modification time.
///
/// The default is [`Filesystem`](MtimeSource::Filesystem): a plain `stat`, with
/// no git involvement (not even repository discovery). Choose
/// [`Git`](MtimeSource::Git) — via [`FsStorage::with_mtime_source`] — for stable,
/// history-derived times (e.g. when publishing), at the cost of a per-call git
/// query (index load, file hash, history walk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MtimeSource {
    /// Filesystem `stat` mtime (fast, reflects on-disk edits). Default.
    #[default]
    Filesystem,
    /// Git commit author-time for clean tracked files, filesystem mtime
    /// otherwise (via [`rw_vcs::Vcs`]).
    Git,
}

/// Internal per-storage mtime strategy. Carries the [`Vcs`] only in `Git`, so
/// `Filesystem` mode holds no git state. The `Vcs` is boxed to keep the enum
/// (and thus every `Filesystem`-mode `FsStorage`) pointer-sized rather than
/// reserving the ~700-byte repository handle inline.
enum MtimeStrategy {
    Filesystem,
    Git(Box<Vcs>),
}

/// Default metadata filename.
const DEFAULT_META_FILENAME: &str = "meta.yaml";

impl FsStorage {
    /// Create a new filesystem storage.
    ///
    /// Watches `**/*.md`, `meta.yaml`, and `*.meta.yaml` (named sidecar / index
    /// form) files for changes.
    ///
    /// Modification times default to [`MtimeSource::Filesystem`]; call
    /// [`with_mtime_source`](Self::with_mtime_source) to opt into git times.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Root directory containing markdown files
    #[must_use]
    pub fn new(source_dir: PathBuf) -> Self {
        Self::with_meta_filename(source_dir, DEFAULT_META_FILENAME)
    }

    /// Create a new filesystem storage with a custom metadata filename.
    ///
    /// Watches `**/*.md`, `**/{meta_filename}`, and `**/*.{meta_filename}` files for changes.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Root directory containing markdown files
    /// * `meta_filename` - Name of metadata files (e.g., "meta.yaml")
    ///
    /// Modification times default to [`MtimeSource::Filesystem`]; call
    /// [`with_mtime_source`](Self::with_mtime_source) to opt into git times.
    #[must_use]
    pub fn with_meta_filename(source_dir: PathBuf, meta_filename: &str) -> Self {
        let scanner = Scanner::new(&source_dir, meta_filename);
        let resolver = PathResolver::new(source_dir, meta_filename);

        Self {
            watch_patterns: resolver.watch_patterns(),
            scanner,
            resolver,
            mtime_cache: RwLock::new(HashMap::new()),
            mtime: MtimeStrategy::Filesystem,
        }
    }

    /// Selects the modification-time source (default
    /// [`Filesystem`](MtimeSource::Filesystem)).
    ///
    /// [`Git`](MtimeSource::Git) discovers the repository from `source_dir` and
    /// uses commit times; [`Filesystem`](MtimeSource::Filesystem) does a plain
    /// `stat` and touches no git state.
    #[must_use]
    pub fn with_mtime_source(mut self, source: MtimeSource) -> Self {
        self.mtime = match source {
            MtimeSource::Filesystem => MtimeStrategy::Filesystem,
            MtimeSource::Git => {
                let vcs = Vcs::new(self.resolver.source_dir());
                if !vcs.has_repo() {
                    tracing::warn!(
                        source_dir = %self.resolver.source_dir().display(),
                        "git modification times requested but no git repository was \
                         found; page times will fall back to filesystem times",
                    );
                }
                MtimeStrategy::Git(Box::new(vcs))
            }
        };
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

    /// Resolve URL path to content file path. See [`PathResolver::resolve_content`].
    fn resolve_content(&self, url_path: &str) -> Option<PathBuf> {
        self.resolver.resolve_content(url_path)
    }

    /// Resolve a page's own metadata file. See [`PathResolver::resolve_meta`].
    fn resolve_meta(&self, url_path: &str) -> Option<PathBuf> {
        self.resolver.resolve_meta(url_path)
    }

    /// Build a `Document` from a `DocumentRef`.
    ///
    /// Converts discovery results (file references) into full Document structs
    /// by reading file contents and extracting titles/metadata.
    ///
    /// Returns `Ok(None)` if the ref produces no valid document (e.g., empty meta.yaml
    /// for a virtual page). Returns `Err` if the namespace declared in metadata is invalid.
    fn build_document(&self, doc_ref: &DocumentRef) -> Result<Option<Document>, StorageError> {
        let validate = |meta: &Meta, file: &Path| -> Result<(), StorageError> {
            if let Some(ns) = &meta.namespace {
                ns.parse::<Namespace>().map_err(|e| {
                    StorageError::new(StorageErrorKind::InvalidPath)
                        .with_backend(BACKEND)
                        .with_path(file.to_path_buf())
                        .with_source(e)
                })?;
            }
            Ok(())
        };

        if let Some(md_path) = &doc_ref.content_path {
            let name_lower = md_path
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().to_lowercase());

            let meta = self.get_meta(md_path, doc_ref.meta_path.as_deref(), &name_lower);

            // Namespace declarations almost always live in the sidecar
            // meta.yaml; attribute validation errors there when one exists,
            // otherwise to the .md file. Edge case: a namespace declared in
            // .md frontmatter alongside an unrelated meta.yaml will be
            // misattributed — the bad value still appears in the error
            // message, so a grep finds it.
            let validation_file = doc_ref.meta_path.as_deref().unwrap_or(md_path);
            validate(&meta, validation_file)?;

            Ok(Some(Document {
                path: doc_ref.url_path.clone(),
                title: meta.title,
                has_content: true,
                page_kind: meta.kind,
                namespace: meta.namespace,
                description: meta.description,
                origin: None,
                pages: meta.pages,
                is_dir: name_lower == "index.md",
            }))
        } else if let Some(meta_path) = &doc_ref.meta_path {
            let Ok(meta_yaml) = fs::read_to_string(meta_path) else {
                return Ok(None);
            };

            if meta_yaml.trim().is_empty() {
                return Ok(None);
            }

            let dir_name = Path::new(&doc_ref.url_path)
                .file_name()
                .map_or("untitled", |n| n.to_str().unwrap_or("untitled"));

            let meta = Meta::resolve(None, Some(&meta_yaml), dir_name);

            validate(&meta, meta_path)?;

            Ok(Some(Document {
                path: doc_ref.url_path.clone(),
                title: meta.title,
                has_content: false,
                page_kind: meta.kind,
                namespace: meta.namespace,
                description: meta.description,
                origin: None,
                pages: meta.pages,
                is_dir: true,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get resolved metadata for a file, using mtime cache when possible.
    ///
    /// Only reads the markdown file content on cache miss, avoiding unnecessary
    /// I/O for unchanged files during scans. Invalidates when either the markdown
    /// file or its associated meta.yaml changes.
    fn get_meta(&self, file_path: &Path, meta_path: Option<&Path>, filename: &str) -> Meta {
        let current_md_mtime = fs::metadata(file_path).ok().and_then(|m| m.modified().ok());
        let current_meta_mtime = meta_path
            .and_then(|p| fs::metadata(p).ok())
            .and_then(|m| m.modified().ok());

        // Check cache — avoid reading file content if both mtimes unchanged.
        {
            let cache = self.mtime_cache.read();
            if let (Some(cached), Some(md_mtime)) = (cache.get(file_path), current_md_mtime)
                && cached.md_mtime == md_mtime
                && cached.meta_mtime == current_meta_mtime
            {
                return cached.meta.clone();
            }
        }

        // Cache miss — read file content now
        let markdown = fs::read_to_string(file_path).ok();
        let meta_yaml = meta_path.and_then(|p| fs::read_to_string(p).ok());
        let meta = Meta::resolve(markdown.as_deref(), meta_yaml.as_deref(), filename);

        // Update cache
        if let Some(md_mtime) = current_md_mtime {
            let mut cache = self.mtime_cache.write();
            cache.insert(
                file_path.to_path_buf(),
                CachedMeta {
                    md_mtime,
                    meta_mtime: current_meta_mtime,
                    meta: meta.clone(),
                },
            );
        }

        meta
    }

    /// URL paths of the existing page(s) a markdown source file could refer to.
    ///
    /// Accepts the path relative to the project root (with the `source_dir`
    /// prefix, e.g. `docs/guide.md`), relative to `source_dir` (e.g. `guide.md`),
    /// or absolute, plus the README homepage. Returns one entry normally, several
    /// when the input is ambiguous (distinct existing pages), or none when it
    /// names no page. Uses the scanner's own classification routine, so the url
    /// path matches the live site exactly.
    #[must_use]
    pub fn url_paths_for_source(&self, file_path: &Path) -> Vec<String> {
        self.resolver.url_paths_for_source(file_path)
    }

    /// Set up a file watcher for README.md (outside `source_dir`).
    ///
    /// Watches the README.md file directly. Events are recorded into the
    /// shared debouncer.
    fn watch_readme(
        readme_path: &Path,
        debouncer: &std::sync::Arc<EventDebouncer>,
    ) -> Result<notify::RecommendedWatcher, StorageError> {
        let debouncer = std::sync::Arc::clone(debouncer);

        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let Some(kind) = storage_event_kind(event.kind) else {
                        return;
                    };

                    for path in event.paths {
                        debouncer.record(path, kind);
                    }
                }
            })
            .map_err(|e| {
                StorageError::new(StorageErrorKind::Other)
                    .with_backend(BACKEND)
                    .with_source(e)
            })?;

        watcher
            .watch(readme_path, RecursiveMode::NonRecursive)
            .map_err(|e| {
                StorageError::new(StorageErrorKind::Other)
                    .with_backend(BACKEND)
                    .with_source(e)
            })?;

        Ok(watcher)
    }
}

/// Read and parse metadata for a URL path from the filesystem.
///
/// Distinct from [`PathResolver::resolve_meta`], which only *locates* a file:
/// this reads and parses it. Used by the watch drain thread to populate
/// `StorageEventKind::Modified`.
///
/// Both candidate searches go through `resolver`, which is also what `read()`,
/// `exists()`, and `meta()` resolve through — so the watch path cannot probe in
/// a different order than a request does. It is NOT automatically aligned with
/// the *scan* path (`Scanner` + `MetaRank`), which encodes the same precedence
/// in a different shape; `scan_and_resolver_agree_across_the_precedence_matrix`
/// pins the two together.
fn resolve_event_meta(resolver: &PathResolver, url_path: &str) -> Meta {
    let meta_yaml = resolver
        .resolve_meta(url_path)
        .and_then(|p| fs::read_to_string(p).ok());

    let content_path = resolver.resolve_content(url_path);
    let markdown = content_path
        .as_deref()
        .and_then(|p| fs::read_to_string(p).ok());

    let fallback = resolver.content_fallback_name(url_path, content_path.as_deref());

    Meta::resolve(markdown.as_deref(), meta_yaml.as_deref(), &fallback)
}

/// Whether a path (relative to `source_dir`) has any dot-prefixed component.
///
/// Mirrors the scanner's hidden-file filtering (the `ignore` walker's
/// `.hidden(true)`), so the watch path does not emit events for files the scan
/// ignores — e.g. a hidden `.meta.yaml`, which would otherwise map to a phantom
/// `.meta` url path and trigger a spurious reload.
fn is_hidden_rel_path(rel_path: &std::path::Path) -> bool {
    rel_path.components().any(|c| {
        matches!(c, std::path::Component::Normal(name)
            if name.to_string_lossy().starts_with('.'))
    })
}

/// Convert a debounced file-system event into a [`StorageEvent`].
///
/// Resolves the file path to a URL path and populates the event kind with
/// resolved metadata (title, pages) for `Modified` events.
fn to_storage_event(event: &DebouncedEvent, resolver: &PathResolver) -> StorageEvent {
    let url_path = if let Ok(rel_path) = event.path.strip_prefix(resolver.source_dir()) {
        let filename = rel_path
            .file_name()
            .map(|f| f.to_string_lossy())
            .unwrap_or_default();
        resolver
            .classify_relpath(rel_path, &filename)
            .map_or_else(|| file_path_to_url(rel_path), Classification::into_url_path)
    } else {
        // Outside source_dir (e.g., README.md) -> root
        String::new()
    };

    let kind = match event.kind {
        RawEventKind::Created => StorageEventKind::Created,
        RawEventKind::Modified => {
            let meta = resolve_event_meta(resolver, &url_path);
            StorageEventKind::Modified {
                title: meta.title,
                pages: meta.pages,
            }
        }
        RawEventKind::Removed => StorageEventKind::Removed,
    };

    StorageEvent {
        path: url_path,
        kind,
    }
}

/// Try to start watching `source_dir` recursively once it exists.
///
/// Used by the watch drain thread to "upgrade" a README-only project (where
/// `source_dir` did not exist at startup) to a full recursive watch as soon as
/// the directory is created. Returns `true` once the recursive watch is active.
///
/// On success it records a synthetic `Created` event for `source_dir`: the
/// freshly created directory and its contents predate the recursive watch, and
/// notify does not replay a `Created` for the watch root, so without it the
/// initial content would never trigger a rescan. Errors are swallowed: a
/// directory created then removed between the check and the watch call (TOCTOU),
/// or a transient watch failure, simply leaves the watch inactive to be retried
/// on the next drain tick; the caller logs a one-time warning if it persists.
///
/// Returns `false` without side effects when `source_dir` is not a directory
/// (the guard is `is_dir()`, not `exists()` — see the call site in `watch`).
fn try_upgrade_recursive_watch(
    watcher: &parking_lot::Mutex<RecommendedWatcher>,
    debouncer: &EventDebouncer,
    source_dir: &Path,
) -> bool {
    if !source_dir.is_dir() {
        return false;
    }

    let mut guard = watcher.lock();
    if guard.watch(source_dir, RecursiveMode::Recursive).is_ok() {
        drop(guard);
        debouncer.record(source_dir.to_path_buf(), RawEventKind::Created);
        true
    } else {
        false
    }
}

impl Storage for FsStorage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        let t0 = Instant::now();
        let refs = self.scanner.scan();
        let walk_elapsed = t0.elapsed();

        let t1 = Instant::now();
        let mut documents: Vec<Document> = refs
            .par_iter()
            .filter_map(|r| self.build_document(r).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        let build_elapsed = t1.elapsed();

        tracing::info!(
            files = refs.len(),
            documents = documents.len(),
            walk_ms = format_args!("{:.1}", walk_elapsed.as_secs_f64() * 1000.0),
            build_ms = format_args!("{:.1}", build_elapsed.as_secs_f64() * 1000.0),
            total_ms = format_args!("{:.1}", t0.elapsed().as_secs_f64() * 1000.0),
            "Storage scan complete"
        );

        // Inject README.md as homepage if no root document found
        if !documents.iter().any(|d| d.path.is_empty())
            && let Some(meta) = self.resolver.homepage_fallback_meta()
        {
            let origin = self
                .resolver
                .source_dir()
                .file_name()
                .and_then(|n| n.to_str())
                .map(ToOwned::to_owned);
            documents.push(Document {
                path: String::new(),
                title: meta.title,
                has_content: true,
                page_kind: None,
                namespace: None,
                description: None,
                origin,
                pages: None,
                is_dir: true,
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

        // Collect all file paths that contribute to this page
        let content_path = self.resolve_content(path);
        let meta_path = self.resolve_meta(path);

        if content_path.is_none() && meta_path.is_none() {
            return Err(StorageError::not_found(path).with_backend(BACKEND));
        }

        let paths: Vec<&Path> = [&content_path, &meta_path]
            .into_iter()
            .filter_map(|p| p.as_deref())
            .collect();

        let mtime = match &self.mtime {
            MtimeStrategy::Filesystem => paths
                .iter()
                .filter_map(|p| fs_mtime(p))
                .fold(0.0_f64, f64::max),
            MtimeStrategy::Git(vcs) => vcs.mtime(&paths),
        };
        Ok(mtime)
    }

    fn watch(&self) -> Result<(StorageEventReceiver, WatchHandle), StorageError> {
        // Create channel for events
        let (event_tx, event_rx) = mpsc::channel();

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = mpsc::channel();

        // Create debouncer (100ms as per RD-034)
        let debouncer = std::sync::Arc::new(EventDebouncer::new(Duration::from_millis(100)));

        // Setup notify watcher
        let source_dir = self.resolver.source_dir().to_path_buf();
        let patterns = self.watch_patterns.clone();
        let debouncer_for_watcher = std::sync::Arc::clone(&debouncer);

        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let Some(kind) = storage_event_kind(event.kind) else {
                        return;
                    };

                    for path in event.paths {
                        let Ok(rel_path) = path.strip_prefix(&source_dir) else {
                            continue;
                        };

                        // Mirror the scanner's hidden-file filtering so a hidden
                        // file (e.g. `.meta.yaml`) never produces an event the
                        // scan would not.
                        if is_hidden_rel_path(rel_path) {
                            continue;
                        }

                        // Directory events (e.g., renames) signal structural
                        // changes that must trigger a rescan.
                        let matches_pattern = patterns.is_empty()
                            || path.is_dir()
                            || patterns
                                .iter()
                                .any(|pattern| pattern.matches_path(rel_path));

                        if matches_pattern {
                            debouncer_for_watcher.record(path, kind);
                        }
                    }
                }
            })
            .map_err(|e| {
                StorageError::new(StorageErrorKind::Other)
                    .with_backend(BACKEND)
                    .with_source(e)
            })?;

        // Only watch source_dir if it is a directory. A README-only project
        // (no docs/) must still start; the drain thread upgrades to a recursive
        // watch if docs/ appears later. README edits are handled by the README
        // watcher. `is_dir()` (not `exists()`) matches the upgrade helper: a
        // non-directory at the path must not abort the watch with a hard error.
        let mut recursive_active = if self.resolver.source_dir().is_dir() {
            watcher
                .watch(self.resolver.source_dir(), RecursiveMode::Recursive)
                .map_err(|e| {
                    StorageError::new(StorageErrorKind::Other)
                        .with_backend(BACKEND)
                        .with_source(e)
                })?;
            true
        } else {
            false
        };

        // Keep watcher alive in Arc
        let watcher = std::sync::Arc::new(parking_lot::Mutex::new(watcher));

        // Set up a second watcher for README.md. The README lives in the parent
        // of source_dir, so it falls outside the recursive watch above and needs
        // its own non-recursive watcher.
        let readme_watcher = self
            .resolver
            .existing_readme()
            .map(|p| Self::watch_readme(p, &debouncer))
            .transpose()?;

        // Spawn thread to drain debouncer and send to channel
        let resolver_for_drain = self.resolver.clone();
        std::thread::spawn(move || {
            // Own the watcher in this thread; the drain loop also locks it to
            // upgrade to a recursive watch once source_dir appears.
            let watcher_guard = watcher;
            let _readme_watcher_guard = readme_watcher;
            // Whether a persistent recursive-watch upgrade failure was logged
            // already, so the warning is emitted at most once (not every tick).
            let mut upgrade_warned = false;

            loop {
                // Check for shutdown signal (blocking until timeout or signal)
                match shutdown_rx.recv_timeout(Duration::from_millis(50)) {
                    // Shutdown signaled or handle dropped
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {} // Continue draining
                }

                // If source_dir did not exist at startup, keep polling until it
                // appears, then upgrade to a recursive watch. The poll runs every
                // tick regardless of file-system events, so creation is always
                // detected.
                if !recursive_active {
                    recursive_active = try_upgrade_recursive_watch(
                        &watcher_guard,
                        &debouncer,
                        resolver_for_drain.source_dir(),
                    );
                    // The directory exists but the watch could not be started
                    // (e.g. permissions, or the inotify watch limit): warn once
                    // so the broken live-reload is not silent. The poll keeps
                    // retrying every tick, so this may yet recover.
                    if !recursive_active
                        && resolver_for_drain.source_dir().is_dir()
                        && !upgrade_warned
                    {
                        tracing::warn!(
                            dir = %resolver_for_drain.source_dir().display(),
                            "failed to start recursive watch on source directory; \
                             retrying every poll — live reload may lag for it"
                        );
                        upgrade_warned = true;
                    }
                }

                for event in debouncer.drain_ready() {
                    let storage_event = to_storage_event(&event, &resolver_for_drain);

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

    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError> {
        Self::validate_path(path)?;

        // Metadata does not cascade: a page carries only what its own
        // metadata file declares.
        let Some(meta_path) = self.resolve_meta(path) else {
            return Ok(None);
        };

        let content = match fs::read_to_string(&meta_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    path = %path,
                    error = %e,
                    "Failed to read metadata file, skipping"
                );
                return Ok(None);
            }
        };

        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let parsed: Result<Metadata, MetadataError> = serde_yaml::from_str(trimmed)
            .map_err(|e| MetadataError::Parse(format!("Invalid YAML: {e}")));

        match parsed {
            Ok(meta) if !meta.is_empty() => Ok(Some(meta)),
            Ok(_) => Ok(None),
            Err(e) => {
                tracing::warn!(
                    path = %path,
                    error = %e,
                    "Failed to parse metadata, skipping"
                );
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_storage::StorageErrorKind;
    use std::assert_matches;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_fs_storage_is_send_sync() {
        assert_send_sync::<FsStorage>();
    }

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_sidecar_combines_metadata_and_content() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original H1\n\nBody.").unwrap();
        fs::write(
            temp_dir.path().join("guide.meta.yaml"),
            "title: Sidecar Title\nkind: guide",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        let doc = docs.iter().find(|d| d.path == "guide").unwrap();
        assert!(doc.has_content);
        assert_eq!(doc.title, "Sidecar Title"); // sidecar wins over H1
        assert_eq!(doc.page_kind, Some("guide".to_owned()));

        // Content still served from the .md file.
        assert_eq!(storage.read("guide").unwrap(), "# Original H1\n\nBody.");
    }

    #[test]
    fn test_meta_directory_wins_over_sibling_on_collision() {
        let temp_dir = create_test_dir();
        // Directory form for url path "foo".
        let foo_dir = temp_dir.path().join("foo");
        fs::create_dir(&foo_dir).unwrap();
        fs::write(foo_dir.join("meta.yaml"), "title: Directory Foo").unwrap();
        // Sibling form for the same url path "foo".
        fs::write(temp_dir.path().join("foo.meta.yaml"), "title: Sibling Foo").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("foo").unwrap().unwrap();
        assert_eq!(meta.title, Some("Directory Foo".to_owned()));
    }

    #[test]
    fn test_index_variant_alone_titles_directory() {
        let temp_dir = create_test_dir();
        let dir = temp_dir.path().join("my-domain");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("index.meta.yaml"), "kind: domain").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        let doc = docs.iter().find(|d| d.path == "my-domain").unwrap();
        assert!(!doc.has_content);
        assert_eq!(doc.title, "My Domain"); // titlecased directory name
        assert_eq!(doc.page_kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_meta_sibling_leaf_own_metadata() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("payments.meta.yaml"),
            "title: Payments Service\nkind: component",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("payments").unwrap().unwrap();

        assert_eq!(meta.title, Some("Payments Service".to_owned()));
        assert_eq!(meta.page_kind, Some("component".to_owned()));
    }

    #[test]
    fn test_meta_sibling_does_not_cascade_to_descendants() {
        let temp_dir = create_test_dir();
        // Sibling meta at url path "foo" with a distinctive title.
        fs::write(
            temp_dir.path().join("foo.meta.yaml"),
            "title: Sibling Title",
        )
        .unwrap();
        // A real nested page under directory "foo".
        let foo_dir = temp_dir.path().join("foo");
        fs::create_dir(&foo_dir).unwrap();
        fs::write(foo_dir.join("bar.md"), "# Bar").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("foo/bar").unwrap();

        // The sibling foo.meta.yaml must NOT cascade its title into foo/bar:
        // either foo/bar resolves to no metadata at all, or its metadata's
        // title isn't the sibling's.
        let cascaded = meta.is_some_and(|m| m.title.as_deref() == Some("Sibling Title"));
        assert!(
            !cascaded,
            "sibling meta is leaf-only and must not cascade its title to descendants"
        );
    }

    #[test]
    fn test_meta_index_variant_does_not_cascade_to_descendants() {
        let temp_dir = create_test_dir();
        let dir = temp_dir.path().join("dir");
        fs::create_dir(&dir).unwrap();
        // Directory metadata via the index.meta.yaml variant, for "dir" itself.
        fs::write(dir.join("index.meta.yaml"), "title: Dir Title").unwrap();
        fs::write(dir.join("child.md"), "# Child").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // "dir" still has its own metadata.
        assert_eq!(
            storage.meta("dir").unwrap().unwrap().title,
            Some("Dir Title".to_owned())
        );

        // But it must not cascade down to a child page.
        let meta = storage.meta("dir/child").unwrap();
        let cascaded = meta.is_some_and(|m| m.title.as_deref() == Some("Dir Title"));
        assert!(
            !cascaded,
            "index.meta.yaml directory metadata must not cascade its title to descendants"
        );
    }

    #[test]
    fn test_exists_for_content_less_sibling() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("payments.meta.yaml"),
            "kind: component",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        assert!(storage.exists("payments"));
    }

    #[test]
    fn test_scan_content_less_sibling_virtual_page() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("payments.meta.yaml"),
            "kind: component\nnamespace: billing",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        let doc = docs.iter().find(|d| d.path == "payments").unwrap();
        assert!(!doc.has_content);
        assert_eq!(doc.title, "Payments"); // titlecased from url segment
        assert_eq!(doc.page_kind, Some("component".to_owned()));
        assert_eq!(doc.namespace, Some("billing".to_owned()));
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
    fn test_scan_extracts_page_kind() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("meta.yaml"), "kind: domain").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.path, "domain");
        assert!(doc.has_content);
        assert_eq!(doc.page_kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_scan_with_custom_meta_filename() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("config.yml"), "kind: section").unwrap();
        fs::write(domain_dir.join("meta.yaml"), "ignored").unwrap(); // Should be ignored

        let storage = FsStorage::with_meta_filename(temp_dir.path().to_path_buf(), "config.yml");
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert!(doc.has_content);
        assert_eq!(doc.page_kind, Some("section".to_owned()));
    }

    #[test]
    fn test_scan_no_page_kind_without_kind_field() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "title: Home Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.path, "");
        assert!(doc.has_content);
        assert!(doc.page_kind.is_none()); // No kind field in metadata
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
        assert!(doc.page_kind.is_none());
    }

    #[test]
    fn test_scan_virtual_page_with_type() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("my-nice-domain");
        fs::create_dir(&domain_dir).unwrap();
        // No title but has kind in meta.yaml
        fs::write(domain_dir.join("meta.yaml"), "kind: domain").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.title, "My Nice Domain"); // Fallback to directory name
        assert_eq!(doc.page_kind, Some("domain".to_owned()));
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
            "title: Domain Title\nkind: domain",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("domain").unwrap();

        assert!(meta.is_some());
        let meta = meta.unwrap();
        assert_eq!(meta.title, Some("Domain Title".to_owned()));
        assert_eq!(meta.page_kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_meta_for_root() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "title: Home").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("").unwrap();

        assert!(meta.is_some());
        assert_eq!(meta.unwrap().title, Some("Home".to_owned()));
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
    fn meta_reads_the_pages_own_fields() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(
            domain_dir.join("meta.yaml"),
            "title: Domain\ndescription: Domain docs\nkind: domain\npages:\n  - overview\n  - api",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("domain").unwrap().unwrap();

        assert_eq!(meta.title, Some("Domain".to_owned()));
        assert_eq!(meta.description, Some("Domain docs".to_owned()));
        assert_eq!(meta.page_kind, Some("domain".to_owned()));
        assert_eq!(
            meta.pages,
            Some(vec!["overview".to_owned(), "api".to_owned()])
        );
    }

    #[test]
    fn meta_does_not_inherit_any_field_from_an_ancestor() {
        let temp_dir = create_test_dir();
        let parent = temp_dir.path().join("parent");
        fs::create_dir(&parent).unwrap();
        fs::write(parent.join("index.md"), "# Parent").unwrap();
        fs::write(
            parent.join("meta.yaml"),
            "title: Parent Title\ndescription: Parent Desc\nkind: domain\npages:\n  - child",
        )
        .unwrap();

        // Child has content but declares no metadata of its own.
        let child = parent.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("index.md"), "# Child").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // Nothing cascades down, so there is no metadata at all for the child.
        assert!(storage.meta("parent/child").unwrap().is_none());
    }

    #[test]
    fn meta_ignores_a_legacy_vars_key_and_reads_the_rest() {
        let temp_dir = create_test_dir();
        let guide = temp_dir.path().join("guide");
        fs::create_dir(&guide).unwrap();
        fs::write(guide.join("index.md"), "# Guide").unwrap();
        fs::write(
            guide.join("meta.yaml"),
            "title: Guide\nvars:\n  owner: team-a\n  env: prod",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let meta = storage.meta("guide").unwrap().unwrap();

        assert_eq!(meta.title, Some("Guide".to_owned()));
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
        assert_eq!(err.kind, StorageErrorKind::NotFound);
        assert_eq!(err.backend, Some("Fs"));
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
    fn test_mtime_cache_detects_markdown_changes() {
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
    fn test_mtime_cache_detects_meta_yaml_changes() {
        let temp_dir = create_test_dir();
        let guide_dir = temp_dir.path().join("guide");
        fs::create_dir(&guide_dir).unwrap();
        fs::write(guide_dir.join("index.md"), "# H1 Title").unwrap();
        fs::write(guide_dir.join("meta.yaml"), "title: YAML Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // First scan — meta.yaml title wins over H1
        let docs1 = storage.scan().unwrap();
        let guide1 = docs1.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(guide1.title, "YAML Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify only meta.yaml, not the markdown file
        fs::write(guide_dir.join("meta.yaml"), "title: New YAML Title").unwrap();

        // Second scan should see new title from meta.yaml
        let docs2 = storage.scan().unwrap();
        let guide2 = docs2.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(guide2.title, "New YAML Title");
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

    /// Create a git repo with one file committed at an explicit old date
    /// (2020-01-01), signing disabled. Returns the tempdir.
    fn git_repo_with_old_commit(rel_file: &str, contents: &str) -> tempfile::TempDir {
        use std::process::Command;
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap();
        };
        // Branch "test", not "main": a global hook here blocks commits to main.
        run(&["init", "-b", "test"]);
        run(&["config", "user.email", "t@t.com"]);
        run(&["config", "user.name", "T"]);
        run(&["config", "commit.gpgsign", "false"]);
        let file = dir.path().join(rel_file);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, contents).unwrap();
        run(&["add", "."]);
        Command::new("git")
            .args(["commit", "-m", "old"])
            .env("GIT_AUTHOR_DATE", "2020-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2020-01-01T00:00:00Z")
            .current_dir(dir.path())
            .output()
            .unwrap();
        dir
    }

    #[test]
    fn git_mode_returns_commit_time_filesystem_mode_returns_fs_time() {
        // Keep the file at the repo root (like the rw-vcs Vcs tests) and use the
        // repo path directly as source_dir, so gix's workdir and the resolved
        // file path share the same form on every platform. A docs/ subdir plus
        // fs::canonicalize tripped repo_relative_path's strip_prefix — the macOS
        // /var -> /private/var symlink one way, the Windows \\?\ verbatim prefix
        // the other — making git mode fall back to fs and defeating the test.
        let dir = git_repo_with_old_commit("guide.md", "# Guide");
        let source_dir = dir.path().to_path_buf();

        // Git mode: the 2020 commit time (well before 1_600_000_000 = 2020-09).
        let git = FsStorage::new(source_dir.clone()).with_mtime_source(MtimeSource::Git);
        let git_mtime = git.mtime("guide").unwrap();
        assert!(
            git_mtime < 1_600_000_000.0,
            "git mtime {git_mtime} should be the 2020 commit time"
        );

        // Filesystem mode (the default): the file's on-disk mtime = ~now.
        let fs = FsStorage::new(source_dir);
        let fs_mtime = fs.mtime("guide").unwrap();
        assert!(
            fs_mtime > 1_600_000_000.0,
            "fs mtime {fs_mtime} should be ~now, not the commit time"
        );
    }

    #[test]
    fn git_mode_resolves_commit_time_for_readme_only_project() {
        // README-only site: no docs/ dir, so the default source_dir points at a
        // non-existent <repo>/docs. Git discovery must climb to the repo root
        // and still report the README's commit time — not the fs checkout time.
        // Use dir.path() as-is (do NOT canonicalize): discovery climbs to it, so
        // gix's workdir and the resolved README path share the same form and
        // repo_relative_path strips cleanly on every platform. Canonicalizing
        // introduces the Windows `\\?\` verbatim prefix (and the macOS
        // /var -> /private/var swap) that gix's workdir lacks, defeating the
        // strip. Same rationale as git_mode_returns_commit_time_*.
        let dir = git_repo_with_old_commit("README.md", "# Hi");
        let missing_source_dir = dir.path().join("docs");
        assert!(!missing_source_dir.exists());

        let storage = FsStorage::new(missing_source_dir).with_mtime_source(MtimeSource::Git);

        // The homepage ("") resolves to the root README; its mtime must be the
        // 2020 commit time (< 2020-09), proving git discovery succeeded.
        let mtime = storage.mtime("").unwrap();
        assert!(
            mtime < 1_600_000_000.0,
            "expected the 2020 commit time, got {mtime} \
             (a value near now means discovery failed and it used fs mtime)",
        );
    }

    #[test]
    fn git_mode_falls_back_to_fs_time_with_no_repo() {
        // Git requested but the tree is not a git repo at all. The observable
        // contract asserted here is the filesystem-time fallback; the accompanying
        // warning in with_mtime_source is a side effect this test does not capture.
        let dir = create_test_dir();
        fs::write(dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(dir.path().to_path_buf()).with_mtime_source(MtimeSource::Git);
        let mtime = storage.mtime("guide").unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert!(mtime > now - 60.0 && mtime <= now);
    }

    #[test]
    fn test_mtime_returns_modification_time() {
        // `create_test_dir` is a bare (non-git) tempdir and `FsStorage::new`
        // defaults to `MtimeSource::Filesystem`, so this also covers the
        // filesystem default working with no git repo present.
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
        assert_eq!(err.kind, StorageErrorKind::NotFound);
        assert_eq!(err.backend, Some("Fs"));
    }

    #[test]
    fn test_read_rejects_path_traversal() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.read("../etc/passwd");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::InvalidPath);
        assert_eq!(err.backend, Some("Fs"));
    }

    #[test]
    fn test_read_rejects_nested_path_traversal() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.read("subdir/../../etc/passwd");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::InvalidPath);
    }

    #[test]
    fn test_mtime_rejects_path_traversal() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.mtime("../etc/passwd");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::InvalidPath);
        assert_eq!(err.backend, Some("Fs"));
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

    #[test]
    fn test_watch_succeeds_when_source_dir_missing() {
        // A README-only project: source_dir (docs/) does not exist.
        let temp_dir = create_test_dir();
        let missing = temp_dir.path().join("docs");
        assert!(!missing.exists());

        let storage = FsStorage::new(missing);
        // watch() must not fail just because docs/ is absent.
        assert!(storage.watch().is_ok());
    }

    #[test]
    fn test_watch_succeeds_with_relative_missing_source_dir() {
        // Relative source_dir whose parent() is the empty path must not error.
        // Assert only Ok — do not depend on any README.md in the test's cwd.
        let storage = FsStorage::new(PathBuf::from("nonexistent-docs-rw-test"));
        assert!(storage.watch().is_ok());
    }

    #[test]
    fn test_scan_injects_readme_when_docs_missing() {
        // source_dir (docs/) absent, README.md present in its parent.
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("README.md"), "# Atlas").unwrap();
        let missing = temp_dir.path().join("docs");

        let storage = FsStorage::new(missing);
        let docs = storage.scan().unwrap();

        let home = docs.iter().find(|d| d.path.is_empty());
        assert!(home.is_some(), "README.md should be injected as homepage");
        assert_eq!(home.unwrap().title, "Atlas");
    }

    #[test]
    fn test_try_upgrade_recursive_watch_when_dir_absent() {
        // Helper returns false and records nothing when the dir does not exist.
        let temp_dir = create_test_dir();
        let missing = temp_dir.path().join("docs");

        let watcher = parking_lot::Mutex::new(
            notify::recommended_watcher(|_res: Result<notify::Event, notify::Error>| {}).unwrap(),
        );
        // Zero debounce window so any recorded event would be immediately drainable.
        let debouncer = EventDebouncer::new(Duration::from_millis(0));

        assert!(!try_upgrade_recursive_watch(&watcher, &debouncer, &missing));
        assert!(
            debouncer.drain_ready().is_empty(),
            "no event should be recorded when dir is absent"
        );
    }

    #[test]
    fn test_try_upgrade_recursive_watch_when_path_is_file() {
        // A non-directory at the source path must not flip the upgrade to active:
        // the helper guards on is_dir(), not exists(), so a file created at the
        // path (possibly before the real directory replaces it) records nothing.
        let temp_dir = create_test_dir();
        let file_path = temp_dir.path().join("docs");
        fs::write(&file_path, "not a directory").unwrap();

        let watcher = parking_lot::Mutex::new(
            notify::recommended_watcher(|_res: Result<notify::Event, notify::Error>| {}).unwrap(),
        );
        let debouncer = EventDebouncer::new(Duration::from_millis(0));

        assert!(!try_upgrade_recursive_watch(
            &watcher, &debouncer, &file_path
        ));
        assert!(
            debouncer.drain_ready().is_empty(),
            "no event should be recorded when the path is a file, not a directory"
        );
    }

    #[test]
    fn test_try_upgrade_recursive_watch_when_dir_present() {
        // Helper returns true and records a synthetic Created once the dir exists.
        let temp_dir = create_test_dir();
        let docs = temp_dir.path().join("docs");
        fs::create_dir(&docs).unwrap();

        let watcher = parking_lot::Mutex::new(
            notify::recommended_watcher(|_res: Result<notify::Event, notify::Error>| {}).unwrap(),
        );
        // Zero debounce window so the synthetic event is immediately drainable.
        let debouncer = EventDebouncer::new(Duration::from_millis(0));

        assert!(try_upgrade_recursive_watch(&watcher, &debouncer, &docs));

        let events = debouncer.drain_ready();
        assert_eq!(
            events.len(),
            1,
            "expected one synthetic event, got: {events:?}"
        );
        assert_eq!(events[0].path, docs);
        assert_eq!(events[0].kind, RawEventKind::Created);
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
    fn test_watch_detects_docs_dir_created_after_start() {
        // Start watching a project whose docs/ does not exist yet.
        let temp_dir = create_test_dir();
        let docs = temp_dir.path().join("docs");
        assert!(!docs.exists());

        let storage = FsStorage::new(docs.clone());
        let (rx, _handle) = storage.watch().unwrap();

        // Create docs/ and a page inside it.
        std::thread::sleep(Duration::from_millis(100));
        fs::create_dir(&docs).unwrap();
        fs::write(docs.join("guide.md"), "# Guide").unwrap();

        // Wait for the 50ms poll to upgrade + the debounce window to drain.
        std::thread::sleep(Duration::from_millis(500));

        // Assert the page *inside* the newly created docs/ is observed — not just
        // the synthetic root Created event. This proves the recursive watch was
        // actually started on the new directory, which is the point of the fix.
        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv()).collect();
        assert!(
            events.iter().any(|e| e.path == "guide"),
            "expected an event for the page inside the new docs/, got: {events:?}"
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
        assert_matches!(event.kind, StorageEventKind::Modified { .. });
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
        let storage = FsStorage::new(temp_dir.path().to_path_buf());

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
        assert_matches!(event.unwrap().kind, StorageEventKind::Modified { .. });

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
    /// returning `(temp_dir, project_root, FsStorage)`.
    ///
    /// `FsStorage` auto-detects README.md in `source_dir`'s parent directory.
    fn create_readme_test_dir(readme_content: &str) -> (tempfile::TempDir, PathBuf, FsStorage) {
        let temp_dir = create_test_dir();
        let project_root = temp_dir.path().to_path_buf();
        let source_dir = project_root.join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(project_root.join("README.md"), readme_content).unwrap();

        let storage = FsStorage::new(source_dir);
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
    fn test_scan_sets_origin_on_readme_homepage() {
        let (dir, _, storage) = create_readme_test_dir("# My Project");
        fs::write(dir.path().join("docs/guide.md"), "# Guide").unwrap();
        let docs = storage.scan().unwrap();

        let home = docs.iter().find(|d| d.path.is_empty()).unwrap();
        assert_eq!(home.origin, Some("docs".to_owned()));
        // The README homepage is the root directory page, so its relative links
        // resolve against the root (not nested under a leaf slug).
        assert!(home.is_dir, "README homepage URL is a directory");

        let guide = docs.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(guide.origin, None);
        assert!(!guide.is_dir, "docs/guide.md is a leaf page");
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

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
    fn test_watch_detects_directory_rename() {
        let temp_dir = create_test_dir();
        // Canonicalize to resolve macOS /var → /private/var symlink,
        // since notify fires events with canonical paths.
        let base = fs::canonicalize(temp_dir.path()).unwrap();
        let old_dir = base.join("old-name");
        fs::create_dir(&old_dir).unwrap();
        fs::write(old_dir.join("index.md"), "# Page").unwrap();

        let storage = FsStorage::new(base.clone());
        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(200));

        // Rename directory
        let new_dir = base.join("new-name");
        fs::rename(&old_dir, &new_dir).unwrap();

        // Wait for debounce + processing (generous for rename detection)
        std::thread::sleep(Duration::from_millis(500));

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv()).collect();
        assert!(
            !events.is_empty(),
            "Expected at least one event for directory rename"
        );
    }

    #[test]
    fn scan_pages_from_meta_yaml() {
        let dir = create_test_dir();
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(docs_dir.join("guides")).unwrap();
        fs::write(docs_dir.join("guides/index.md"), "# Guides").unwrap();
        fs::write(
            docs_dir.join("guides/getting-started.md"),
            "# Getting Started",
        )
        .unwrap();
        fs::write(docs_dir.join("guides/configuration.md"), "# Configuration").unwrap();
        fs::write(
            docs_dir.join("guides/meta.yaml"),
            "pages:\n  - getting-started\n  - configuration",
        )
        .unwrap();

        let storage = FsStorage::new(docs_dir);
        let docs = storage.scan().unwrap();

        let guides = docs.iter().find(|d| d.path == "guides").unwrap();
        assert_eq!(
            guides.pages,
            Some(vec![
                "getting-started".to_owned(),
                "configuration".to_owned()
            ])
        );
    }

    #[test]
    fn scan_pages_from_frontmatter() {
        let dir = create_test_dir();
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(docs_dir.join("guides")).unwrap();
        fs::write(
            docs_dir.join("guides/index.md"),
            "---\npages:\n  - alpha\n---\n# Guides",
        )
        .unwrap();
        fs::write(docs_dir.join("guides/alpha.md"), "# Alpha").unwrap();

        let storage = FsStorage::new(docs_dir);
        let docs = storage.scan().unwrap();

        let guides = docs.iter().find(|d| d.path == "guides").unwrap();
        assert_eq!(guides.pages, Some(vec!["alpha".to_owned()]));
    }

    #[test]
    fn scan_no_pages_returns_none() {
        let dir = create_test_dir();
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(docs_dir);
        let docs = storage.scan().unwrap();

        let guide = docs.iter().find(|d| d.path == "guide").unwrap();
        assert!(guide.pages.is_none());
    }

    #[test]
    fn scan_populates_document_namespace_from_meta_yaml() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("billing");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Billing").unwrap();
        fs::write(
            domain_dir.join("meta.yaml"),
            "kind: domain\nnamespace: payments",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "billing").unwrap();
        assert_eq!(doc.namespace.as_deref(), Some("payments"));
    }

    #[test]
    fn scan_rejects_invalid_namespace() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "namespace: bad/value").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let err = storage.scan().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("bad/value"),
            "error should name the value: {msg}"
        );
    }

    #[test]
    fn scan_error_names_meta_yaml_when_namespace_in_sidecar() {
        // Namespace declared in meta.yaml — error attributes to it, not the
        // companion index.md.
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "namespace: bad/value").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let err = storage.scan().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("meta.yaml"),
            "error should name meta.yaml: {msg}"
        );
    }

    #[test]
    fn test_to_storage_event_named_sibling_routes_to_sibling_path() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let source_dir = Path::new("/docs");
        let event = DebouncedEvent {
            path: PathBuf::from("/docs/systems/payments.meta.yaml"),
            kind: RawEventKind::Removed,
        };
        let resolver = PathResolver::new(source_dir.to_path_buf(), "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver);
        assert_eq!(storage_event.path, "systems/payments");
    }

    #[test]
    fn test_to_storage_event_index_variant_routes_to_directory() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let source_dir = Path::new("/docs");
        let event = DebouncedEvent {
            path: PathBuf::from("/docs/dir/index.meta.yaml"),
            kind: RawEventKind::Removed,
        };
        let resolver = PathResolver::new(source_dir.to_path_buf(), "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver);
        assert_eq!(storage_event.path, "dir"); // NOT "dir/index"
    }

    #[test]
    fn test_to_storage_event_bare_meta_routes_to_directory() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let source_dir = Path::new("/docs");
        let event = DebouncedEvent {
            path: PathBuf::from("/docs/dir/meta.yaml"),
            kind: RawEventKind::Removed,
        };
        let resolver = PathResolver::new(source_dir.to_path_buf(), "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver);
        assert_eq!(storage_event.path, "dir");
    }

    #[test]
    fn test_modified_event_carries_sibling_title() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("payments.meta.yaml"),
            "title: Payments Service",
        )
        .unwrap();

        let event = DebouncedEvent {
            path: temp_dir.path().join("payments.meta.yaml"),
            kind: RawEventKind::Modified,
        };
        let resolver = PathResolver::new(temp_dir.path().to_path_buf(), "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver);
        assert_eq!(storage_event.path, "payments");
        match storage_event.kind {
            StorageEventKind::Modified { title, .. } => {
                assert_eq!(title, "Payments Service");
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    #[test]
    fn test_modified_event_on_readme_homepage_uses_its_h1() {
        // Regression test for the drift this refactor removes: the free
        // `resolve_meta` had no README fallback, so a live-reload on a
        // README-only project (no docs/index.md) reported the root page's
        // title as the "home" slug fallback instead of the README's own H1.
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let project_root = create_test_dir();
        let source_dir = project_root.path().join("docs"); // does not exist
        fs::write(project_root.path().join("README.md"), "# Readme Home Title").unwrap();

        let event = DebouncedEvent {
            path: project_root.path().join("README.md"),
            kind: RawEventKind::Modified,
        };
        let resolver = PathResolver::new(source_dir, "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver);
        assert_eq!(storage_event.path, "");
        match storage_event.kind {
            StorageEventKind::Modified { title, .. } => {
                assert_eq!(title, "Readme Home Title");
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    #[test]
    fn readme_homepage_without_h1_titles_the_same_on_scan_and_event() {
        // Pins the two paths to one fallback name. An H1-less README is titled
        // by `scan` (via the injected homepage document) and by the watch path
        // (via `content_fallback_name`); both now read HOMEPAGE_FALLBACK_NAME,
        // and this asserts they cannot drift back onto separate literals.
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(root.join("README.md"), "Body with no heading.").unwrap();

        let storage = FsStorage::new(docs.clone());
        let scan_title = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path.is_empty())
            .expect("README homepage document")
            .title;

        let resolver = PathResolver::new(docs, "meta.yaml");
        let event = DebouncedEvent {
            path: root.join("README.md"),
            kind: RawEventKind::Modified,
        };
        let StorageEventKind::Modified {
            title: event_title, ..
        } = to_storage_event(&event, &resolver).kind
        else {
            panic!("expected Modified");
        };

        assert_eq!(
            scan_title, "Home",
            "H1-less README titles from HOMEPAGE_FALLBACK_NAME"
        );
        assert_eq!(
            scan_title, event_title,
            "scan and watch paths must title an H1-less README identically"
        );
    }

    #[test]
    fn test_is_hidden_rel_path() {
        use std::path::Path;
        assert!(is_hidden_rel_path(Path::new(".meta.yaml")));
        assert!(is_hidden_rel_path(Path::new("dir/.hidden.md")));
        assert!(is_hidden_rel_path(Path::new(".rw/cache/x")));
        assert!(!is_hidden_rel_path(Path::new("dir/visible.md")));
        assert!(!is_hidden_rel_path(Path::new("payments.meta.yaml")));
        assert!(!is_hidden_rel_path(Path::new("")));
    }

    // --- url_paths_for_source ---

    /// Create a test project: `<tmp>/README.md`, `<tmp>/docs/` with several pages.
    /// Storage `source_dir` is `<tmp>/docs`.
    fn make_url_paths_storage() -> (tempfile::TempDir, FsStorage) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let docs = root.join("docs");
        fs::create_dir_all(docs.join("billing")).unwrap();

        // content files
        fs::write(docs.join("index.md"), "# Home").unwrap();
        fs::write(docs.join("guide.md"), "# Guide").unwrap();
        fs::write(docs.join("billing/index.md"), "# Billing").unwrap();
        fs::write(docs.join("billing/overview.md"), "# Overview").unwrap();
        // metadata file (should never appear in output)
        fs::write(docs.join("meta.yaml"), "title: Site").unwrap();
        // README.md in parent (homepage fallback)
        fs::write(root.join("README.md"), "# README Home").unwrap();
        // a file completely outside source_dir
        fs::create_dir_all(root.join("elsewhere")).unwrap();
        fs::write(root.join("elsewhere/x.md"), "# X").unwrap();

        let storage = FsStorage::new(docs);
        (tmp, storage)
    }

    #[test]
    fn url_paths_for_source_dir_relative_index() {
        let (_tmp, storage) = make_url_paths_storage();
        // source_dir-relative: "index.md" → root page
        assert_eq!(
            storage.url_paths_for_source(Path::new("index.md")),
            vec![String::new()]
        );
    }

    #[test]
    fn url_paths_for_source_dir_relative_nested() {
        let (_tmp, storage) = make_url_paths_storage();
        // source_dir-relative nested: "billing/overview.md"
        assert_eq!(
            storage.url_paths_for_source(Path::new("billing/overview.md")),
            vec!["billing/overview".to_owned()]
        );
    }

    #[test]
    fn url_paths_for_source_project_root_relative_index() {
        let (_tmp, storage) = make_url_paths_storage();
        // project-root-relative (prefixed): "docs/index.md" → root page
        assert_eq!(
            storage.url_paths_for_source(Path::new("docs/index.md")),
            vec![String::new()]
        );
    }

    #[test]
    fn url_paths_for_source_project_root_relative_nested() {
        let (_tmp, storage) = make_url_paths_storage();
        // project-root-relative nested: "docs/billing/overview.md"
        assert_eq!(
            storage.url_paths_for_source(Path::new("docs/billing/overview.md")),
            vec!["billing/overview".to_owned()]
        );
    }

    #[test]
    fn url_paths_for_source_absolute_under_source_dir() {
        let (tmp, storage) = make_url_paths_storage();
        // absolute path under source_dir
        let abs = tmp.path().join("docs/guide.md");
        assert_eq!(storage.url_paths_for_source(&abs), vec!["guide".to_owned()]);
    }

    #[test]
    fn url_paths_for_source_nested_index_md() {
        let (_tmp, storage) = make_url_paths_storage();
        // "docs/billing/index.md" → "billing"
        assert_eq!(
            storage.url_paths_for_source(Path::new("docs/billing/index.md")),
            vec!["billing".to_owned()]
        );
    }

    #[test]
    fn url_paths_for_source_readme_homepage_when_no_index() {
        // No docs/index.md: the parent README.md IS the served homepage → root.
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("guide.md"), "# Guide").unwrap();
        fs::write(tmp.path().join("README.md"), "# Home").unwrap();
        let storage = FsStorage::new(docs);
        assert_eq!(
            storage.url_paths_for_source(Path::new("README.md")),
            vec![String::new()]
        );
    }

    #[test]
    fn url_paths_for_source_readme_shadowed_by_index_is_empty() {
        // docs/index.md is the homepage, so the parent README.md is not a served
        // page — it must NOT be mapped to the root url.
        let (_tmp, storage) = make_url_paths_storage();
        assert!(
            storage
                .url_paths_for_source(Path::new("README.md"))
                .is_empty()
        );
    }

    #[test]
    fn url_paths_for_source_meta_yaml_is_empty() {
        let (_tmp, storage) = make_url_paths_storage();
        // metadata file → no pages
        assert!(
            storage
                .url_paths_for_source(Path::new("meta.yaml"))
                .is_empty()
        );
    }

    #[test]
    fn url_paths_for_source_nonexistent_is_empty() {
        let (_tmp, storage) = make_url_paths_storage();
        assert!(
            storage
                .url_paths_for_source(Path::new("nope.md"))
                .is_empty()
        );
    }

    #[test]
    fn url_paths_for_source_outside_source_dir_is_empty() {
        let (tmp, storage) = make_url_paths_storage();
        let p = tmp.path().join("elsewhere/x.md");
        assert!(storage.url_paths_for_source(&p).is_empty());
    }

    #[test]
    fn url_paths_for_source_ambiguity_returns_both() {
        // When both "docs/guide.md" (source_dir-relative → page "docs/guide") and
        // the prefix-stripped "guide.md" (→ page "guide") exist, the input
        // "docs/guide.md" is ambiguous and returns both interpretations.
        let (tmp, storage) = make_url_paths_storage();
        let docs = tmp.path().join("docs");

        // Create docs/docs/guide.md so the verbatim "docs/guide.md" path is also
        // a real source_dir-relative file mapping to page "docs/guide".
        fs::create_dir_all(docs.join("docs")).unwrap();
        fs::write(docs.join("docs/guide.md"), "# Docs Guide").unwrap();

        let mut got = storage.url_paths_for_source(Path::new("docs/guide.md"));
        got.sort();
        assert_eq!(got.len(), 2, "expected 2 interpretations, got: {got:?}");
        assert!(
            got.contains(&"guide".to_owned()),
            "missing 'guide' in {got:?}"
        );
        assert!(
            got.contains(&"docs/guide".to_owned()),
            "missing 'docs/guide' in {got:?}"
        );
    }

    #[test]
    fn test_scan_classifies_is_dir() {
        let temp_dir = create_test_dir();
        // Leaf page: docs/guide.md  -> URL "guide", is_dir = false.
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();
        // Index page: docs/domain/index.md -> URL "domain", is_dir = true.
        let domain = temp_dir.path().join("domain");
        fs::create_dir(&domain).unwrap();
        fs::write(domain.join("index.md"), "# Domain").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        let guide = docs.iter().find(|d| d.path == "guide").unwrap();
        assert!(
            !guide.is_dir,
            "leaf guide.md URL is a file, not a directory"
        );

        let domain = docs.iter().find(|d| d.path == "domain").unwrap();
        assert!(domain.is_dir, "domain/index.md URL is a directory");
    }

    /// The scan path (`Scanner` + `MetaRank` ordinal tie-break) and the watch
    /// path (`PathResolver` probe order) encode the same precedence in
    /// different shapes.
    /// Nothing in the type system links them, so pin them against each other
    /// across the full matrix.
    ///
    /// Deliberately does not cover the root url of a README-homepage site: `scan`
    /// injects that document itself and passes no meta.yaml, while the resolver
    /// would find `docs/meta.yaml` for the root. The two disagree there; nothing
    /// yet depends on them agreeing. Widening this test to the root url is the
    /// check that would catch it. (No issue filed — file one before relying on it.)
    #[test]
    fn scan_and_resolver_agree_across_the_precedence_matrix() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Every metadata form, each on its own url so ranks never tie.
        fs::create_dir_all(root.join("canonical")).unwrap();
        fs::write(root.join("canonical/index.md"), "# Canonical").unwrap();
        fs::write(root.join("canonical/meta.yaml"), "title: Canonical").unwrap();
        fs::write(root.join("canonical/index.meta.yaml"), "title: Loser").unwrap();

        fs::create_dir_all(root.join("variant")).unwrap();
        fs::write(root.join("variant/index.md"), "# Variant").unwrap();
        fs::write(root.join("variant/index.meta.yaml"), "title: Variant").unwrap();

        fs::write(root.join("sibling.md"), "# Sibling").unwrap();
        fs::write(root.join("sibling.meta.yaml"), "title: Sibling").unwrap();

        // Content precedence: index.md must beat the standalone of the same name.
        fs::create_dir_all(root.join("both")).unwrap();
        fs::write(root.join("both/index.md"), "# Both Index").unwrap();
        fs::write(root.join("both.md"), "# Both Standalone").unwrap();

        let storage = FsStorage::new(root.to_path_buf());
        let resolver = PathResolver::new(root.to_path_buf(), "meta.yaml");
        let refs = storage.scanner.scan();

        let mut urls: Vec<_> = refs.iter().map(|r| r.url_path.clone()).collect();
        urls.sort();
        assert_eq!(
            urls,
            ["both", "canonical", "sibling", "variant"],
            "the fixture's pages, so the loop below cannot agree vacuously"
        );

        for doc_ref in &refs {
            assert_eq!(
                doc_ref.meta_path,
                resolver.resolve_meta(&doc_ref.url_path),
                "scan and resolver disagree on the metadata file for url {:?}",
                doc_ref.url_path
            );
            assert_eq!(
                doc_ref.content_path,
                resolver.resolve_content(&doc_ref.url_path),
                "scan and resolver disagree on the content file for url {:?}",
                doc_ref.url_path
            );
        }
    }
}
