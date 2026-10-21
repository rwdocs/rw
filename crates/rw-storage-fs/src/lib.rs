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
//! let (documents, _) = storage.scan()?;
//! for doc in documents {
//!     println!("{}: {}", doc.path, doc.meta.title);
//! }
//! # Ok(())
//! # }
//! ```

mod debouncer;
mod dir_watch;
mod event;
mod scanner;
mod source;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{self, Component, Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use glob::Pattern;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use rw_diagrams::{diagram_language, is_plantuml_file};
use rw_meta::{Diagnostic, DiagnosticSource, Meta};
use rw_vcs::Vcs;

use debouncer::{DebouncedEvent, EventDebouncer, RawEventKind};
pub use dir_watch::watch_dirs;
pub use event::{StorageEvent, StorageEventKind, StorageEventReceiver, WatchHandle};
use rw_storage::{
    Document, MAX_FILE_SIZE, Storage, StorageError, StorageErrorKind, file_content_type,
    image_content_type,
};
use scanner::Listing;
use source::{
    Classification, PathResolver, classify_relpath, content_fallback_name, dir_url_path,
    watch_patterns,
};

/// Backend identifier for error messages.
const BACKEND: &str = "Fs";

/// How long a path must stay quiet before its change is reported.
const DEBOUNCE: Duration = Duration::from_millis(100);
/// How often a watch thread drains the debouncer.
const DRAIN_POLL: Duration = Duration::from_millis(50);

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
        let fallback = content_fallback_name(url_path, content_path);
        let fresh =
            Meta::resolve_with_diagnostics(markdown.as_deref(), meta_yaml.as_deref(), &fallback);
        let diagnostics: Arc<[Diagnostic]> = Arc::from(fresh.diagnostics);

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
pub struct FsStorage {
    /// Cloned into the watch drain thread, which resolves through the same rules.
    resolver: PathResolver,
    /// Project root — see `rw_config::Config::project_dir`. Stored for git
    /// repository discovery in [`FsStorage::with_mtime_source`] and as the root
    /// of [`Storage::read_file`]; the `README.md` homepage candidate is baked
    /// into `resolver` at construction and does not read this field.
    project_dir: PathBuf,
    /// Cloned into the watch drain thread along with `resolver`.
    meta_cache: MetaCache,
    /// Modification-time source; holds no repository in filesystem mode.
    vcs: Vcs,
}

/// Selects how [`FsStorage`] computes a page's modification time.
///
/// The default is [`Filesystem`](MtimeSource::Filesystem): a plain `stat`, with
/// no git involvement (not even repository discovery). Choose
/// [`Git`](MtimeSource::Git) — via [`FsStorage::with_mtime_source`] — for stable,
/// history-derived times (e.g. when publishing), at the cost of hashing the file
/// on every call and walking history once per file per HEAD commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MtimeSource {
    /// Filesystem `stat` mtime (fast, reflects on-disk edits). Default.
    #[default]
    Filesystem,
    /// Git committer time for clean tracked files, filesystem mtime
    /// otherwise (via [`rw_vcs::Vcs`]).
    Git,
}

impl FsStorage {
    /// Create a new filesystem storage.
    ///
    /// Watches `**/*.md`, `meta.yaml`, and `*.meta.yaml` (named sidecar / index
    /// form) files, images, and diagram files, for changes.
    ///
    /// The `README.md` homepage fallback and git repository discovery are
    /// resolved from `project_dir`. `source_dir` must lie inside it, as
    /// `rw_config` confines it.
    ///
    /// Modification times default to [`MtimeSource::Filesystem`]; call
    /// [`with_mtime_source`](Self::with_mtime_source) to opt into git times.
    #[must_use]
    pub fn new(project_dir: PathBuf, source_dir: PathBuf) -> Self {
        let resolver = PathResolver::new(&project_dir, source_dir);

        Self {
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

    /// Reject a URL path with a `..` component, so it cannot escape the source
    /// directory. `..` inside a name (`a..b`) is an ordinary page.
    ///
    /// Splits on `\` too: Windows joins treat it as a separator.
    fn validate_path(path: &str) -> Result<(), StorageError> {
        if path.split(['/', '\\']).any(|c| c == "..") {
            return Err(StorageError::new(StorageErrorKind::InvalidPath)
                .with_path(path)
                .with_backend(BACKEND));
        }
        Ok(())
    }

    /// The canonical path of the project file `path`, if
    /// [`Storage::read_file`] may serve it.
    fn project_file(&self, path: &str) -> Option<PathBuf> {
        let relative = Path::new(path);
        if !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
            || file_content_type(path).is_none()
        {
            return None;
        }
        // A symlink inside the project can point anywhere, so the resolved
        // file must be confined and allowlisted too.
        let file = self.project_dir.join(relative).canonicalize().ok()?;
        if !self.resolver.in_project(&file) || file.to_str().and_then(file_content_type).is_none() {
            return None;
        }
        let metadata = fs::metadata(&file).ok()?;
        (metadata.is_file() && metadata.len() <= MAX_FILE_SIZE).then_some(file)
    }

    /// Fails once the source directory, confined to the project at config
    /// load, has become a symlink out of it, as a `git pull` can make it.
    fn check_source_dir(&self) -> Result<(), StorageError> {
        let source_dir = self.resolver.source_dir();
        if fs::canonicalize(source_dir).is_ok_and(|dir| !self.resolver.in_project(&dir)) {
            return Err(StorageError::new(StorageErrorKind::InvalidPath)
                .with_path(source_dir)
                .with_backend(BACKEND)
                .with_source(io::Error::other(
                    "the source directory is outside the project",
                )));
        }
        Ok(())
    }

    /// Build the document at `url_path` from the sources the resolver locates
    /// in `listing`, or `None` when they form no page, with a warning per
    /// metadata problem. A page with a problem still builds, with the
    /// offending fields dropped.
    fn build_document(&self, url_path: &str, listing: &Listing) -> Option<(Document, Vec<String>)> {
        let exists = |path: &Path| listing.exists(path);
        let content_path = self.resolver.resolve_content_with(url_path, exists);
        let sidecar_path = self.resolver.resolve_meta_with(url_path, exists);
        let (meta, diagnostics) =
            self.meta_cache
                .resolve(url_path, content_path.as_deref(), sidecar_path.as_deref())?;
        let warnings = diagnostics
            .iter()
            .map(|diagnostic| {
                let file = match diagnostic.source {
                    DiagnosticSource::Frontmatter => &content_path,
                    DiagnosticSource::Sidecar => &sidecar_path,
                };
                match file {
                    Some(file) => format!("{}: {diagnostic}", self.resolver.display_path(file)),
                    None => diagnostic.to_string(),
                }
            })
            .collect();

        let document = Document {
            path: url_path.to_owned(),
            has_content: content_path.is_some(),
            meta,
            source: content_path.and_then(|path| self.resolver.source(&path)),
            diagnostics,
        };
        Some((document, warnings))
    }
}

/// The event for a change to an existing file under `url_path`.
fn modified_event_kind(
    resolver: &PathResolver,
    meta_cache: &MetaCache,
    url_path: &str,
) -> StorageEventKind {
    let content_path = resolver.resolve_content(url_path);
    let sidecar_path = resolver.resolve_meta(url_path);
    let before = meta_cache
        .0
        .read()
        .get(url_path)
        .map(|cached| Arc::clone(&cached.diagnostics))
        .unwrap_or_default();
    match meta_cache.resolve(url_path, content_path.as_deref(), sidecar_path.as_deref()) {
        // Only a rescan logs a metadata problem, and an unchanged meta
        // triggers none.
        Some((meta, diagnostics)) if diagnostics == before => StorageEventKind::Modified {
            meta: Meta::clone(&meta),
        },
        // Scan has no page here (e.g. a directory, or an emptied sidecar), so
        // there is no meta to compare: only a rescan can tell what changed.
        _ => StorageEventKind::Created,
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

/// Whether a watch event on `path` can change what a scan finds.
///
/// Directory events signal structural changes. A path that is gone may have
/// been a directory, since moving one out of the tree (`mv`, Finder's Move to
/// Trash) reports only the directory, never the files in it; a deleted
/// non-page file costs one lazy rescan.
fn is_relevant(path: &Path, rel_path: &Path, patterns: &[Pattern]) -> bool {
    patterns
        .iter()
        .any(|pattern| pattern.matches_path(rel_path))
        || path.is_dir()
        || !path.exists()
}

/// Whether `rel_path` names a file a page can depend on without being one: an
/// image, a diagram file, or a `PlantUML` include.
fn is_tracked_file(rel_path: &Path) -> bool {
    rel_path
        .to_str()
        .is_some_and(|rel| image_content_type(rel).is_some() || diagram_language(rel).is_some())
        || is_plantuml_file(rel_path)
}

/// The project path of the tracked file at `path`, as page renders name it;
/// `None` for any other path.
fn tracked_file(resolver: &PathResolver, path: &Path) -> Option<String> {
    let source_dir = resolver.source_in_project();
    let rel_path = path.strip_prefix(resolver.source_dir()).ok()?;
    if !is_tracked_file(rel_path) || path.is_dir() {
        return None;
    }
    let rel_path = dir_url_path(rel_path);
    Some(if source_dir.is_empty() {
        rel_path
    } else {
        format!("{source_dir}/{rel_path}")
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

    /// `path` relative to the root, in whichever form it was reported; `None`
    /// if it lies outside.
    fn relative<'a>(&mut self, path: &'a Path) -> Option<&'a Path> {
        if let Ok(rel) = path.strip_prefix(&self.dir) {
            return Some(rel);
        }
        if self.canonical.is_none() {
            self.canonical = self.dir.canonicalize().ok();
        }
        [Some(&self.absolute), self.canonical.as_ref()]
            .into_iter()
            .flatten()
            .find_map(|root| path.strip_prefix(root).ok())
    }

    /// `path` re-rooted under the configured root, so URL resolution
    /// downstream strips the same prefix; `None` if it lies outside.
    fn reroot(&mut self, path: &Path) -> Option<PathBuf> {
        let rel = self.relative(path)?;
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
    if let Some(path) = tracked_file(resolver, &event.path) {
        return StorageEvent::File { path };
    }
    let url_path = if let Ok(rel_path) = event.path.strip_prefix(resolver.source_dir()) {
        let filename = rel_path
            .file_name()
            .map(|f| f.to_string_lossy())
            .unwrap_or_default();
        // Unclassified paths passed `is_relevant` as directories.
        classify_relpath(rel_path, &filename)
            .map_or_else(|| dir_url_path(rel_path), Classification::into_url_path)
    } else {
        // Outside source_dir only the README is watched, and it serves the root.
        String::new()
    };

    let kind = match event.kind.reconcile(event.path.exists()) {
        RawEventKind::Created => StorageEventKind::Created,
        RawEventKind::Modified => modified_event_kind(resolver, meta_cache, &url_path),
        RawEventKind::Removed => StorageEventKind::Removed,
    };

    StorageEvent::Page {
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

/// Watch each existing README directory non-recursively.
///
/// Directories, not the file: inotify watches a file's inode, which an atomic
/// save (write a temp file, rename it over) replaces, ending the watch. Never
/// one inside `source_dir`: notify keeps one recursive flag per directory, so a
/// non-recursive watch there would downgrade the recursive one, which covers
/// it anyway.
fn watch_readme_dirs<'a>(
    watcher: &mut RecommendedWatcher,
    dirs: impl Iterator<Item = &'a Path>,
    source_dir: &Path,
) -> Result<(), StorageError> {
    let source_dir = source_dir.canonicalize().ok();
    let mut seen: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        let Ok(canonical) = dir.canonicalize() else {
            continue;
        };
        if canonical.is_dir()
            && !seen.contains(&canonical)
            && !source_dir
                .as_ref()
                .is_some_and(|src| canonical.starts_with(src))
        {
            watcher
                .watch(dir, RecursiveMode::NonRecursive)
                .map_err(watch_error)?;
            seen.push(canonical);
        }
    }
    Ok(())
}

impl Storage for FsStorage {
    fn scan(&self) -> Result<(Vec<Document>, Vec<String>), StorageError> {
        self.check_source_dir()?;
        let t0 = Instant::now();
        let listing = scanner::scan(&self.resolver);
        let walk_elapsed = t0.elapsed();

        let t1 = Instant::now();
        let (documents, meta_warnings): (Vec<Document>, Vec<Vec<String>>) = listing
            .url_paths
            .par_iter()
            .filter_map(|url_path| self.build_document(url_path, &listing))
            .unzip();
        let build_elapsed = t1.elapsed();

        tracing::info!(
            urls = listing.url_paths.len(),
            documents = documents.len(),
            walk_ms = format_args!("{:.1}", walk_elapsed.as_secs_f64() * 1000.0),
            build_ms = format_args!("{:.1}", build_elapsed.as_secs_f64() * 1000.0),
            total_ms = format_args!("{:.1}", t0.elapsed().as_secs_f64() * 1000.0),
            "Storage scan complete"
        );

        let mut warnings = listing.warnings;
        warnings.extend(meta_warnings.into_iter().flatten());
        Ok((documents, warnings))
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        Self::validate_path(path)?;
        self.check_source_dir()?;
        let full_path = self
            .resolver
            .resolve_content(path)
            .ok_or_else(|| StorageError::not_found(path).with_backend(BACKEND))?;
        fs::read_to_string(&full_path)
            .map_err(|e| StorageError::io(e, Some(PathBuf::from(path))).with_backend(BACKEND))
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        let not_found = || StorageError::not_found(path).with_backend(BACKEND);
        let io = |e| StorageError::io(e, Some(PathBuf::from(path))).with_backend(BACKEND);
        let file = self.project_file(path).ok_or_else(not_found)?;
        // The file can be swapped or grow after `project_file` looked at it,
        // so the open handle is checked again and the read is bounded.
        let file = File::open(file).map_err(io)?;
        let metadata = file.metadata().map_err(io)?;
        if !metadata.is_file() {
            return Err(not_found());
        }
        // `take` hides the length from `read_to_end`, which would grow the
        // buffer by doubling.
        let len = usize::try_from(metadata.len().min(MAX_FILE_SIZE)).unwrap_or_default();
        let mut bytes = Vec::with_capacity(len);
        file.take(MAX_FILE_SIZE + 1)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes.len() as u64 > MAX_FILE_SIZE {
            return Err(not_found());
        }
        Ok(bytes)
    }

    fn has_file(&self, path: &str) -> bool {
        self.project_file(path).is_some()
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

    fn source_dir_in_project(&self) -> String {
        self.resolver.source_in_project().to_owned()
    }
}

impl FsStorage {
    /// Start watching for document changes, and for changes to the images and
    /// diagram files under `source_dir` (see [`StorageEvent::File`]). Watching
    /// stops when the returned handle is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the file watcher cannot be started.
    pub fn watch(&self) -> Result<(StorageEventReceiver, WatchHandle), StorageError> {
        let (event_tx, event_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let debouncer = Arc::new(EventDebouncer::new(DEBOUNCE));

        let mut root = WatchRoot::new(self.resolver.source_dir().to_path_buf());
        let readme_watch = self.resolver.readme_watch();
        let readme_target = readme_watch.target_in_source;
        let mut readme_roots: Vec<(WatchRoot, OsString)> = readme_watch
            .dirs
            .iter()
            .map(|(dir, name)| (WatchRoot::new(dir.clone()), name.clone()))
            .collect();
        let readme_path = self.resolver.readme_path().to_path_buf();
        let patterns = watch_patterns();
        let debouncer_for_watcher = Arc::clone(&debouncer);

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let Some(kind) = storage_event_kind(event.kind) else {
                    return;
                };

                for path in event.paths {
                    // `to_storage_event` maps the README to root.
                    let mut is_readme = readme_roots
                        .iter_mut()
                        .any(|(dir, name)| dir.relative(&path) == Some(Path::new(name)));
                    if let Some(path) = root.reroot(&path)
                        && let Ok(rel_path) = path.strip_prefix(&root.dir)
                    {
                        // A symlinked README's target is also a page here.
                        is_readme |= readme_target.as_deref() == Some(rel_path);

                        // Mirror the scanner's hidden-file filtering so a
                        // hidden file (e.g. `.meta.yaml`) never produces an
                        // event the scan would not. Tracked files may sit in
                        // dot folders.
                        if is_tracked_file(rel_path)
                            || (!is_hidden_rel_path(rel_path)
                                && is_relevant(&path, rel_path, &patterns))
                        {
                            debouncer_for_watcher.record(path, kind);
                        }
                    }
                    if is_readme {
                        debouncer_for_watcher.record(readme_path.clone(), kind);
                    }
                }
            }
        })
        .map_err(watch_error)?;

        // Only watch source_dir if it is a directory. A README-only project
        // (no docs/) must still start; the drain thread upgrades to a recursive
        // watch if docs/ appears later. `is_dir()` (not `exists()`) matches the
        // upgrade helper: a non-directory at the path must not abort the watch
        // with a hard error.
        let mut recursive_active = if self.resolver.source_dir().is_dir() {
            watcher
                .watch(self.resolver.source_dir(), RecursiveMode::Recursive)
                .map_err(watch_error)?;
            true
        } else {
            false
        };

        watch_readme_dirs(
            &mut watcher,
            readme_watch.dirs.iter().map(|(dir, _)| dir.as_path()),
            self.resolver.source_dir(),
        )?;

        let resolver_for_drain = self.resolver.clone();
        let meta_cache = self.meta_cache.clone();
        thread::spawn(move || {
            // Whether a persistent recursive-watch upgrade failure was logged
            // already, so the warning is emitted at most once (not every tick).
            let mut upgrade_warned = false;

            loop {
                match shutdown_rx.recv_timeout(DRAIN_POLL) {
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
    use std::time::UNIX_EPOCH;

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
                let (first, _) = storage.scan().unwrap();
                let (second, _) = storage.scan().unwrap();
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
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();
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
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "payments").unwrap();

        assert_eq!(doc.meta.title, "Payments Service");
        assert_eq!(doc.meta.kind.as_deref(), Some("component"));
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
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();

        assert_eq!(docs, [] as [rw_storage::Document; 0]);
    }

    #[test]
    fn test_scan_missing_dir() {
        let storage = FsStorage::new(PathBuf::from("/nonexistent"), PathBuf::from("/nonexistent"));
        let (docs, _) = storage.scan().unwrap();

        assert_eq!(docs, [] as [rw_storage::Document; 0]);
    }

    #[test]
    fn test_scan_flat_structure() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# User Guide\n\nContent.").unwrap();
        fs::write(temp_dir.path().join("api.md"), "# API Reference\n\nDocs.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();

        assert_eq!(docs.len(), 2);
        let paths: Vec<_> = docs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"domain"));
        assert!(paths.contains(&"domain/guide"));
    }

    #[cfg(unix)]
    #[test]
    fn scan_keeps_a_backslash_in_a_folder_name() {
        let temp_dir = create_test_dir();
        let dir = temp_dir.path().join(r"a\b");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("index.md"), "# Folder").unwrap();
        fs::write(dir.join("child.md"), "# Child").unwrap();
        fs::write(dir.join("child.meta.yaml"), "description: sidecar").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let mut docs: Vec<_> = storage
            .scan()
            .unwrap()
            .0
            .into_iter()
            .map(|d| {
                (
                    d.path,
                    d.meta.title.clone(),
                    d.meta.description.clone(),
                    d.source,
                )
            })
            .collect();
        docs.sort();

        let expected = [
            (r"a\b", "Folder", None, r"a\b/index.md"),
            (r"a\b/child", "Child", Some("sidecar"), r"a\b/child.md"),
        ]
        .map(|(path, title, description, source)| {
            (
                path.to_owned(),
                title.to_owned(),
                description.map(str::to_owned),
                Some(source.to_owned()),
            )
        });
        assert_eq!(docs, expected);
        assert_eq!(storage.read(r"a\b/child").unwrap(), "# Child");
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
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].meta.title, "Setup Guide");
    }

    #[test]
    fn index_md_without_h1_is_titled_like_its_directory() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("setup-guide")).unwrap();
        fs::write(root.join("index.md"), "Root body.").unwrap();
        fs::write(root.join("setup-guide/index.md"), "Guide body.").unwrap();

        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        let mut titles: Vec<_> = storage
            .scan()
            .unwrap()
            .0
            .into_iter()
            .map(|d| (d.path, d.meta.title.clone()))
            .collect();
        titles.sort();

        assert_eq!(
            titles,
            [
                (String::new(), "Home".to_owned()),
                ("setup-guide".to_owned(), "Setup Guide".to_owned()),
            ]
        );
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden").unwrap();
        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.path, "domain");
        assert!(doc.has_content);
        assert_eq!(doc.meta.kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_scan_no_page_kind_without_kind_field() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "title: Home Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.meta.title, "My Nice Domain"); // Fallback to directory name
        assert_eq!(doc.meta.kind, Some("domain".to_owned()));
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
        let (docs, _) = storage.scan().unwrap();
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
        let (docs, _) = storage.scan().unwrap();
        let child = docs.iter().find(|d| d.path == "parent/child").unwrap();

        assert_eq!(child.meta.title, "Child");
        assert!(child.meta.description.is_none());
        assert!(child.meta.kind.is_none());
        assert!(child.meta.pages.is_none());
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
        let (docs1, _) = storage.scan().unwrap();
        let first = docs1.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(first.diagnostics.len(), 1);

        // Second scan without changes hits the meta cache: the Document
        // carries the same diagnostics, returned silently rather than re-logged.
        let (docs2, _) = storage.scan().unwrap();
        let second = docs2.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(second.diagnostics, first.diagnostics);
    }

    #[test]
    fn test_mtime_cache_detects_markdown_changes() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

        // First scan
        let (docs1, _) = storage.scan().unwrap();
        assert_eq!(docs1[0].meta.title, "Original Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify file
        fs::write(temp_dir.path().join("guide.md"), "# Updated Title").unwrap();

        // Second scan should see new title
        let (docs2, _) = storage.scan().unwrap();
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
        let (docs1, _) = storage.scan().unwrap();
        let guide1 = docs1.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(guide1.meta.title, "YAML Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify only meta.yaml, not the markdown file
        fs::write(guide_dir.join("meta.yaml"), "title: New YAML Title").unwrap();

        // Second scan should see new title from meta.yaml
        let (docs2, _) = storage.scan().unwrap();
        let guide2 = docs2.iter().find(|d| d.path == "guide").unwrap();
        assert_eq!(guide2.meta.title, "New YAML Title");
    }

    #[test]
    fn git_mode_returns_commit_time_filesystem_mode_returns_fs_time() {
        // Use `fixture.path()` as source_dir as-is, so gix's workdir and the
        // resolved file path share one form. Canonicalizing (macOS /var ->
        // /private/var, Windows `\\?\`) makes repo_relative_path's strip_prefix
        // miss, and git mode silently falls back to fs time.
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
        // README-only site: source_dir <repo>/docs does not exist, so git
        // discovery must run from the project dir. Do not canonicalize
        // `fixture.path()`; see git_mode_returns_commit_time_*.
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
    fn test_name_with_double_dot_is_scanned_and_readable() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("a..b.md"), "# A to B").unwrap();
        fs::write(temp_dir.path().join("c..d.meta.yaml"), "title: C to D").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        let mut paths: Vec<_> = storage
            .scan()
            .unwrap()
            .0
            .into_iter()
            .map(|d| d.path)
            .collect();
        paths.sort();

        assert_eq!(paths, ["a..b", "c..d"]);
        assert_eq!(storage.read("a..b").unwrap(), "# A to B");
        assert!(storage.mtime("a..b").is_ok());
        assert!(storage.mtime("c..d").is_ok());
    }

    #[test]
    fn test_read_rejects_dotdot_component_to_existing_file() {
        let temp_dir = create_test_dir();
        fs::create_dir(temp_dir.path().join("a")).unwrap();
        fs::write(temp_dir.path().join("b.md"), "# B").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

        for path in ["a/../b", "a\\..\\b"] {
            let err = storage.read(path).unwrap_err();
            assert_eq!(err.kind, StorageErrorKind::InvalidPath, "{path}");
        }
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

    /// A `project/` with a `docs/` source directory, beside an `outside/`
    /// directory holding `secret.png`.
    fn files_fixture() -> (tempfile::TempDir, FsStorage) {
        let temp = create_test_dir();
        let project = temp.path().join("project");
        for (path, content) in [
            ("project/docs/guide/img/flow.png", "flow"),
            ("project/docs/Logo.PNG", "logo"),
            ("project/.github/assets/badge.svg", "<svg/>"),
            ("project/docs/index.md", "# Home"),
            ("project/docs/spec.pdf", "pdf"),
            ("project/docs/config.yaml", "token: 1"),
            ("project/.env", "SECRET=1"),
            ("project/.rw/comments/sqlite.db", "db"),
            ("project/.git/config", "token"),
            ("outside/secret.png", "secret"),
        ] {
            let path = temp.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        let storage = FsStorage::new(project.clone(), project.join("docs"));
        (temp, storage)
    }

    #[track_caller]
    fn assert_file_not_found(storage: &FsStorage, path: &str) {
        let err = storage.read_file(path).unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::NotFound, "{path}");
        assert_eq!(err.backend, Some("Fs"), "{path}");
        assert!(!storage.has_file(path), "{path}");
    }

    #[track_caller]
    fn assert_file(storage: &FsStorage, path: &str, content: &str) {
        assert_eq!(
            storage.read_file(path).unwrap(),
            content.as_bytes(),
            "{path}"
        );
        assert!(storage.has_file(path), "{path}");
    }

    #[test]
    fn read_file_serves_allowlisted_files_anywhere_under_the_project() {
        let (_temp, storage) = files_fixture();

        for (path, content) in [
            ("docs/guide/img/flow.png", "flow"),
            ("docs/Logo.PNG", "logo"),
            (".github/assets/badge.svg", "<svg/>"),
            ("docs/spec.pdf", "pdf"),
        ] {
            assert_file(&storage, path, content);
        }
    }

    #[test]
    fn read_file_refuses_files_over_the_size_cap() {
        let (temp, storage) = files_fixture();
        let docs = temp.path().join("project/docs");
        fs::File::create(docs.join("fits.png"))
            .unwrap()
            .set_len(MAX_FILE_SIZE)
            .unwrap();
        fs::File::create(docs.join("huge.png"))
            .unwrap()
            .set_len(MAX_FILE_SIZE + 1)
            .unwrap();

        assert!(storage.has_file("docs/fits.png"));
        assert_file_not_found(&storage, "docs/huge.png");
    }

    #[test]
    fn read_file_refuses_files_off_the_allowlist() {
        let (_temp, storage) = files_fixture();

        for path in [
            "docs/index.md",
            "docs/config.yaml",
            ".env",
            ".rw/comments/sqlite.db",
        ] {
            assert_file_not_found(&storage, path);
        }
    }

    #[test]
    fn read_file_refuses_paths_that_are_not_plain_and_relative() {
        let (temp, storage) = files_fixture();
        let absolute = temp.path().join("project/docs/guide/img/flow.png");

        for path in [
            "",
            "../outside/secret.png",
            "docs/../docs/guide/img/flow.png",
            "./docs/guide/img/flow.png",
            absolute.to_str().unwrap(),
        ] {
            assert_file_not_found(&storage, path);
        }
    }

    #[test]
    fn read_file_reports_a_missing_file_as_not_found() {
        let (temp, storage) = files_fixture();
        fs::create_dir(temp.path().join("project/docs/folder.png")).unwrap();

        for path in ["docs/missing.png", "docs/folder.png"] {
            assert_file_not_found(&storage, path);
        }
    }

    #[cfg(unix)]
    #[test]
    fn read_file_follows_symlinks_only_within_the_project() {
        let (temp, storage) = files_fixture();
        let docs = temp.path().join("project/docs");
        let outside = temp.path().join("outside");
        std::os::unix::fs::symlink(docs.join("guide/img/flow.png"), docs.join("alias.png"))
            .unwrap();
        std::os::unix::fs::symlink(outside.join("secret.png"), docs.join("escape.png")).unwrap();
        std::os::unix::fs::symlink(&outside, docs.join("outside")).unwrap();
        std::os::unix::fs::symlink("../.env", docs.join("env.png")).unwrap();
        std::os::unix::fs::symlink("../.rw/comments/sqlite.db", docs.join("db.svg")).unwrap();
        std::os::unix::fs::symlink("../.git/config", docs.join("cfg.png")).unwrap();

        assert_file(&storage, "docs/alias.png", "flow");
        for path in [
            "docs/escape.png",
            "docs/outside/secret.png",
            "docs/env.png",
            "docs/db.svg",
            "docs/cfg.png",
        ] {
            assert_file_not_found(&storage, path);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_source_dir_that_became_a_symlink_out_of_the_project_serves_nothing() {
        let temp = create_test_dir();
        let project = temp.path().join("project");
        let shared = temp.path().join("shared/docs");
        fs::create_dir_all(project.join("docs")).unwrap();
        fs::create_dir_all(shared.join("img")).unwrap();
        fs::write(project.join("docs/index.md"), "# Home").unwrap();
        fs::write(shared.join("index.md"), "# Shared").unwrap();
        fs::write(shared.join("img/flow.png"), "flow").unwrap();
        let storage = FsStorage::new(project.clone(), project.join("docs"));
        assert!(storage.scan().is_ok());

        fs::remove_dir_all(project.join("docs")).unwrap();
        std::os::unix::fs::symlink(&shared, project.join("docs")).unwrap();

        assert_eq!(
            storage.scan().unwrap_err().kind,
            StorageErrorKind::InvalidPath
        );
        assert_eq!(
            storage.read("").unwrap_err().kind,
            StorageErrorKind::InvalidPath
        );
        assert_file_not_found(&storage, "docs/img/flow.png");
    }

    #[test]
    fn test_watch_succeeds_with_existing_source_dir() {
        let temp_dir = create_test_dir();
        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());

        assert!(storage.watch().is_ok());
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
            root.reroot(&real.join("sub/page.md")),
            Some(link.join("sub/page.md"))
        );
        assert_eq!(
            root.reroot(&link.join("page.md")),
            Some(link.join("page.md"))
        );
        assert_eq!(root.reroot(&temp_dir.path().join("other/page.md")), None);
        assert_eq!(root.reroot(&real), Some(link));
    }

    #[test]
    fn watch_root_reroots_absolute_paths_under_a_relative_source_dir() {
        let dir = PathBuf::from("rw-watch-root-missing-docs");
        let mut root = WatchRoot::new(dir.clone());
        let absolute = std::env::current_dir().unwrap().join(&dir);

        assert_eq!(
            root.reroot(&absolute.join("page.md")),
            Some(dir.join("page.md"))
        );
    }

    /// Receives until a page event `wanted` matches, skipping other events:
    /// `FSEvents` may replay recent history when a watch starts.
    fn recv_until(rx: &StorageEventReceiver, wanted: impl Fn(&str, &StorageEventKind) -> bool) {
        recv_matching(
            rx,
            |event| matches!(event, StorageEvent::Page { path, kind } if wanted(path, kind)),
        );
    }

    fn recv_matching(rx: &StorageEventReceiver, wanted: impl Fn(&StorageEvent) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(event) if wanted(&event) => return,
                Ok(event) => seen.push(event),
                Err(_) => break,
            }
        }
        panic!("expected event never arrived, got: {seen:?}");
    }

    #[test]
    fn watch_reports_a_created_page() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(root.join("new.md"), "# New").unwrap();

        recv_until(&rx, |path, kind| {
            path == "new" && *kind != StorageEventKind::Removed
        });
    }

    #[test]
    fn tracked_file_names_an_image_or_diagram_by_its_project_path() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        let docs = root.join("docs");
        fs::create_dir_all(docs.join("img.png")).unwrap();
        let nested = PathResolver::new(root, docs.clone());
        let at_root = PathResolver::new(root, root.to_path_buf());

        assert_eq!(
            tracked_file(&nested, &docs.join("guide/a.PNG")).as_deref(),
            Some("docs/guide/a.PNG")
        );
        assert_eq!(
            tracked_file(&nested, &docs.join(".assets/a.svg")).as_deref(),
            Some("docs/.assets/a.svg")
        );
        assert_eq!(
            tracked_file(&at_root, &root.join("a.png")).as_deref(),
            Some("a.png")
        );
        for name in [
            "flow.PUML",
            "flow.mmd",
            "flow.dot",
            "parts.iuml",
            "x.vg.json",
        ] {
            assert_eq!(
                tracked_file(&nested, &docs.join(name)),
                Some(format!("docs/{name}"))
            );
        }
        assert_eq!(tracked_file(&nested, &docs.join(".env")), None);
        assert_eq!(tracked_file(&nested, &docs.join("notes.txt")), None);
        assert_eq!(tracked_file(&nested, &docs.join("x.json")), None);
        assert_eq!(tracked_file(&nested, &docs.join("spec.pdf")), None);
        assert_eq!(tracked_file(&nested, &docs.join("guide.md")), None);
        assert_eq!(tracked_file(&nested, &docs.join("img.png")), None);
        assert_eq!(tracked_file(&nested, &root.join("a.png")), None);
    }

    fn recv_file(rx: &StorageEventReceiver, path: &str) {
        recv_matching(rx, |e| *e == StorageEvent::File { path: path.into() });
    }

    #[test]
    fn watch_reports_image_edits_and_creations_by_project_path() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        let docs = root.join("docs");
        fs::create_dir_all(docs.join("img")).unwrap();
        fs::create_dir_all(docs.join(".assets")).unwrap();
        fs::write(docs.join("img/a.png"), "a").unwrap();
        let storage = FsStorage::new(root.to_path_buf(), docs.clone());
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(docs.join("img/a.png"), "b").unwrap();
        recv_file(&rx, "docs/img/a.png");
        fs::write(docs.join("img/new.png"), "c").unwrap();
        recv_file(&rx, "docs/img/new.png");
        fs::write(docs.join(".assets/a.png"), "d").unwrap();
        recv_file(&rx, "docs/.assets/a.png");
    }

    #[test]
    fn watch_reports_diagram_files_and_includes_but_no_other_file() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        let storage = FsStorage::new(root.to_path_buf(), docs.clone());
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(docs.join(".env"), "SECRET=1").unwrap();
        fs::write(docs.join("notes.txt"), "notes").unwrap();
        thread::sleep(Duration::from_millis(300));
        fs::write(docs.join("flow.puml"), "A -> B").unwrap();
        fs::write(docs.join("parts.iuml"), "B -> C").unwrap();

        let wanted = ["docs/flow.puml", "docs/parts.iuml"];
        let mut seen = Vec::new();
        let mut deadline = Instant::now() + Duration::from_secs(5);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let Ok(event) = rx.recv_timeout(left) else {
                break;
            };
            let (StorageEvent::Page { path, .. } | StorageEvent::File { path }) = event;
            seen.push(path);
            if wanted.iter().all(|path| seen.contains(&(*path).to_owned())) {
                deadline = deadline.min(Instant::now() + Duration::from_millis(300));
            }
        }
        for path in wanted {
            assert!(seen.iter().any(|seen| seen == path), "{seen:?}");
        }
        assert!(
            !seen
                .iter()
                .any(|path| path.contains(".env") || path.contains("notes")),
            "{seen:?}"
        );
    }

    #[test]
    fn watch_reports_a_deleted_page() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        fs::write(root.join("to-delete.md"), "# Delete Me").unwrap();
        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        let (rx, _handle) = storage.watch().unwrap();

        fs::remove_file(root.join("to-delete.md")).unwrap();

        recv_until(&rx, |path, kind| {
            path == "to-delete" && *kind == StorageEventKind::Removed
        });
    }

    #[test]
    fn watch_reports_pages_in_a_source_dir_created_after_start() {
        let temp_dir = create_test_dir();
        let docs = temp_dir.path().join("docs");
        let storage = FsStorage::new(temp_dir.path().to_path_buf(), docs.clone());
        let (rx, _handle) = storage.watch().unwrap();

        fs::create_dir(&docs).unwrap();
        // The upgrade's synthetic root event: a write before it goes unreported.
        recv_until(&rx, |path, _| path.is_empty());
        fs::write(docs.join("guide.md"), "# Guide").unwrap();

        recv_until(&rx, |path, _| path == "guide");
    }

    /// A project with an empty `docs/` and a `README.md` at its root.
    fn create_readme_test_dir(readme_content: &str) -> (tempfile::TempDir, FsStorage) {
        let temp_dir = create_test_dir();
        let project_root = temp_dir.path();
        let source_dir = project_root.join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(project_root.join("README.md"), readme_content).unwrap();

        let storage = FsStorage::new(project_root.to_path_buf(), source_dir);
        (temp_dir, storage)
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
            .0
            .into_iter()
            .find(|d| d.path == "guide")
            .unwrap();
        let second = storage
            .scan()
            .unwrap()
            .0
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
            .0
            .into_iter()
            .find(|d| d.path == "payments")
            .unwrap();
        let second = storage
            .scan()
            .unwrap()
            .0
            .into_iter()
            .find(|d| d.path == "payments")
            .unwrap();

        assert!(Arc::ptr_eq(&first.meta, &second.meta));
    }

    #[test]
    fn readme_fallback_frontmatter_is_preserved_and_shared_across_unchanged_scans() {
        let (_dir, storage) = create_readme_test_dir(
            "---\ntitle: Readme Home\ndescription: Project docs\nkind: domain\nnamespace: docs\nattrs: {owner: project, nested: [null, true]}\npages:\n  - guide\n---\n# Body Title",
        );
        let first = storage
            .scan()
            .unwrap()
            .0
            .into_iter()
            .find(|d| d.path.is_empty())
            .unwrap();
        let second = storage
            .scan()
            .unwrap()
            .0
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
        assert_eq!(first.source.as_deref(), Some("README.md"));
    }

    #[test]
    fn readme_homepage_with_malformed_frontmatter_keeps_page_with_error_diagnostic() {
        let (_dir, storage) = create_readme_test_dir("---\ntitle: [unclosed\n---\n# Hello\n");
        let (docs, _) = storage.scan().unwrap();

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
    fn readme_does_not_override_an_existing_index() {
        let (dir, storage) = create_readme_test_dir("# README");
        fs::write(dir.path().join("docs/index.md"), "# Docs Home").unwrap();
        let (docs, _) = storage.scan().unwrap();

        let homes: Vec<_> = docs.iter().filter(|d| d.path.is_empty()).collect();
        assert_eq!(homes.len(), 1);
        assert_eq!(homes[0].meta.title, "Docs Home");
        assert_eq!(storage.read("").unwrap(), "# Docs Home");
    }

    #[test]
    fn test_mtime_works_for_readme_homepage() {
        let (_dir, storage) = create_readme_test_dir("# Home");
        let mtime = storage.mtime("").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert!(mtime > now - 60.0);
        assert!(mtime <= now);
    }

    #[test]
    fn watch_reports_a_readme_homepage_edit() {
        let (dir, storage) = create_readme_test_dir("# Original");
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(dir.path().join("README.md"), "# Modified").unwrap();

        recv_until(&rx, |path, kind| {
            path.is_empty()
                && matches!(kind, StorageEventKind::Modified { meta } if meta.title == "Modified")
        });
    }

    #[test]
    fn watch_reports_every_atomic_save_of_a_readme_homepage() {
        let (dir, storage) = create_readme_test_dir("# Original");
        let (rx, _handle) = storage.watch().unwrap();
        let readme = dir.path().join("README.md");
        let temp = dir.path().join("README.md.tmp");

        for title in ["First", "Second"] {
            fs::write(&temp, format!("# {title}")).unwrap();
            fs::rename(&temp, &readme).unwrap();

            recv_until(&rx, |path, kind| {
                path.is_empty()
                    && matches!(kind, StorageEventKind::Modified { meta } if meta.title == title)
            });
        }
    }

    #[cfg(unix)]
    #[test]
    fn watch_reports_an_edit_to_a_symlinked_readme_homepages_target() {
        let (dir, storage) = create_readme_test_dir("# Unused");
        let shared = dir.path().join("shared");
        fs::create_dir(&shared).unwrap();
        fs::write(shared.join("README.md"), "# Original").unwrap();
        fs::remove_file(dir.path().join("README.md")).unwrap();
        std::os::unix::fs::symlink(shared.join("README.md"), dir.path().join("README.md")).unwrap();
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(shared.join("README.md"), "# Modified").unwrap();

        recv_until(&rx, |path, kind| {
            path.is_empty()
                && matches!(kind, StorageEventKind::Modified { meta } if meta.title == "Modified")
        });
    }

    /// A project whose `README.md` is a symlink to `docs/{target}`, with a
    /// nested page at `docs/sub/page.md`.
    #[cfg(unix)]
    fn create_readme_linked_into_docs_test_dir(target: &str) -> (tempfile::TempDir, FsStorage) {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        let docs = root.join("docs");
        fs::create_dir_all(docs.join("sub")).unwrap();
        fs::write(docs.join(target), "# Original").unwrap();
        fs::write(docs.join("sub/page.md"), "# Page").unwrap();
        std::os::unix::fs::symlink(docs.join(target), root.join("README.md")).unwrap();

        let storage = FsStorage::new(root.to_path_buf(), docs);
        (temp_dir, storage)
    }

    #[cfg(unix)]
    #[test]
    fn a_readme_symlinked_out_of_the_project_is_no_homepage() {
        let temp_dir = create_test_dir();
        let project = temp_dir.path().join("project");
        fs::create_dir_all(project.join("docs")).unwrap();
        fs::write(temp_dir.path().join("secret.md"), "# Secret").unwrap();
        std::os::unix::fs::symlink(temp_dir.path().join("secret.md"), project.join("README.md"))
            .unwrap();
        let storage = FsStorage::new(project.clone(), project.join("docs"));

        let (docs, _) = storage.scan().unwrap();

        assert!(!docs.iter().any(|doc| doc.has_content), "{docs:?}");
        assert_eq!(
            storage.read("").unwrap_err().kind,
            StorageErrorKind::NotFound
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_readme_symlinked_into_docs_is_the_homepage() {
        let (_dir, storage) = create_readme_linked_into_docs_test_dir("overview.md");

        let (docs, _) = storage.scan().unwrap();

        assert!(
            docs.iter()
                .any(|doc| doc.path.is_empty() && doc.has_content)
        );
        assert_eq!(storage.read("").unwrap(), "# Original");
    }

    #[cfg(unix)]
    #[test]
    fn readme_linked_into_docs_keeps_nested_edits_reported() {
        let (dir, storage) = create_readme_linked_into_docs_test_dir("index.md");
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(dir.path().join("docs/sub/page.md"), "# Edited").unwrap();

        recv_until(&rx, |path, _| path == "sub/page");
    }

    #[cfg(unix)]
    #[test]
    fn readme_linked_into_docs_keeps_nested_creations_reported() {
        let (dir, storage) = create_readme_linked_into_docs_test_dir("index.md");
        let (rx, _handle) = storage.watch().unwrap();

        fs::create_dir(dir.path().join("docs/new")).unwrap();
        // inotify watches a new directory only once it sees it created.
        recv_until(&rx, |path, _| path == "new");
        fs::write(dir.path().join("docs/new/p.md"), "# P").unwrap();

        recv_until(&rx, |path, _| path == "new/p");
    }

    #[cfg(unix)]
    #[test]
    fn readme_homepage_linked_into_docs_reports_target_edits_at_the_root() {
        let (dir, storage) = create_readme_linked_into_docs_test_dir("overview.md");
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(dir.path().join("docs/overview.md"), "# Modified").unwrap();

        recv_until(&rx, |path, kind| {
            path.is_empty()
                && matches!(kind, StorageEventKind::Modified { meta } if meta.title == "Modified")
        });
    }

    #[cfg(unix)]
    #[test]
    fn readme_homepage_linked_into_docs_reports_target_edits_through_a_symlinked_project() {
        let temp_dir = create_test_dir();
        let real = temp_dir.path().join("real");
        fs::create_dir_all(real.join("docs")).unwrap();
        fs::write(real.join("docs/overview.md"), "# Original").unwrap();
        std::os::unix::fs::symlink(real.join("docs/overview.md"), real.join("README.md")).unwrap();
        let link = temp_dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let storage = FsStorage::new(link.clone(), link.join("docs"));
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(real.join("docs/overview.md"), "# Modified").unwrap();

        recv_until(&rx, |path, kind| {
            path.is_empty()
                && matches!(kind, StorageEventKind::Modified { meta } if meta.title == "Modified")
        });
    }

    #[cfg(unix)]
    #[test]
    fn readme_homepage_in_source_dir_linked_outside_reports_target_edits() {
        let temp_dir = create_test_dir();
        let project = temp_dir.path().join("proj");
        let shared = temp_dir.path().join("shared");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&shared).unwrap();
        fs::write(shared.join("README.md"), "# Original").unwrap();
        std::os::unix::fs::symlink(shared.join("README.md"), project.join("README.md")).unwrap();
        let storage = FsStorage::new(project.clone(), project.join("."));
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(shared.join("README.md"), "# Modified").unwrap();

        recv_until(&rx, |path, kind| {
            path.is_empty()
                && matches!(kind, StorageEventKind::Modified { meta } if meta.title == "Modified")
        });
    }

    #[test]
    fn watch_reports_a_readme_homepage_created_after_start() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        fs::create_dir(root.join("docs")).unwrap();
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(root.join("README.md"), "# Late").unwrap();

        recv_until(&rx, |path, kind| {
            path.is_empty() && *kind != StorageEventKind::Removed
        });
    }

    /// A project whose `source_dir` is the project root (`source_dir = "."`),
    /// holding a `README.md` and a `guide.md`.
    fn create_readme_in_source_dir_test_dir() -> (tempfile::TempDir, FsStorage) {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        fs::write(root.join("README.md"), "# Readme Home").unwrap();
        fs::write(root.join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(root.to_path_buf(), root.join("."));
        (temp_dir, storage)
    }

    #[test]
    fn readme_homepage_inside_source_dir_is_served_only_at_the_root() {
        let (_dir, storage) = create_readme_in_source_dir_test_dir();

        let mut paths: Vec<_> = storage
            .scan()
            .unwrap()
            .0
            .into_iter()
            .map(|d| d.path)
            .collect();
        paths.sort();
        assert_eq!(paths, ["", "guide"]);
        assert_eq!(storage.read("").unwrap(), "# Readme Home");
        assert_eq!(
            storage.read("README").unwrap_err().kind,
            StorageErrorKind::NotFound
        );
    }

    #[test]
    fn readme_homepage_inside_source_dir_is_read_from_there() {
        let (_dir, storage) = create_readme_in_source_dir_test_dir();

        let (docs, _) = storage.scan().unwrap();
        let home = docs.iter().find(|d| d.path.is_empty()).unwrap();

        assert_eq!(home.source.as_deref(), Some("README.md"));
    }

    #[test]
    fn an_index_excludes_the_readme_next_to_it() {
        let (dir, storage) = create_readme_in_source_dir_test_dir();
        fs::write(dir.path().join("index.md"), "# Index Home").unwrap();

        let (docs, _) = storage.scan().unwrap();
        let mut paths: Vec<_> = docs.iter().map(|d| d.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, ["", "guide"]);
        let home = docs.iter().find(|d| d.path.is_empty()).unwrap();
        assert_eq!(home.source.as_deref(), Some("index.md"));
        assert_eq!(storage.read("").unwrap(), "# Index Home");
        assert_eq!(
            storage.read("README").unwrap_err().kind,
            StorageErrorKind::NotFound
        );
    }

    #[test]
    fn a_folder_readme_serves_its_folder() {
        let temp_dir = create_test_dir();
        let docs = temp_dir.path().join("docs");
        fs::create_dir_all(docs.join("setup-guide")).unwrap();
        fs::write(docs.join("setup-guide/README.md"), "Body.").unwrap();
        fs::write(docs.join("setup-guide/step.md"), "# Step").unwrap();
        let storage = FsStorage::new(temp_dir.path().to_path_buf(), docs);

        let (scanned, _) = storage.scan().unwrap();
        let mut paths: Vec<_> = scanned.iter().map(|d| d.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, ["setup-guide", "setup-guide/step"]);
        let folder = scanned.iter().find(|d| d.path == "setup-guide").unwrap();
        assert_eq!(folder.source.as_deref(), Some("docs/setup-guide/README.md"));
        assert_eq!(folder.meta.title, "Setup Guide");
        assert_eq!(storage.read("setup-guide").unwrap(), "Body.");
        assert_eq!(
            storage.read("setup-guide/README").unwrap_err().kind,
            StorageErrorKind::NotFound
        );
    }

    #[test]
    fn source_dir_in_project_places_the_source_dir_in_the_project() {
        let temp_dir = create_test_dir();
        let project_dir = temp_dir.path().join("project");

        fs::create_dir_all(project_dir.join("site")).unwrap();
        for (source_dir, expected) in [
            (project_dir.join("site/docs"), "site/docs"),
            (project_dir.clone(), ""),
            // Missing, so only its existing ancestor can be resolved.
            (project_dir.join("site/../docs"), "docs"),
        ] {
            let storage = FsStorage::new(project_dir.clone(), source_dir.clone());

            assert_eq!(
                storage.source_dir_in_project(),
                expected,
                "{}",
                source_dir.display()
            );
        }
    }

    #[test]
    fn a_documents_source_names_the_file_it_is_read_from() {
        for source_dir in ["docs", "site/docs"] {
            let temp_dir = create_test_dir();
            for file in [
                "README.md",
                "a/README.md",
                "b/index.md",
                "b/README.md",
                "c.md",
                "d/e.md",
            ] {
                let file = if file == "README.md" {
                    file.to_owned()
                } else {
                    format!("{source_dir}/{file}")
                };
                let path = temp_dir.path().join(&file);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, file).unwrap();
            }
            let storage = FsStorage::new(
                temp_dir.path().to_path_buf(),
                temp_dir.path().join(source_dir),
            );

            let mut sources = Vec::new();
            for document in storage.scan().unwrap().0 {
                let source = document.source.unwrap();
                assert_eq!(storage.read(&document.path).unwrap(), source);
                sources.push(source);
            }
            sources.sort_unstable();
            assert_eq!(
                sources,
                [
                    "README.md".to_owned(),
                    format!("{source_dir}/a/README.md"),
                    format!("{source_dir}/b/index.md"),
                    format!("{source_dir}/c.md"),
                    format!("{source_dir}/d/e.md"),
                ]
            );
        }
    }

    /// Case-insensitive filesystems (APFS, NTFS) find `readme.md` when asked
    /// for `README.md`; such a file must not serve any page.
    #[test]
    fn a_differently_cased_dir_page_serves_nothing() {
        let (dir, storage) = create_readme_test_dir("# Project Readme");
        let docs = dir.path().join("docs");
        fs::write(docs.join("readme.md"), "# Lower Readme").unwrap();
        for (leaf, dir_page) in [("a", "readme.md"), ("b", "Index.md")] {
            fs::create_dir_all(docs.join(leaf)).unwrap();
            fs::write(docs.join(format!("{leaf}.md")), "# Leaf").unwrap();
            fs::write(docs.join(leaf).join(dir_page), "# Hijack").unwrap();
        }

        let (scanned, _) = storage.scan().unwrap();
        for (path, body, source) in [
            ("", "# Project Readme", "README.md"),
            ("a", "# Leaf", "docs/a.md"),
            ("b", "# Leaf", "docs/b.md"),
        ] {
            assert_eq!(storage.read(path).unwrap(), body, "{path:?}");
            let doc = scanned.iter().find(|d| d.path == path).unwrap();
            assert_eq!(doc.source.as_deref(), Some(source), "{path:?}");
        }
    }

    #[test]
    fn an_index_excludes_a_folder_readme() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/index.md"), "# Index").unwrap();
        fs::write(root.join("a/README.md"), "# Readme").unwrap();
        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());

        let (scanned, _) = storage.scan().unwrap();
        let paths: Vec<_> = scanned.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["a"]);
        assert_eq!(scanned[0].source.as_deref(), Some("a/index.md"));
        assert_eq!(storage.read("a").unwrap(), "# Index");
    }

    #[test]
    fn a_docs_readme_homepage_wins_over_the_project_readme() {
        let (dir, storage) = create_readme_test_dir("# Project Readme");
        fs::write(dir.path().join("docs/README.md"), "# Docs Readme").unwrap();

        let (scanned, _) = storage.scan().unwrap();
        let home = scanned.iter().find(|d| d.path.is_empty()).unwrap();

        assert_eq!(storage.read("").unwrap(), "# Docs Readme");
        assert_eq!(home.source.as_deref(), Some("docs/README.md"));
    }

    #[test]
    fn watch_reports_a_folder_readme_edit_at_its_folder() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/README.md"), "# Readme").unwrap();
        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(root.join("a/README.md"), "# Modified").unwrap();

        recv_until(&rx, |path, kind| {
            path == "a"
                && matches!(kind, StorageEventKind::Modified { meta } if meta.title == "Modified")
        });
    }

    #[test]
    fn watch_hands_a_folder_to_its_readme_when_the_index_goes() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/index.md"), "# Index").unwrap();
        fs::write(root.join("a/README.md"), "# Readme").unwrap();
        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        let (rx, _handle) = storage.watch().unwrap();

        fs::remove_file(root.join("a/index.md")).unwrap();

        recv_until(&rx, |path, kind| {
            path == "a" && *kind == StorageEventKind::Removed
        });
        assert_eq!(storage.read("a").unwrap(), "# Readme");
    }

    #[test]
    fn watch_reports_a_readme_homepage_edit_inside_source_dir() {
        let (dir, storage) = create_readme_in_source_dir_test_dir();
        let (rx, _handle) = storage.watch().unwrap();

        fs::write(dir.path().join("README.md"), "# Modified").unwrap();

        recv_until(&rx, |path, kind| {
            path.is_empty()
                && matches!(kind, StorageEventKind::Modified { meta } if meta.title == "Modified")
        });
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
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();

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
        let (docs, _) = storage.scan().unwrap();
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
        let (docs, _) = storage.scan().unwrap();
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
        let (docs, _) = storage.scan().unwrap();
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
        let (docs, _) = storage.scan().unwrap();
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
        let (docs, _) = storage.scan().unwrap();
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
        let (docs, _) = storage.scan().unwrap();

        for path in ["guide", "billing"] {
            let doc = docs.iter().find(|d| d.path == path).unwrap();
            assert!(doc.diagnostics.is_empty(), "{path} should be clean");
        }
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
        let resolver = PathResolver::new(temp_dir.path(), temp_dir.path().to_path_buf());
        let StorageEvent::Page {
            path,
            kind: StorageEventKind::Modified { meta },
        } = to_storage_event(&event, &resolver, &MetaCache::default())
        else {
            panic!("expected Modified");
        };
        assert_eq!(path, "payments");
        assert_eq!(meta.title, "Payments Service");
    }

    #[test]
    fn modified_event_with_a_new_metadata_problem_is_reported_created() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let temp_dir = create_test_dir();
        let page = temp_dir.path().join("a.md");
        fs::write(&page, "# A\n").unwrap();
        let storage = FsStorage::new(temp_dir.path().to_path_buf(), temp_dir.path().to_path_buf());
        storage.scan().unwrap();
        // The meta stays the same: the invalid namespace is dropped.
        fs::write(&page, "---\nnamespace: bad/value\n---\n# A\n").unwrap();

        let event = DebouncedEvent {
            path: page,
            kind: RawEventKind::Modified,
        };

        assert_eq!(
            to_storage_event(&event, &storage.resolver, &storage.meta_cache),
            StorageEvent::Page {
                path: "a".into(),
                kind: StorageEventKind::Created,
            }
        );
    }

    #[test]
    fn modified_event_for_a_missing_file_is_reported_removed() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let temp_dir = create_test_dir();
        let event = DebouncedEvent {
            path: temp_dir.path().join("gone.md"),
            kind: RawEventKind::Modified,
        };
        let resolver = PathResolver::new(temp_dir.path(), temp_dir.path().to_path_buf());

        assert_eq!(
            to_storage_event(&event, &resolver, &MetaCache::default()),
            StorageEvent::Page {
                path: "gone".into(),
                kind: StorageEventKind::Removed,
            }
        );
    }

    #[test]
    fn test_modified_event_on_readme_homepage_uses_its_h1() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let project_root = create_test_dir();
        let source_dir = project_root.path().join("docs"); // does not exist
        fs::write(project_root.path().join("README.md"), "# Readme Home Title").unwrap();

        let event = DebouncedEvent {
            path: project_root.path().join("README.md"),
            kind: RawEventKind::Modified,
        };
        let resolver = PathResolver::new(project_root.path(), source_dir);
        let StorageEvent::Page {
            path,
            kind: StorageEventKind::Modified { meta },
        } = to_storage_event(&event, &resolver, &MetaCache::default())
        else {
            panic!("expected Modified");
        };
        assert_eq!(path, "");
        assert_eq!(meta.title, "Readme Home Title");
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
            .0
            .into_iter()
            .find(|d| d.path.is_empty())
            .expect("README homepage document")
            .meta
            .title
            .clone();

        let resolver = PathResolver::new(root, docs);
        let event = DebouncedEvent {
            path: root.join("README.md"),
            kind: RawEventKind::Modified,
        };
        let StorageEvent::Page {
            kind: StorageEventKind::Modified { meta },
            ..
        } = to_storage_event(&event, &resolver, &MetaCache::default())
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
    fn modified_event_for_a_source_that_forms_no_page_is_reported_created() {
        use crate::debouncer::{DebouncedEvent, RawEventKind};

        let dir = create_test_dir();
        let root = dir.path();
        fs::write(root.join("emptied.meta.yaml"), "  \n").unwrap();

        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        assert_eq!(storage.scan().unwrap().0, Vec::<Document>::new());

        let resolver = PathResolver::new(root, root.to_path_buf());
        let event = DebouncedEvent {
            path: root.join("emptied.meta.yaml"),
            kind: RawEventKind::Modified,
        };
        assert_eq!(
            to_storage_event(&event, &resolver, &MetaCache::default()),
            StorageEvent::Page {
                path: "emptied".into(),
                kind: StorageEventKind::Created,
            }
        );
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
            .0
            .into_iter()
            .find(|d| d.path.is_empty())
            .expect("root document")
            .meta;

        let resolver = PathResolver::new(root, root.to_path_buf());
        let event = DebouncedEvent {
            path: root.join("meta.yaml"),
            kind: RawEventKind::Modified,
        };
        let StorageEvent::Page {
            kind: StorageEventKind::Modified { meta },
            ..
        } = to_storage_event(&event, &resolver, &MetaCache::default())
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

    #[test]
    fn is_relevant_drops_only_existing_non_page_files() {
        let dir = create_test_dir();
        let root = dir.path();
        fs::create_dir(root.join("notes")).unwrap();
        fs::write(root.join("guide.md"), "# Guide").unwrap();
        fs::write(root.join("note.txt"), "").unwrap();
        let patterns = watch_patterns();
        let relevant = |rel: &str| is_relevant(&root.join(rel), Path::new(rel), &patterns);

        assert!(relevant("guide.md"));
        assert!(relevant("gone.md"));
        assert!(relevant("notes"));
        assert!(relevant("moved-away"));
        assert!(relevant("deleted.png"));
        assert!(!relevant("note.txt"));
    }

    #[test]
    fn directory_moved_out_of_the_tree_is_reported_removed_under_its_own_url() {
        let dir = create_test_dir();
        let root = dir.path();
        fs::create_dir(root.join("v1.2")).unwrap();
        fs::write(root.join("v1.2/a.md"), "# A").unwrap();
        let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
        fs::remove_dir_all(root.join("v1.2")).unwrap();

        let path = root.join("v1.2");
        assert!(is_relevant(&path, Path::new("v1.2"), &watch_patterns()));
        let debouncer = EventDebouncer::new(Duration::ZERO);
        debouncer.record(path, RawEventKind::Removed);
        let events: Vec<StorageEvent> = debouncer
            .drain_ready()
            .iter()
            .map(|event| to_storage_event(event, &storage.resolver, &storage.meta_cache))
            .collect();

        assert_eq!(
            events,
            [StorageEvent::Page {
                path: "v1.2".to_owned(),
                kind: StorageEventKind::Removed,
            }]
        );
    }

    /// Pins every precedence rule across scan, `read`, and the watch path, so
    /// a second encoding of "which file serves this url" cannot creep back in.
    #[test]
    fn scan_and_resolver_agree_across_the_precedence_matrix() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();
        let root = project.join("docs");

        // Every metadata form, each on its own url.
        fs::create_dir_all(root.join("canonical")).unwrap();
        fs::write(root.join("canonical/index.md"), "# Canonical").unwrap();
        fs::write(root.join("canonical/meta.yaml"), "description: canonical").unwrap();
        fs::write(root.join("canonical/index.meta.yaml"), "description: loser").unwrap();

        fs::create_dir_all(root.join("variant")).unwrap();
        fs::write(root.join("variant/index.md"), "# Variant").unwrap();
        fs::write(root.join("variant/index.meta.yaml"), "description: variant").unwrap();

        // Frontmatter and sidecar both contribute, so every field must agree.
        fs::write(
            root.join("sibling.md"),
            "---\nkind: system\nname: billing\nattrs:\n  owner: team\n---\n# Sibling\n",
        )
        .unwrap();
        fs::write(
            root.join("sibling.meta.yaml"),
            "description: sibling\nnamespace: payments\npages: [x, y]\nattrs:\n  tier: 1\n",
        )
        .unwrap();

        // Content precedence: index.md must beat the standalone of the same name.
        fs::create_dir_all(root.join("both")).unwrap();
        fs::write(root.join("both/index.md"), "# Both Index").unwrap();
        fs::write(root.join("both.md"), "# Both Standalone").unwrap();

        // The README homepage lives outside `docs/` but pairs with its sidecar.
        fs::write(project.join("README.md"), "# Readme").unwrap();
        fs::write(root.join("meta.yaml"), "description: root").unwrap();

        let storage = FsStorage::new(project.to_path_buf(), root);
        let (documents, _) = storage.scan().unwrap();
        let mut scanned: Vec<_> = documents
            .iter()
            .map(|d| {
                (
                    d.path.clone(),
                    d.meta.title.clone(),
                    d.meta.description.clone().unwrap_or_default(),
                )
            })
            .collect();
        scanned.sort();
        let expected = [
            ("", "Readme", "root"),
            ("both", "Both Index", ""),
            ("canonical", "Canonical", "canonical"),
            ("sibling", "Sibling", "sibling"),
            ("variant", "Variant", "variant"),
        ]
        .map(|(p, t, d)| (p.to_owned(), t.to_owned(), d.to_owned()));
        assert_eq!(scanned, expected);

        for document in &documents {
            let url_path = &document.path;
            let read = storage.read(url_path).unwrap();
            let StorageEventKind::Modified { meta } =
                modified_event_kind(&storage.resolver, &MetaCache::default(), url_path)
            else {
                panic!("expected Modified for {url_path:?}");
            };
            assert!(
                read.lines().any(|line| line == format!("# {}", meta.title)),
                "read and watch disagree on the content file for url {url_path:?}"
            );
            assert_eq!(
                *document.meta, meta,
                "scan and watch disagree on url {url_path:?}"
            );
        }
    }
}
