//! Filesystem storage implementation for RW documentation engine.
//!
//! This crate provides [`FsStorage`], a filesystem-based implementation of the
//! [`Storage`] trait. It handles:
//!
//! - Recursive directory scanning for markdown files
//! - Metadata extraction (title, description, kind) with mtime caching
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
//! let storage = FsStorage::new(PathBuf::from("."), PathBuf::from("docs"));
//! let documents = storage.scan()?;
//! for doc in documents {
//!     println!("{}: {}", doc.path, doc.meta.title);
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
use std::path::{self, Component, Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use glob::Pattern;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use rw_meta::{Diagnostic, DiagnosticSource, Meta};
use rw_vcs::Vcs;

use debouncer::{DebouncedEvent, EventDebouncer, RawEventKind};
use rw_storage::{
    Document, Storage, StorageError, StorageErrorKind, StorageEvent, StorageEventKind,
    StorageEventReceiver, WatchHandle,
};
use scanner::DocumentRef;
use source::{Classification, PathResolver, file_path_to_url};

/// Backend identifier for error messages.
const BACKEND: &str = "Fs";

/// Convert a `notify::EventKind` to a `RawEventKind`.
///
/// Returns `None` for event kinds that are not relevant (e.g., Access).
fn storage_event_kind(kind: EventKind) -> Option<RawEventKind> {
    match kind {
        EventKind::Create(_) => Some(RawEventKind::Created),
        EventKind::Modify(_) => Some(RawEventKind::Modified),
        EventKind::Remove(_) => Some(RawEventKind::Removed),
        _ => None,
    }
}

fn watch_error(e: notify::Error) -> StorageError {
    StorageError::new(StorageErrorKind::Other)
        .with_backend(BACKEND)
        .with_source(e)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceStamp {
    path: PathBuf,
    mtime: Option<SystemTime>,
}

#[derive(Debug)]
struct CachedMeta {
    content: Option<SourceStamp>,
    sidecar: Option<SourceStamp>,
    meta: Arc<Meta>,
    diagnostics: Arc<[Diagnostic]>,
}

/// The one place a page's [`Meta`] is read and parsed from its source files.
///
/// Shared by `scan` and the watch drain thread: a `Modified` event's meta must
/// equal the scanned one, or the live-reload manager reloads structure on every
/// edit. Sharing the cache, too, needs no bypass for the watch path: an edit
/// moves the stamped mtime, and an edit that does not would leave the rescan
/// it triggers stale anyway.
#[derive(Debug, Clone, Default)]
struct MetaCache(Arc<RwLock<HashMap<String, CachedMeta>>>);

impl MetaCache {
    fn source_stamp(path: Option<&Path>) -> Option<SourceStamp> {
        path.map(|path| SourceStamp {
            path: path.to_path_buf(),
            mtime: fs::metadata(path)
                .ok()
                .and_then(|meta| meta.modified().ok()),
        })
    }

    /// `None` when the sources form no page: neither exists, or a content-less
    /// page's sidecar is unreadable or empty.
    fn resolve(
        &self,
        resolver: &PathResolver,
        url_path: &str,
        content_path: Option<&Path>,
        sidecar_path: Option<&Path>,
    ) -> Option<(Arc<Meta>, Arc<[Diagnostic]>)> {
        let content = Self::source_stamp(content_path);
        let sidecar = Self::source_stamp(sidecar_path);

        if content.is_none() && sidecar.is_none() {
            self.0.write().remove(url_path);
            return None;
        }

        {
            let cache = self.0.read();
            if let Some(cached) = cache.get(url_path)
                && cached.content == content
                && cached.sidecar == sidecar
            {
                return Some((Arc::clone(&cached.meta), Arc::clone(&cached.diagnostics)));
            }
        }

        let meta_yaml = sidecar_path.and_then(|path| fs::read_to_string(path).ok());
        let markdown = if let Some(content_path) = content_path {
            fs::read_to_string(content_path).ok()
        } else {
            if meta_yaml
                .as_deref()
                .is_none_or(|yaml| yaml.trim().is_empty())
            {
                self.0.write().remove(url_path);
                return None;
            }
            None
        };
        let fallback = resolver.content_fallback_name(url_path, content_path);
        let fresh =
            Meta::resolve_with_diagnostics(markdown.as_deref(), meta_yaml.as_deref(), &fallback);

        // Fresh resolve: log each problem against the file that caused it.
        // Cache hits above stay silent — a problem is news once, not per scan.
        let diagnostics: Arc<[Diagnostic]> = Arc::from(fresh.diagnostics);
        for diagnostic in diagnostics.iter() {
            let file = match diagnostic.source {
                DiagnosticSource::Frontmatter => content_path,
                DiagnosticSource::Sidecar => sidecar_path,
            };
            if let Some(file) = file {
                tracing::warn!("{}: {diagnostic}", file.display());
            } else {
                tracing::warn!("{diagnostic}");
            }
        }

        let meta = Arc::new(fresh.meta);
        self.0.write().insert(
            url_path.to_owned(),
            CachedMeta {
                content,
                sidecar,
                meta: Arc::clone(&meta),
                diagnostics: Arc::clone(&diagnostics),
            },
        );

        Some((meta, diagnostics))
    }
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
/// let storage = FsStorage::new(PathBuf::from("."), PathBuf::from("docs"));
/// let documents = storage.scan()?;
/// for doc in documents {
///     println!("{}: {}", doc.path, doc.meta.title);
/// }
/// # Ok(())
/// # }
/// ```
pub struct FsStorage {
    /// Cloned into the watch drain thread, which resolves through the same rules.
    resolver: PathResolver,
    /// Project root — see `rw_config::Config::project_dir`. Stored for git
    /// repository discovery in [`FsStorage::with_mtime_source`]; the `README.md`
    /// homepage candidate is baked into `resolver` at construction and does not
    /// read this field.
    project_dir: PathBuf,
    /// Cloned into the watch drain thread along with `resolver`.
    meta_cache: MetaCache,
    /// Glob patterns for file watching (`**/*.md` and metadata files).
    watch_patterns: Vec<Pattern>,
    /// Modification-time source; holds no repository in filesystem mode.
    vcs: Vcs,
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
    /// * `project_dir` - see [`with_meta_filename`](Self::with_meta_filename)
    /// * `source_dir` - Root directory containing markdown files
    #[must_use]
    pub fn new(project_dir: PathBuf, source_dir: PathBuf) -> Self {
        Self::with_meta_filename(project_dir, source_dir, DEFAULT_META_FILENAME)
    }

    /// Create a new filesystem storage with a custom metadata filename.
    ///
    /// Watches `**/*.md`, `**/{meta_filename}`, and `**/*.{meta_filename}` files for changes.
    ///
    /// # Arguments
    ///
    /// * `project_dir` - Project root; the `README.md` homepage fallback and
    ///   git repository discovery are resolved from it
    /// * `source_dir` - Root directory containing markdown files
    /// * `meta_filename` - Name of metadata files (e.g., "meta.yaml")
    ///
    /// Modification times default to [`MtimeSource::Filesystem`]; call
    /// [`with_mtime_source`](Self::with_mtime_source) to opt into git times.
    #[must_use]
    pub fn with_meta_filename(
        project_dir: PathBuf,
        source_dir: PathBuf,
        meta_filename: &str,
    ) -> Self {
        let resolver = PathResolver::new(&project_dir, source_dir, meta_filename);

        Self {
            watch_patterns: resolver.watch_patterns(),
            resolver,
            project_dir,
            meta_cache: MetaCache::default(),
            vcs: Vcs::without_repo(),
        }
    }

    /// Selects the modification-time source (default
    /// [`Filesystem`](MtimeSource::Filesystem)).
    ///
    /// [`Git`](MtimeSource::Git) discovers the repository from `project_dir` and
    /// uses commit times; [`Filesystem`](MtimeSource::Filesystem) does a plain
    /// `stat` and touches no git state.
    #[must_use]
    pub fn with_mtime_source(mut self, source: MtimeSource) -> Self {
        self.vcs = match source {
            MtimeSource::Filesystem => Vcs::without_repo(),
            MtimeSource::Git => {
                let vcs = Vcs::new(&self.project_dir);
                if !vcs.has_repo() {
                    tracing::warn!(
                        project_dir = %self.project_dir.display(),
                        "git modification times requested but no git repository was \
                         found; page times will fall back to filesystem times",
                    );
                }
                vcs
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

    /// Build a `Document` from a `DocumentRef`.
    fn build_document_ref(&self, doc_ref: &DocumentRef) -> Option<Document> {
        let is_dir = doc_ref
            .content_path
            .as_deref()
            .is_none_or(|path| path.file_name().is_some_and(|name| name == "index.md"));

        self.build_document(
            &doc_ref.url_path,
            doc_ref.content_path.as_deref(),
            doc_ref.meta_path.as_deref(),
            None,
            is_dir,
        )
    }

    /// Build a document from selected sources, reusing its resolved [`Meta`].
    /// Metadata problems surface as diagnostics, never errors — a page with
    /// a problem still builds, with the offending fields dropped.
    fn build_document(
        &self,
        url_path: &str,
        content_path: Option<&Path>,
        sidecar_path: Option<&Path>,
        origin: Option<String>,
        is_dir: bool,
    ) -> Option<Document> {
        let (meta, diagnostics) =
            self.meta_cache
                .resolve(&self.resolver, url_path, content_path, sidecar_path)?;

        Some(Document {
            path: url_path.to_owned(),
            has_content: content_path.is_some(),
            meta,
            origin,
            is_dir,
            diagnostics,
        })
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
        debouncer: &Arc<EventDebouncer>,
    ) -> Result<RecommendedWatcher, StorageError> {
        let debouncer = Arc::clone(debouncer);

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let Some(kind) = storage_event_kind(event.kind) else {
                    return;
                };

                for path in event.paths {
                    debouncer.record(path, kind);
                }
            }
        })
        .map_err(watch_error)?;

        watcher
            .watch(readme_path, RecursiveMode::NonRecursive)
            .map_err(watch_error)?;

        Ok(watcher)
    }
}

/// The event for a change to an existing file under `url_path`.
///
/// Locates the sources through `resolver` — what `read()` and `mtime()` use —
/// rather than the scanner, which encodes the same precedence in a different
/// shape; `scan_and_resolver_agree_across_the_precedence_matrix` pins the two.
fn modified_event_kind(
    resolver: &PathResolver,
    meta_cache: &MetaCache,
    url_path: &str,
) -> StorageEventKind {
    let content_path = resolver.resolve_content(url_path);
    let sidecar_path = resolver.resolve_meta(url_path);
    match meta_cache.resolve(
        resolver,
        url_path,
        content_path.as_deref(),
        sidecar_path.as_deref(),
    ) {
        Some((meta, _)) => StorageEventKind::Modified {
            meta: Meta::clone(&meta),
        },
        // Scan has no page here (e.g. a directory, or an emptied sidecar), so
        // there is no meta to compare: only a rescan can tell what changed.
        None => StorageEventKind::Created,
    }
}

/// Whether a path (relative to `source_dir`) has any dot-prefixed component.
///
/// Mirrors the scanner's hidden-file filtering (the `ignore` walker's
/// `.hidden(true)`), so the watch path does not emit events for files the scan
/// ignores — e.g. a hidden `.meta.yaml`, which would otherwise map to a phantom
/// `.meta` url path and trigger a spurious reload.
fn is_hidden_rel_path(rel_path: &Path) -> bool {
    rel_path.components().any(|c| {
        matches!(c, Component::Normal(name)
            if name.to_string_lossy().starts_with('.'))
    })
}

/// The watched `source_dir`, matching event paths in the form the backend
/// reports them.
///
/// notify does not report paths as configured: inotify makes a relative root
/// absolute, and `FSEvents` canonicalizes it, resolving symlinks (macOS `/tmp`,
/// `/var`). Matching only the configured form drops every event then.
struct WatchRoot {
    dir: PathBuf,
    absolute: PathBuf,
    // Resolved lazily: `source_dir` may not exist when the watch starts.
    canonical: Option<PathBuf>,
}

impl WatchRoot {
    fn new(dir: PathBuf) -> Self {
        Self {
            absolute: path::absolute(&dir).unwrap_or_else(|_| dir.clone()),
            dir,
            canonical: None,
        }
    }

    /// `path` re-rooted under the configured `source_dir`, so URL resolution
    /// downstream strips the same prefix; `None` if it lies outside.
    fn reroot(&mut self, path: PathBuf) -> Option<PathBuf> {
        if path.starts_with(&self.dir) {
            return Some(path);
        }
        if self.canonical.is_none() {
            self.canonical = self.dir.canonicalize().ok();
        }
        let rel = [Some(&self.absolute), self.canonical.as_ref()]
            .into_iter()
            .flatten()
            .find_map(|root| path.strip_prefix(root).ok())?;
        Some(self.dir.join(rel))
    }
}

/// Convert a debounced file-system event into a [`StorageEvent`].
///
/// Resolves the file path to a URL path and populates the event kind with
/// resolved metadata for `Modified` events.
fn to_storage_event(
    event: &DebouncedEvent,
    resolver: &PathResolver,
    meta_cache: &MetaCache,
) -> StorageEvent {
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

    let kind = match event.kind.reconcile(event.path.exists()) {
        RawEventKind::Created => StorageEventKind::Created,
        RawEventKind::Modified => modified_event_kind(resolver, meta_cache, &url_path),
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
    watcher: &mut RecommendedWatcher,
    debouncer: &EventDebouncer,
    source_dir: &Path,
) -> bool {
    if !source_dir.is_dir() {
        return false;
    }

    if watcher.watch(source_dir, RecursiveMode::Recursive).is_ok() {
        debouncer.record(source_dir.to_path_buf(), RawEventKind::Created);
        true
    } else {
        false
    }
}

impl Storage for FsStorage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        let t0 = Instant::now();
        let refs = scanner::scan(&self.resolver);
        let walk_elapsed = t0.elapsed();

        let t1 = Instant::now();
        let mut documents: Vec<Document> = refs
            .par_iter()
            .filter_map(|r| self.build_document_ref(r))
            .collect();
        let build_elapsed = t1.elapsed();

        tracing::info!(
            files = refs.len(),
            documents = documents.len(),
            walk_ms = format_args!("{:.1}", walk_elapsed.as_secs_f64() * 1000.0),
            build_ms = format_args!("{:.1}", build_elapsed.as_secs_f64() * 1000.0),
            total_ms = format_args!("{:.1}", t0.elapsed().as_secs_f64() * 1000.0),
            "Storage scan complete"
        );

        // Inject README.md as homepage if no root document found.
        if !documents.iter().any(|d| d.path.is_empty())
            && let Some(readme) = self.resolver.existing_readme()
        {
            let origin = self
                .resolver
                .source_dir()
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned);

            if let Some(document) = self.build_document("", Some(readme), None, origin, true) {
                documents.push(document);
            }
        }

        Ok(documents)
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        Self::validate_path(path)?;
        let full_path = self
            .resolver
            .resolve_content(path)
            .ok_or_else(|| StorageError::not_found(path).with_backend(BACKEND))?;
        fs::read_to_string(&full_path)
            .map_err(|e| StorageError::io(e, Some(PathBuf::from(path))).with_backend(BACKEND))
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        Self::validate_path(path)?;

        let content_path = self.resolver.resolve_content(path);
        let meta_path = self.resolver.resolve_meta(path);

        if content_path.is_none() && meta_path.is_none() {
            return Err(StorageError::not_found(path).with_backend(BACKEND));
        }

        let paths: Vec<&Path> = [&content_path, &meta_path]
            .into_iter()
            .filter_map(|p| p.as_deref())
            .collect();

        Ok(self.vcs.mtime(&paths))
    }

    fn watch(&self) -> Result<(StorageEventReceiver, WatchHandle), StorageError> {
        let (event_tx, event_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let debouncer = Arc::new(EventDebouncer::new(Duration::from_millis(100)));

        let mut root = WatchRoot::new(self.resolver.source_dir().to_path_buf());
        let patterns = self.watch_patterns.clone();
        let debouncer_for_watcher = Arc::clone(&debouncer);

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let Some(kind) = storage_event_kind(event.kind) else {
                    return;
                };

                for path in event.paths {
                    let Some(path) = root.reroot(path) else {
                        continue;
                    };
                    let Ok(rel_path) = path.strip_prefix(&root.dir) else {
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
                    let matches_pattern = path.is_dir()
                        || patterns
                            .iter()
                            .any(|pattern| pattern.matches_path(rel_path));

                    if matches_pattern {
                        debouncer_for_watcher.record(path, kind);
                    }
                }
            }
        })
        .map_err(watch_error)?;

        // Only watch source_dir if it is a directory. A README-only project
        // (no docs/) must still start; the drain thread upgrades to a recursive
        // watch if docs/ appears later. README edits are handled by the README
        // watcher. `is_dir()` (not `exists()`) matches the upgrade helper: a
        // non-directory at the path must not abort the watch with a hard error.
        let mut recursive_active = if self.resolver.source_dir().is_dir() {
            watcher
                .watch(self.resolver.source_dir(), RecursiveMode::Recursive)
                .map_err(watch_error)?;
            true
        } else {
            false
        };

        // The README lives in the parent of source_dir, so it falls outside the
        // recursive watch above and needs its own non-recursive watcher.
        let readme_watcher = self
            .resolver
            .existing_readme()
            .map(|p| Self::watch_readme(p, &debouncer))
            .transpose()?;

        let resolver_for_drain = self.resolver.clone();
        let meta_cache = self.meta_cache.clone();
        thread::spawn(move || {
            // Both watchers stop when dropped, so this thread owns them.
            let _readme_watcher = readme_watcher;
            // Whether a persistent recursive-watch upgrade failure was logged
            // already, so the warning is emitted at most once (not every tick).
            let mut upgrade_warned = false;

            loop {
                match shutdown_rx.recv_timeout(Duration::from_millis(50)) {
                    // Shutdown signaled or handle dropped
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }

                // If source_dir did not exist at startup, keep polling until it
                // appears, then upgrade to a recursive watch. The poll runs every
                // tick regardless of file-system events, so creation is always
                // detected.
                if !recursive_active {
                    recursive_active = try_upgrade_recursive_watch(
                        &mut watcher,
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
                    let storage_event = to_storage_event(&event, &resolver_for_drain, &meta_cache);

                    if event_tx.send(storage_event).is_err() {
                        // Receiver dropped, exit thread
                        return;
                    }
                }
            }
        });

        Ok((event_rx, WatchHandle::new(shutdown_tx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_git_fixture::{GitFixture, assert_commit_time, commit_time};
    use rw_meta::{DiagnosticSource, Severity};
    use rw_storage::StorageErrorKind;
    use std::assert_matches;
    use std::time::UNIX_EPOCH;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_fs_storage_is_send_sync() {
        assert_send_sync::<FsStorage>();
    }

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn readme_homepage_is_found_at_the_project_root_with_a_nested_source_dir() {
        let temp = tempfile::tempdir().expect("create tempdir");
        let project_dir = temp.path().to_path_buf();
        let source_dir = project_dir.join("docs").join("site");
        std::fs::create_dir_all(&source_dir).expect("create nested source dir");
        std::fs::write(project_dir.join("README.md"), "# From the project root\n")
            .expect("write README");
        // Decoy: a README resolved from `source_dir.parent()` would land here.
        std::fs::write(
            project_dir.join("docs").join("README.md"),
            "# Wrong README\n",
        )
        .expect("write decoy README");

        let storage = FsStorage::new(project_dir, source_dir);
        let content = storage.read("").expect("read homepage");

        assert!(
            content.contains("From the project root"),
            "expected the project-root README, got: {content}"
        );
    }

    #[test]
    fn attrs_follow_selected_sidecar_and_frontmatter_without_inheritance() {
        // Exercise every selected form, both with content and metadata-only.
        // Lower-ranked files deliberately have unique attrs that must not leak.
        for selected in ["page/meta.yaml", "page/index.meta.yaml", "page.meta.yaml"] {
            for has_content in [false, true] {
                let temp = create_test_dir();
                fs::create_dir(temp.path().join("page")).unwrap();
                let forms = ["page/meta.yaml", "page/index.meta.yaml", "page.meta.yaml"];
                let rank = forms.iter().position(|form| *form == selected).unwrap();
                for form in &forms[rank + 1..] {
                    fs::write(temp.path().join(form), "attrs: {unselected: true}\n").unwrap();
                }
                fs::write(
                    temp.path().join(selected),
                    "title: Selected\nnamespace: parent\nattrs: {owner: sidecar, keep: true, nested: {old: true}, list: [old]}\n",
                ).unwrap();
                if has_content {
                    fs::write(temp.path().join("page/index.md"),
                        "---\nattrs: {owner: null, nested: {new: true}, list: [new]}\n---\n# Body\n").unwrap();
                }
                fs::write(temp.path().join("page/child.md"), "# Child\n").unwrap();
                let storage = FsStorage::new(temp.path().to_path_buf(), temp.path().to_path_buf());
                let first = storage.scan().unwrap();
                let second = storage.scan().unwrap();
                let page = first.iter().find(|d| d.path == "page").unwrap();
                let cached = second.iter().find(|d| d.path == "page").unwrap();
                assert!(Arc::ptr_eq(&page.meta, &cached.meta));
                assert_eq!(page.has_content, has_content);
                assert_eq!(page.meta.title, "Selected");
                let expected = if has_content {
                    serde_json::json!({"owner": null, "keep": true, "nested": {"new": true}, "list": ["new"]})
                } else {
                    serde_json::json!({"owner": "sidecar", "keep": true, "nested": {"old": true}, "list": ["old"]})
                };
                assert_eq!(
                    serde_json::to_value(&page.meta.attrs).unwrap(),
                    expected,
                    "{selected}"
                );
                let child = first.iter().find(|d| d.path == "page/child").unwrap();
                assert!(child.meta.attrs.is_empty(), "{selected} must not inherit");
                assert_eq!(page.meta.namespace.as_deref(), Some("parent"));
            }
        }
    }

    #[test]
    fn test_sidecar_combines_metadata_and_content() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original H1\n\nBody.").unwrap();
        fs::write(
            temp_dir.path().join("guide.meta.yaml"),
            "title: Sidecar Title\nkind: guide\nattrs: {owner: named-leaf}",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        let doc = docs.iter().find(|d| d.path == "guide").unwrap();
        assert!(doc.has_content);
        assert_eq!(doc.meta.title, "Sidecar Title"); // sidecar wins over H1
        assert_eq!(doc.meta.kind, Some("guide".to_owned()));
        assert_eq!(doc.meta.attrs["owner"], "named-leaf");

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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "foo").unwrap();
        assert_eq!(doc.meta.title, "Directory Foo");
    }

    #[test]
    fn test_index_variant_alone_titles_directory() {
        let temp_dir = create_test_dir();
        let dir = temp_dir.path().join("my-domain");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("index.meta.yaml"), "kind: domain").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        let doc = docs.iter().find(|d| d.path == "my-domain").unwrap();
        assert!(!doc.has_content);
        assert_eq!(doc.meta.title, "My Domain"); // titlecased directory name
        assert_eq!(doc.meta.kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_meta_sibling_leaf_own_metadata() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("payments.meta.yaml"),
            "title: Payments Service\nkind: component",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "payments").unwrap();

        assert_eq!(doc.meta.title, "Payments Service");
        assert_eq!(doc.meta.kind.as_deref(), Some("component"));
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let child = docs.iter().find(|d| d.path == "foo/bar").unwrap();

        assert_ne!(child.meta.title, "Sibling Title");
    }

    #[test]
    fn test_meta_index_variant_does_not_cascade_to_descendants() {
        let temp_dir = create_test_dir();
        let dir = temp_dir.path().join("dir");
        fs::create_dir(&dir).unwrap();
        // Directory metadata via the index.meta.yaml variant, for "dir" itself.
        fs::write(dir.join("index.meta.yaml"), "title: Dir Title").unwrap();
        fs::write(dir.join("child.md"), "# Child").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let dir_doc = docs.iter().find(|d| d.path == "dir").unwrap();
        let child = docs.iter().find(|d| d.path == "dir/child").unwrap();

        assert_eq!(dir_doc.meta.title, "Dir Title");
        assert_ne!(child.meta.title, "Dir Title");
    }

    #[test]
    fn test_scan_content_less_sibling_virtual_page() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("payments.meta.yaml"),
            "kind: component\nnamespace: billing\nattrs: {owner: metadata-only-leaf}",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        let doc = docs.iter().find(|d| d.path == "payments").unwrap();
        assert!(!doc.has_content);
        assert_eq!(doc.meta.title, "Payments"); // titlecased from url segment
        assert_eq!(doc.meta.kind, Some("component".to_owned()));
        assert_eq!(doc.meta.namespace, Some("billing".to_owned()));
        assert_eq!(doc.meta.attrs["owner"], "metadata-only-leaf");
    }

    #[test]
    fn test_scan_empty_dir() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs, [] as [rw_storage::Document; 0]);
    }

    #[test]
    fn test_scan_missing_dir() {
        let storage = FsStorage::new(PathBuf::from("/nonexistent"), PathBuf::from("/nonexistent"));
        let docs = storage.scan().unwrap();

        assert_eq!(docs, [] as [rw_storage::Document; 0]);
    }

    #[test]
    fn test_scan_flat_structure() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# User Guide\n\nContent.").unwrap();
        fs::write(temp_dir.path().join("api.md"), "# API Reference\n\nDocs.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].meta.title, "My Custom Title");
    }

    #[test]
    fn test_scan_falls_back_to_filename() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("setup-guide.md"),
            "Content without heading.",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].meta.title, "Setup Guide");
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden").unwrap();
        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.path, "domain");
        assert!(doc.has_content);
        assert_eq!(doc.meta.kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_scan_with_custom_meta_filename() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("config.yml"), "kind: section").unwrap();
        fs::write(domain_dir.join("meta.yaml"), "ignored").unwrap(); // Should be ignored

        let storage = FsStorage::with_meta_filename(
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            "config.yml",
        );
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert!(doc.has_content);
        assert_eq!(doc.meta.kind, Some("section".to_owned()));
    }

    #[test]
    fn test_scan_no_page_kind_without_kind_field() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "title: Home Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.path, "");
        assert!(doc.has_content);
        assert!(doc.meta.kind.is_none()); // No kind field in metadata
    }

    #[test]
    fn test_scan_creates_virtual_page() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        // No index.md, only meta.yaml
        fs::write(domain_dir.join("meta.yaml"), "title: Domain Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.path, "domain");
        assert_eq!(doc.meta.title, "Domain Title");
        assert!(!doc.has_content); // Virtual page
        assert!(doc.meta.kind.is_none());
    }

    #[test]
    fn test_scan_virtual_page_with_kind() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("my-nice-domain");
        fs::create_dir(&domain_dir).unwrap();
        // No title but has kind in meta.yaml
        fs::write(domain_dir.join("meta.yaml"), "kind: domain").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.meta.title, "My Nice Domain"); // Fallback to directory name
        assert_eq!(doc.meta.kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_scan_no_virtual_page_without_metadata() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("empty-domain");
        fs::create_dir(&domain_dir).unwrap();
        // No meta.yaml, no index.md

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs, [] as [rw_storage::Document; 0]);
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "domain").unwrap();

        assert_eq!(doc.meta.title, "Domain");
        assert_eq!(doc.meta.description.as_deref(), Some("Domain docs"));
        assert_eq!(doc.meta.kind.as_deref(), Some("domain"));
        assert_eq!(
            doc.meta.pages.as_deref(),
            Some(["overview".to_owned(), "api".to_owned()].as_slice())
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let child = docs.iter().find(|d| d.path == "parent/child").unwrap();

        assert_eq!(child.meta.title, "Child");
        assert!(child.meta.description.is_none());
        assert!(child.meta.kind.is_none());
        assert!(child.meta.pages.is_none());
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "guide").unwrap();

        assert_eq!(doc.meta.title, "Guide");
    }

    #[test]
    fn test_read_existing_file() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide\n\nContent here.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let result = storage.read("nonexistent");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::NotFound);
        assert_eq!(err.backend, Some("Fs"));
    }

    #[test]
    fn test_mtime_cache_reuses_titles() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

        // First scan
        let docs1 = storage.scan().unwrap();
        assert_eq!(docs1[0].meta.title, "Original Title");

        // Second scan without changes - should use cache
        let docs2 = storage.scan().unwrap();
        assert_eq!(docs2[0].meta.title, "Original Title");
    }

    #[test]
    fn second_scan_returns_cached_diagnostics() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide H1\n").unwrap();
        fs::write(
            temp_dir.path().join("guide.meta.yaml"),
            "title: [a, b]\nkind: guide",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

        // First scan
        let docs1 = storage.scan().unwrap();
        let first = docs1.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(first.diagnostics.len(), 1);

        // Second scan without changes hits the meta cache: the Document
        // carries the same diagnostics, returned silently rather than re-logged.
        let docs2 = storage.scan().unwrap();
        let second = docs2.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(second.diagnostics, first.diagnostics);
    }

    #[test]
    fn test_mtime_cache_detects_markdown_changes() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

        // First scan
        let docs1 = storage.scan().unwrap();
        assert_eq!(docs1[0].meta.title, "Original Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify file
        fs::write(temp_dir.path().join("guide.md"), "# Updated Title").unwrap();

        // Second scan should see new title
        let docs2 = storage.scan().unwrap();
        assert_eq!(docs2[0].meta.title, "Updated Title");
    }

    #[test]
    fn test_mtime_cache_detects_meta_yaml_changes() {
        let temp_dir = create_test_dir();
        let guide_dir = temp_dir.path().join("guide");
        fs::create_dir(&guide_dir).unwrap();
        fs::write(guide_dir.join("index.md"), "# H1 Title").unwrap();
        fs::write(guide_dir.join("meta.yaml"), "title: YAML Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

        // First scan — meta.yaml title wins over H1
        let docs1 = storage.scan().unwrap();
        let guide1 = docs1.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(guide1.meta.title, "YAML Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify only meta.yaml, not the markdown file
        fs::write(guide_dir.join("meta.yaml"), "title: New YAML Title").unwrap();

        // Second scan should see new title from meta.yaml
        let docs2 = storage.scan().unwrap();
        let guide2 = docs2.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(guide2.meta.title, "New YAML Title");
    }

    // Note: file_path_to_url tests are in source.rs

    #[test]
    fn test_resolve_content_root() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let resolved = storage.resolver.resolve_content("");

        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("index.md"));
    }

    #[test]
    fn test_resolve_content_falls_back_to_standalone() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let resolved = storage.resolver.resolve_content("guide");

        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("guide.md"));
    }

    #[test]
    fn test_resolve_content_not_found() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let resolved = storage.resolver.resolve_content("nonexistent");

        assert!(resolved.is_none());
    }

    #[test]
    fn git_mode_returns_commit_time_filesystem_mode_returns_fs_time() {
        // Keep the file at the repo root (like the rw-vcs Vcs tests) and use
        // `fixture.path()` directly as source_dir, so gix's workdir and the
        // resolved file path share the same form on every platform. A docs/
        // subdir plus fs::canonicalize tripped repo_relative_path's
        // strip_prefix — the macOS /var -> /private/var symlink one way, the
        // Windows \\?\ verbatim prefix the other — making git mode fall back to
        // fs and defeating the test.
        let fixture = GitFixture::init();
        fixture.write("guide.md", "# Guide");
        let committed_at = fixture.commit_all("old");
        let source_dir = fixture.path().to_path_buf();

        // Git mode: the commit time exactly, not merely "long ago".
        let git = FsStorage::new(source_dir.clone(), source_dir.clone())
            .with_mtime_source(MtimeSource::Git);
        assert_commit_time(git.mtime("guide").unwrap(), committed_at);

        // Filesystem mode (the default): the file's on-disk mtime exactly, not
        // merely "after 2020" — every value produced today satisfies that,
        // including a wrong one. Read straight from the filesystem so the
        // expectation does not come from the code under test.
        let on_disk = fs::metadata(fixture.path().join("guide.md"))
            .and_then(|m| m.modified())
            .expect("the file exists")
            .duration_since(UNIX_EPOCH)
            .expect("a file written today is after the epoch")
            .as_secs_f64();
        let fs = FsStorage::new(source_dir.clone(), source_dir);
        let fs_mtime = fs.mtime("guide").unwrap();
        #[allow(clippy::float_cmp)]
        {
            assert_eq!(fs_mtime, on_disk);
        }
        assert!(
            fs_mtime > commit_time(committed_at),
            "fs mtime {fs_mtime} should be ~now, not the commit time"
        );
    }

    #[test]
    fn git_mode_resolves_commit_time_for_readme_only_project() {
        // README-only site: no docs/ dir, so the default source_dir points at a
        // non-existent <repo>/docs. Git discovery runs from the project dir (the
        // repo root) and must report the README's commit time — not the fs
        // checkout time. Use `fixture.path()` as-is (do NOT canonicalize): gix's
        // workdir and the resolved README path must share the same form so
        // repo_relative_path strips cleanly on every platform. Canonicalizing
        // introduces the Windows `\\?\` verbatim prefix (and the macOS
        // /var -> /private/var swap) that gix's workdir lacks, defeating the
        // strip. Same rationale as git_mode_returns_commit_time_*.
        let fixture = GitFixture::init();
        fixture.write("README.md", "# Hi");
        let committed_at = fixture.commit_all("old");
        let missing_source_dir = fixture.path().join("docs");
        assert!(!missing_source_dir.exists());

        let storage = FsStorage::new(fixture.path().to_path_buf(), missing_source_dir)
            .with_mtime_source(MtimeSource::Git);

        // The homepage ("") resolves to the root README; its mtime must be that
        // README's commit time, proving git discovery succeeded. Anything near
        // now means discovery failed and it used the fs mtime.
        assert_commit_time(storage.mtime("").unwrap(), committed_at);
    }

    #[test]
    fn git_mode_falls_back_to_fs_time_with_no_repo() {
        // Git requested but the tree is not a git repo at all. The observable
        // contract asserted here is the filesystem-time fallback; the accompanying
        // warning in with_mtime_source is a side effect this test does not capture.
        let dir = create_test_dir();
        fs::write(dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(dir.path().to_path_buf(), dir.path().to_path_buf())
            .with_mtime_source(MtimeSource::Git);
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let result = storage.read("../etc/passwd");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::InvalidPath);
        assert_eq!(err.backend, Some("Fs"));
    }

    #[test]
    fn test_read_rejects_nested_path_traversal() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let result = storage.read("subdir/../../etc/passwd");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::InvalidPath);
    }

    #[test]
    fn test_mtime_rejects_path_traversal() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let result = storage.mtime("../etc/passwd");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::InvalidPath);
        assert_eq!(err.backend, Some("Fs"));
    }

    #[test]
    fn test_watch_returns_receiver_and_handle() {
        let temp_dir = create_test_dir();
        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

        let result = storage.watch();
        assert!(result.is_ok());
    }

    #[test]
    fn test_watch_succeeds_when_source_dir_missing() {
        // A README-only project: source_dir (docs/) does not exist.
        let temp_dir = create_test_dir();
        let missing = temp_dir.path().join("docs");
        assert!(!missing.exists());

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), missing);
        // watch() must not fail just because docs/ is absent.
        assert!(storage.watch().is_ok());
    }

    #[test]
    fn test_watch_succeeds_with_relative_missing_source_dir() {
        // A relative, non-existent source_dir must not error. The project_dir
        // is a tempdir so the README candidate can't resolve to a real file in
        // whatever cwd the test happens to run from.
        let temp = create_test_dir();
        let storage = FsStorage::new(
            temp.path().to_path_buf(),
            PathBuf::from("nonexistent-docs-rw-test"),
        );
        assert!(storage.watch().is_ok());
    }

    #[test]
    fn test_scan_injects_readme_when_docs_missing() {
        // source_dir (docs/) absent, README.md present in its parent.
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("README.md"), "# Atlas").unwrap();
        let missing = temp_dir.path().join("docs");

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), missing);
        let docs = storage.scan().unwrap();

        let home = docs.iter().find(|d| d.path.is_empty());
        assert!(home.is_some(), "README.md should be injected as homepage");
        assert_eq!(home.unwrap().meta.title, "Atlas");
    }

    #[test]
    fn test_try_upgrade_recursive_watch_when_dir_absent() {
        // Helper returns false and records nothing when the dir does not exist.
        let temp_dir = create_test_dir();
        let missing = temp_dir.path().join("docs");

        let mut watcher = notify::recommended_watcher(|_res: notify::Result<Event>| {}).unwrap();
        // Zero debounce window so any recorded event would be immediately drainable.
        let debouncer = EventDebouncer::new(Duration::from_millis(0));

        assert!(!try_upgrade_recursive_watch(
            &mut watcher,
            &debouncer,
            &missing
        ));
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

        let mut watcher = notify::recommended_watcher(|_res: notify::Result<Event>| {}).unwrap();
        let debouncer = EventDebouncer::new(Duration::from_millis(0));

        assert!(!try_upgrade_recursive_watch(
            &mut watcher,
            &debouncer,
            &file_path
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

        let mut watcher = notify::recommended_watcher(|_res: notify::Result<Event>| {}).unwrap();
        // Zero debounce window so the synthetic event is immediately drainable.
        let debouncer = EventDebouncer::new(Duration::from_millis(0));

        assert!(try_upgrade_recursive_watch(&mut watcher, &debouncer, &docs));

        let events = debouncer.drain_ready();
        assert_eq!(
            events.len(),
            1,
            "expected one synthetic event, got: {events:?}"
        );
        assert_eq!(events[0].path, docs);
        assert_eq!(events[0].kind, RawEventKind::Created);
    }

    #[cfg(unix)]
    #[test]
    fn watch_root_reroots_canonical_paths_under_a_symlinked_source_dir() {
        let temp_dir = create_test_dir();
        let real = fs::canonicalize(temp_dir.path()).unwrap().join("real");
        fs::create_dir(&real).unwrap();
        let link = temp_dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let mut root = WatchRoot::new(link.clone());

        assert_eq!(
            root.reroot(real.join("sub/page.md")),
            Some(link.join("sub/page.md"))
        );
        assert_eq!(
            root.reroot(link.join("page.md")),
            Some(link.join("page.md"))
        );
        assert_eq!(root.reroot(temp_dir.path().join("other/page.md")), None);
        assert_eq!(root.reroot(real), Some(link));
    }

    #[test]
    fn watch_root_reroots_absolute_paths_under_a_relative_source_dir() {
        let dir = PathBuf::from("rw-watch-root-missing-docs");
        let mut root = WatchRoot::new(dir.clone());
        let absolute = std::env::current_dir().unwrap().join(&dir);

        assert_eq!(
            root.reroot(absolute.join("page.md")),
            Some(dir.join("page.md"))
        );
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

        let storage = FsStorage::new(temp_path.clone(), temp_path.clone());
        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(200));

        // Create a file
        fs::write(temp_path.join("new.md"), "# New").unwrap();

        // Wait for debounce + processing (be generous with timing)
        std::thread::sleep(Duration::from_millis(500));

        // Try to receive events
        let events: Vec<_> = rx.try_iter().collect();

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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), docs.clone());
        let (rx, _handle) = storage.watch().unwrap();

        // Create docs/, then a page inside it once the 50ms poll has upgraded
        // the watch: FSEvents does not report a write that precedes it.
        std::thread::sleep(Duration::from_millis(100));
        fs::create_dir(&docs).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        fs::write(docs.join("guide.md"), "# Guide").unwrap();

        // Wait for the debounce window to drain.
        std::thread::sleep(Duration::from_millis(500));

        // Assert the page *inside* the newly created docs/ is observed — not just
        // the synthetic root Created event. This proves the recursive watch was
        // actually started on the new directory, which is the point of the fix.
        let events: Vec<_> = rx.try_iter().collect();
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(100));

        // Modify the file
        fs::write(temp_dir.path().join("existing.md"), "# Modified").unwrap();

        // Wait for debounce + processing
        std::thread::sleep(Duration::from_millis(250));

        // Should receive modified event (URL path)
        let event = rx.try_recv();
        assert!(event.is_ok(), "Expected to receive event");
        let event = event.unwrap();
        assert_eq!(event.path, "existing");
        assert_matches!(event.kind, StorageEventKind::Modified { .. });
    }

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
    fn test_watch_detects_file_deletion() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("to-delete.md"), "# Delete Me").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(100));

        // Delete the file
        fs::remove_file(temp_dir.path().join("to-delete.md")).unwrap();

        // Wait for debounce + processing
        std::thread::sleep(Duration::from_millis(250));

        // Should receive removed event (URL path)
        let event = rx.try_recv();
        assert!(event.is_ok(), "Expected to receive event");
        let event = event.unwrap();
        assert_eq!(event.path, "to-delete");
        assert_eq!(event.kind, StorageEventKind::Removed);
    }

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
    fn test_watch_respects_patterns() {
        let temp_dir = create_test_dir();
        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

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
        assert!(event.is_ok(), "Expected to receive event for .md file");
        let event = event.unwrap();
        assert_eq!(event.path, "doc");

        // No more events
        let event = rx.try_recv();
        assert!(event.is_err());
    }

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
    fn test_watch_debounces_multiple_events() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("file.md"), "# Original").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
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
        assert!(event.is_ok(), "Expected to receive event");
        assert_matches!(event.unwrap().kind, StorageEventKind::Modified { .. });

        // No more events
        let event = rx.try_recv();
        assert!(event.is_err());
    }

    #[test]
    #[ignore = "timing-sensitive, can be flaky in test environments"]
    fn test_watch_handle_stops_watching() {
        let temp_dir = create_test_dir();
        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

        let (rx, handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(100));

        // Stop watching
        drop(handle);

        // Give the thread time to process stop signal
        std::thread::sleep(Duration::from_millis(100));

        // Create a file
        fs::write(temp_dir.path().join("new.md"), "# New").unwrap();

        // Wait
        std::thread::sleep(Duration::from_millis(250));

        // Should not receive any events (watcher is stopped)
        let event = rx.try_recv();
        assert!(event.is_err());
    }

    /// Create a test directory with `docs/` subdirectory and README.md,
    /// returning `(temp_dir, project_root, FsStorage)`.
    ///
    /// `FsStorage` resolves the README homepage fallback from `project_dir`.
    fn create_readme_test_dir(readme_content: &str) -> (tempfile::TempDir, PathBuf, FsStorage) {
        let temp_dir = create_test_dir();
        let project_root = temp_dir.path().to_path_buf();
        let source_dir = project_root.join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(project_root.join("README.md"), readme_content).unwrap();

        let storage = FsStorage::new(project_root.clone(), source_dir);
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
        assert_eq!(home.meta.title, "My Project");
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
    fn content_backed_meta_is_shared_across_unchanged_scans() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("guide.md"),
            "---\ndescription: Getting started\nkind: guide\nnamespace: payments\npages:\n  - intro\n---\n# Guide\n",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let first = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path == "guide")
            .unwrap();
        let second = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path == "guide")
            .unwrap();

        assert!(Arc::ptr_eq(&first.meta, &second.meta));
    }

    #[test]
    fn virtual_page_meta_is_shared_across_unchanged_scans() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("payments.meta.yaml"),
            "title: Payments\ndescription: Billing docs\nkind: domain\nnamespace: billing\npages:\n  - intro",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let first = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path == "payments")
            .unwrap();
        let second = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path == "payments")
            .unwrap();

        assert!(Arc::ptr_eq(&first.meta, &second.meta));
    }

    #[test]
    fn readme_fallback_frontmatter_is_preserved_and_shared_across_unchanged_scans() {
        let (_dir, _, storage) = create_readme_test_dir(
            "---\ntitle: Readme Home\ndescription: Project docs\nkind: domain\nnamespace: docs\nattrs: {owner: project, nested: [null, true]}\npages:\n  - guide\n---\n# Body Title",
        );
        let first = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path.is_empty())
            .unwrap();
        let second = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path.is_empty())
            .unwrap();

        assert!(Arc::ptr_eq(&first.meta, &second.meta));
        assert_eq!(first.meta.title, "Readme Home");
        assert_eq!(
            serde_json::to_value(&first.meta.attrs).unwrap(),
            serde_json::json!({"owner": "project", "nested": [null, true]})
        );
        assert_eq!(first.meta.description.as_deref(), Some("Project docs"));
        assert_eq!(first.meta.kind.as_deref(), Some("domain"));
        assert_eq!(first.meta.namespace.as_deref(), Some("docs"));
        assert_eq!(
            first.meta.pages.as_deref(),
            Some(["guide".to_owned()].as_slice())
        );
        assert_eq!(first.origin.as_deref(), Some("docs"));
        assert!(first.is_dir);
    }

    #[test]
    fn readme_homepage_with_malformed_frontmatter_keeps_page_with_error_diagnostic() {
        let (_dir, _, storage) = create_readme_test_dir("---\ntitle: [unclosed\n---\n# Hello\n");
        let docs = storage.scan().unwrap();

        let home = docs
            .iter()
            .find(|d| d.path.is_empty())
            .expect("malformed frontmatter must not drop the homepage");
        assert_eq!(home.meta.title, "Hello");
        assert_eq!(home.diagnostics.len(), 1);
        assert_eq!(home.diagnostics[0].source, DiagnosticSource::Frontmatter);
        assert_eq!(home.diagnostics[0].severity, Severity::Error);
        assert_eq!(home.diagnostics[0].field, None);
    }

    #[test]
    fn test_scan_does_not_inject_readme_when_index_exists() {
        let (dir, _, storage) = create_readme_test_dir("# README");
        fs::write(dir.path().join("docs/index.md"), "# Docs Home").unwrap();
        let docs = storage.scan().unwrap();

        let homes: Vec<_> = docs.iter().filter(|d| d.path.is_empty()).collect();
        assert_eq!(homes.len(), 1);
        assert_eq!(homes[0].meta.title, "Docs Home");
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

        let events: Vec<_> = rx.try_iter().collect();
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

        let storage = FsStorage::new(base.clone(), base.clone());
        let (rx, _handle) = storage.watch().unwrap();

        // Wait for watcher to be ready
        std::thread::sleep(Duration::from_millis(200));

        // Rename directory
        let new_dir = base.join("new-name");
        fs::rename(&old_dir, &new_dir).unwrap();

        // Wait for debounce + processing (generous for rename detection)
        std::thread::sleep(Duration::from_millis(500));

        let events: Vec<_> = rx.try_iter().collect();
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

        let storage = FsStorage::new(dir.path().to_path_buf(), docs_dir);
        let docs = storage.scan().unwrap();

        let guides = docs.iter().find(|d| d.path == "guides").unwrap();
        assert_eq!(
            guides.meta.pages,
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

        let storage = FsStorage::new(dir.path().to_path_buf(), docs_dir);
        let docs = storage.scan().unwrap();

        let guides = docs.iter().find(|d| d.path == "guides").unwrap();
        assert_eq!(guides.meta.pages, Some(vec!["alpha".to_owned()]));
    }

    #[test]
    fn scan_no_pages_returns_none() {
        let dir = create_test_dir();
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(dir.path().to_path_buf(), docs_dir);
        let docs = storage.scan().unwrap();

        let guide = docs.iter().find(|d| d.path == "guide").unwrap();
        assert!(guide.meta.pages.is_none());
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "billing").unwrap();
        assert_eq!(doc.meta.namespace.as_deref(), Some("payments"));
    }

    #[test]
    fn invalid_namespace_warns_and_page_survives() {
        let temp_dir = create_test_dir();
        let billing = temp_dir.path().join("billing");
        fs::create_dir_all(&billing).unwrap();
        fs::write(
            billing.join("meta.yaml"),
            "namespace: bad/value\ntitle: Billing\n",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let billing = docs.iter().find(|d| d.path == "billing").unwrap();
        assert_eq!(billing.meta.title, "Billing");
        assert_eq!(billing.meta.namespace, None);
        assert_eq!(billing.diagnostics.len(), 1);
        assert_eq!(billing.diagnostics[0].source, DiagnosticSource::Sidecar);
        assert_eq!(billing.diagnostics[0].severity, Severity::Warning);
        assert_eq!(billing.diagnostics[0].field.as_deref(), Some("namespace"));
        assert!(
            billing.diagnostics[0].message.contains("bad/value"),
            "diagnostic should name the value: {}",
            billing.diagnostics[0].message
        );
    }

    #[test]
    fn wrong_typed_sidecar_title_falls_back_to_h1_with_warning() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide H1\n").unwrap();
        fs::write(
            temp_dir.path().join("guide.meta.yaml"),
            "title: [a, b]\nkind: guide",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "guide").unwrap();

        assert_eq!(doc.meta.title, "Guide H1");
        assert_eq!(doc.meta.kind.as_deref(), Some("guide"));
        assert_eq!(doc.diagnostics.len(), 1);
        assert_eq!(doc.diagnostics[0].source, DiagnosticSource::Sidecar);
        assert_eq!(doc.diagnostics[0].severity, Severity::Warning);
        assert_eq!(doc.diagnostics[0].field.as_deref(), Some("title"));
    }

    #[test]
    fn scalar_pages_on_metadata_only_page_keeps_page_with_warning() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("payments.meta.yaml"),
            "title: Payments\npages: everything",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "payments").unwrap();

        assert!(!doc.has_content);
        assert_eq!(doc.meta.title, "Payments");
        assert_eq!(doc.meta.pages, None);
        assert_eq!(doc.diagnostics.len(), 1);
        assert_eq!(doc.diagnostics[0].source, DiagnosticSource::Sidecar);
        assert_eq!(doc.diagnostics[0].severity, Severity::Warning);
        assert_eq!(doc.diagnostics[0].field.as_deref(), Some("pages"));
    }

    #[test]
    fn diagnostics_list_sidecar_before_frontmatter() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("guide.md"),
            "---\ntitle: [a, b]\n---\n# H\n",
        )
        .unwrap();
        fs::write(temp_dir.path().join("guide.meta.yaml"), "kind: [x]").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "guide").unwrap();

        let sources: Vec<_> = doc.diagnostics.iter().map(|d| d.source).collect();
        assert_eq!(
            sources,
            vec![DiagnosticSource::Sidecar, DiagnosticSource::Frontmatter]
        );
        assert_eq!(doc.diagnostics[0].field.as_deref(), Some("kind"));
        assert_eq!(doc.diagnostics[1].field.as_deref(), Some("title"));
    }

    #[test]
    fn clean_pages_have_no_diagnostics() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide H1\n").unwrap();
        fs::write(
            temp_dir.path().join("guide.meta.yaml"),
            "title: Guide\nkind: guide",
        )
        .unwrap();
        let domain = temp_dir.path().join("billing");
        fs::create_dir(&domain).unwrap();
        fs::write(domain.join("index.md"), "# Billing\n").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        for path in ["guide", "billing"] {
            let doc = docs.iter().find(|d| d.path == path).unwrap();
            assert!(doc.diagnostics.is_empty(), "{path} should be clean");
        }
    }

    #[test]
    fn test_to_storage_event_named_sibling_routes_to_sibling_path() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let source_dir = Path::new("/docs");
        let event = DebouncedEvent {
            path: PathBuf::from("/docs/systems/payments.meta.yaml"),
            kind: RawEventKind::Removed,
        };
        let resolver = PathResolver::new(source_dir, source_dir.to_path_buf(), "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver, &MetaCache::default());
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
        let resolver = PathResolver::new(source_dir, source_dir.to_path_buf(), "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver, &MetaCache::default());
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
        let resolver = PathResolver::new(source_dir, source_dir.to_path_buf(), "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver, &MetaCache::default());
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
        let resolver =
            PathResolver::new(temp_dir.path(), temp_dir.path().to_path_buf(), "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver, &MetaCache::default());
        assert_eq!(storage_event.path, "payments");
        match storage_event.kind {
            StorageEventKind::Modified { meta } => {
                assert_eq!(meta.title, "Payments Service");
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    #[test]
    fn modified_event_for_a_missing_file_is_reported_removed() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let temp_dir = create_test_dir();
        let event = DebouncedEvent {
            path: temp_dir.path().join("gone.md"),
            kind: RawEventKind::Modified,
        };
        let resolver =
            PathResolver::new(temp_dir.path(), temp_dir.path().to_path_buf(), "meta.yaml");

        let storage_event = to_storage_event(&event, &resolver, &MetaCache::default());
        assert_eq!(storage_event.path, "gone");
        assert_eq!(storage_event.kind, StorageEventKind::Removed);
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
        let resolver = PathResolver::new(project_root.path(), source_dir, "meta.yaml");
        let storage_event = to_storage_event(&event, &resolver, &MetaCache::default());
        assert_eq!(storage_event.path, "");
        match storage_event.kind {
            StorageEventKind::Modified { meta } => {
                assert_eq!(meta.title, "Readme Home Title");
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    #[test]
    fn readme_homepage_without_h1_titles_the_same_on_scan_and_event() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(root.join("README.md"), "Body with no heading.").unwrap();

        let storage = FsStorage::new(root.to_path_buf(), docs.clone());
        let scan_title = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path.is_empty())
            .expect("README homepage document")
            .meta
            .title
            .clone();

        let resolver = PathResolver::new(root, docs, "meta.yaml");
        let event = DebouncedEvent {
            path: root.join("README.md"),
            kind: RawEventKind::Modified,
        };
        let StorageEventKind::Modified { meta } =
            to_storage_event(&event, &resolver, &MetaCache::default()).kind
        else {
            panic!("expected Modified");
        };

        assert_eq!(
            scan_title, "Home",
            "H1-less README titles from HOMEPAGE_FALLBACK_NAME"
        );
        assert_eq!(
            scan_title, meta.title,
            "scan and watch paths must title an H1-less README identically"
        );
    }

    #[test]
    fn modified_event_meta_equals_scanned_meta() {
        // The live-reload manager reloads structure whenever these differ, so
        // any drift turns every body edit into a nav reload.
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let dir = create_test_dir();
        let root = dir.path();
        fs::create_dir_all(root.join("guide")).unwrap();
        fs::write(
            root.join("guide/a.md"),
            "---\nkind: system\nname: billing\ndescription: Bills\nattrs:\n  owner: team\n---\n# A\n",
        )
        .unwrap();
        fs::write(
            root.join("guide/a.meta.yaml"),
            "namespace: payments\npages: [x, y]\nattrs:\n  tier: 1\n",
        )
        .unwrap();

        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        let scanned = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path == "guide/a")
            .expect("guide/a document")
            .meta;

        let resolver = PathResolver::new(root, root.to_path_buf(), "meta.yaml");
        let event = DebouncedEvent {
            path: root.join("guide/a.md"),
            kind: RawEventKind::Modified,
        };
        let StorageEventKind::Modified { meta } =
            to_storage_event(&event, &resolver, &MetaCache::default()).kind
        else {
            panic!("expected Modified");
        };

        assert_eq!(meta.kind.as_deref(), Some("system"));
        assert_eq!(meta.namespace.as_deref(), Some("payments"));
        assert_eq!(*scanned, meta);
    }

    #[test]
    fn modified_event_for_a_source_that_forms_no_page_is_reported_created() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let dir = create_test_dir();
        let root = dir.path();
        fs::write(root.join("emptied.meta.yaml"), "  \n").unwrap();

        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        assert_eq!(storage.scan().unwrap(), Vec::<Document>::new());

        let resolver = PathResolver::new(root, root.to_path_buf(), "meta.yaml");
        let event = DebouncedEvent {
            path: root.join("emptied.meta.yaml"),
            kind: RawEventKind::Modified,
        };
        let storage_event = to_storage_event(&event, &resolver, &MetaCache::default());

        assert_eq!(storage_event.path, "emptied");
        assert_eq!(storage_event.kind, StorageEventKind::Created);
    }

    #[test]
    fn untitled_sidecar_only_root_is_home_on_scan_and_event() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let dir = create_test_dir();
        let root = dir.path();
        fs::write(root.join("meta.yaml"), "kind: system\n").unwrap();

        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        let scanned = storage
            .scan()
            .unwrap()
            .into_iter()
            .find(|d| d.path.is_empty())
            .expect("root document")
            .meta;

        let resolver = PathResolver::new(root, root.to_path_buf(), "meta.yaml");
        let event = DebouncedEvent {
            path: root.join("meta.yaml"),
            kind: RawEventKind::Modified,
        };
        let StorageEventKind::Modified { meta } =
            to_storage_event(&event, &resolver, &MetaCache::default()).kind
        else {
            panic!("expected Modified");
        };

        assert_eq!(scanned.title, "Home");
        assert_eq!(*scanned, meta);
    }

    #[test]
    fn test_is_hidden_rel_path() {
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

        let storage = FsStorage::new(root.to_path_buf(), docs);
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
        let storage = FsStorage::new(tmp.path().to_path_buf(), docs);
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
        assert_eq!(
            storage.url_paths_for_source(Path::new("README.md")),
            [] as [String; 0]
        );
    }

    #[test]
    fn url_paths_for_source_meta_yaml_is_empty() {
        let (_tmp, storage) = make_url_paths_storage();
        // metadata file → no pages
        assert_eq!(
            storage.url_paths_for_source(Path::new("meta.yaml")),
            [] as [String; 0]
        );
    }

    #[test]
    fn url_paths_for_source_nonexistent_is_empty() {
        let (_tmp, storage) = make_url_paths_storage();
        assert_eq!(
            storage.url_paths_for_source(Path::new("nope.md")),
            [] as [String; 0]
        );
    }

    #[test]
    fn url_paths_for_source_outside_source_dir_is_empty() {
        let (tmp, storage) = make_url_paths_storage();
        let p = tmp.path().join("elsewhere/x.md");
        assert_eq!(storage.url_paths_for_source(&p), [] as [String; 0]);
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

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        let guide = docs.iter().find(|d| d.path == "guide").unwrap();
        assert!(
            !guide.is_dir,
            "leaf guide.md URL is a file, not a directory"
        );

        let domain = docs.iter().find(|d| d.path == "domain").unwrap();
        assert!(domain.is_dir, "domain/index.md URL is a directory");
    }

    /// The scan path (`scanner` + `MetaRank` ordinal tie-break) and the watch
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

        let resolver = PathResolver::new(root, root.to_path_buf(), "meta.yaml");
        let refs = scanner::scan(&resolver);

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
