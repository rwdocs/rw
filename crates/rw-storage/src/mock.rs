//! Mock storage implementation for testing.
//!
//! Provides [`MockStorage`] for unit testing without filesystem access.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use parking_lot::RwLock;
use rw_meta::Meta;

use crate::file::file_content_type;
use crate::storage::{Document, Storage, StorageError, StorageErrorKind};

fn titled(title: impl Into<String>) -> Arc<Meta> {
    Arc::new(Meta {
        attrs: BTreeMap::new(),
        name: None,
        title: title.into(),
        description: None,
        kind: None,
        namespace: None,
        pages: None,
    })
}

/// Panics on any diagnostic so a typo in a test fixture cannot silently drop
/// a field.
fn from_yaml(path: &str, meta_yaml: &str) -> Arc<Meta> {
    let fallback = path.rsplit('/').next().unwrap_or(path);
    let resolved = Meta::resolve_with_diagnostics(None, Some(meta_yaml), fallback);
    assert!(
        resolved.diagnostics.is_empty(),
        "invalid mock metadata for {path:?}: {:?}",
        resolved.diagnostics
    );
    Arc::new(resolved.meta)
}

fn document(path: impl Into<String>, has_content: bool, meta: Arc<Meta>) -> Document {
    Document {
        path: path.into(),
        has_content,
        meta,
        origin: None,
        is_dir: true,
        readme: false,
        diagnostics: Vec::new().into(),
    }
}

/// Mock storage for testing.
///
/// Stores documents and content in memory. Use the builder methods
/// to configure the mock with test data. Documents are returned exactly as
/// configured; the mock does not apply metadata inheritance or merging.
///
/// # Example
///
/// ```
/// use rw_storage::{MockStorage, Storage};
///
/// let storage = MockStorage::new()
///     .with_document("guide", "User Guide")
///     .with_content("guide", "# User Guide\n\nContent.");
///
/// let docs = storage.scan().unwrap();
/// let content = storage.read("guide").unwrap();
/// ```
pub struct MockStorage {
    documents: RwLock<Vec<Document>>,
    /// Contents keyed by URL path.
    contents: RwLock<HashMap<String, String>>,
    /// Modification times keyed by URL path.
    mtimes: RwLock<HashMap<String, f64>>,
    /// Project files keyed by file path.
    files: HashMap<String, Vec<u8>>,
    /// If set, `scan()` returns this error kind.
    scan_error: RwLock<Option<StorageErrorKind>>,
    /// If `true`, `scan()` panics instead of returning.
    scan_panic: AtomicBool,
    /// If set, overrides the default `has_changed()` return value.
    has_changed: RwLock<Option<Result<bool, StorageErrorKind>>>,
    /// Number of times `scan()` has been called (including failed calls).
    scan_count: AtomicUsize,
    /// Optional hook run inside a successful `scan()` (test injection point).
    scan_hook: RwLock<Option<Box<dyn FnMut() + Send + Sync>>>,
    source_dir_in_project: Option<String>,
}

impl Default for MockStorage {
    fn default() -> Self {
        Self {
            documents: RwLock::new(Vec::new()),
            contents: RwLock::new(HashMap::new()),
            mtimes: RwLock::new(HashMap::new()),
            files: HashMap::new(),
            scan_error: RwLock::new(None),
            scan_panic: AtomicBool::new(false),
            has_changed: RwLock::new(None),
            scan_count: AtomicUsize::new(0),
            scan_hook: RwLock::new(None),
            source_dir_in_project: None,
        }
    }
}

impl MockStorage {
    /// Create a new empty mock storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a [`Document`] that [`Storage::scan`] returns verbatim.
    #[must_use]
    pub fn with_scanned_document(self, document: Document) -> Self {
        self.documents.write().push(document);
        self
    }

    /// Add a document with the given URL path and title.
    ///
    /// The document has `has_content=true` and no `page_kind`.
    #[must_use]
    pub fn with_document(self, path: impl Into<String>, title: impl Into<String>) -> Self {
        self.documents
            .write()
            .push(document(path, true, titled(title)));
        self
    }

    /// Add a document whose metadata is resolved from `meta_yaml`, as from a
    /// sidecar `meta.yaml`.
    ///
    /// The document has `has_content=true`.
    ///
    /// # Panics
    ///
    /// If `meta_yaml` produces any metadata diagnostic.
    #[must_use]
    pub fn with_document_meta(self, path: &str, meta_yaml: &str) -> Self {
        self.documents
            .write()
            .push(document(path, true, from_yaml(path, meta_yaml)));
        self
    }

    /// Add a virtual page (no content) whose metadata is resolved from
    /// `meta_yaml`.
    ///
    /// # Panics
    ///
    /// If `meta_yaml` produces any metadata diagnostic.
    #[must_use]
    pub fn with_virtual_page(self, path: &str, meta_yaml: &str) -> Self {
        self.documents
            .write()
            .push(document(path, false, from_yaml(path, meta_yaml)));
        self
    }

    /// Add content for a URL path.
    #[must_use]
    pub fn with_content(self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.contents.write().insert(path.into(), content.into());
        self
    }

    /// Add a document with both document entry and content.
    ///
    /// The document has `has_content=true` and no `page_kind`.
    #[must_use]
    pub fn with_file(
        self,
        path: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let path: String = path.into();
        self.documents
            .write()
            .push(document(path.clone(), true, titled(title)));
        self.contents.write().insert(path, content.into());
        self
    }

    /// Add a project file for [`Storage::read_file`], which still refuses it
    /// unless its extension is allowlisted.
    #[must_use]
    pub fn with_project_file(mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.insert(path.into(), bytes.into());
        self
    }

    /// Set modification time (seconds since the Unix epoch) for a URL path.
    #[must_use]
    pub fn with_mtime(self, path: impl Into<String>, mtime: f64) -> Self {
        self.mtimes.write().insert(path.into(), mtime);
        self
    }

    /// Set what [`Storage::source_dir_in_project`] returns (default `None`).
    #[must_use]
    pub fn with_source_dir_in_project(mut self, dir: impl Into<String>) -> Self {
        self.source_dir_in_project = Some(dir.into());
        self
    }

    /// Configure `scan()` to return an error with the given kind.
    #[must_use]
    pub fn with_scan_error(self, kind: StorageErrorKind) -> Self {
        *self.scan_error.write() = Some(kind);
        self
    }

    /// Set or clear the scan error at runtime (for testing reload-with-error scenarios).
    pub fn set_scan_error(&self, kind: Option<StorageErrorKind>) {
        *self.scan_error.write() = kind;
    }

    /// Configure `scan()` to panic instead of returning.
    ///
    /// Used by regression tests that simulate a backend panicking while
    /// `Site` holds `reload_lock`. `Release`/`Acquire` are used (instead
    /// of `Relaxed`) so cross-thread tests that set the flag from one
    /// thread and observe it from a reload worker on another thread get
    /// a happens-before guarantee.
    pub fn set_scan_panic(&self, panic: bool) {
        self.scan_panic.store(panic, Ordering::Release);
    }

    /// Number of times `scan()` has been called, including failed calls.
    ///
    /// Useful for verifying that callers do not re-scan during a backend outage.
    /// The counter has no associated state to synchronize, so `Relaxed` is
    /// sufficient — callers reading the counter must already happen-after the
    /// scans they want to observe through some other synchronization.
    pub fn scan_count(&self) -> usize {
        self.scan_count.load(Ordering::Relaxed)
    }

    /// Override `has_changed()` to return a fixed value or error.
    ///
    /// - `Some(Ok(true))` — report changed
    /// - `Some(Ok(false))` — report unchanged
    /// - `Some(Err(kind))` — return an error
    /// - `None` — use the default (always `true`)
    pub fn set_has_changed(&self, value: Option<Result<bool, StorageErrorKind>>) {
        *self.has_changed.write() = value;
    }

    /// Install (or clear with `None`) a hook invoked inside a successful
    /// `scan()`, after the panic/error checks pass and before documents are
    /// read. Used to simulate an `invalidate()` racing an in-flight scan.
    ///
    /// The hook must not call back into this `MockStorage` — the `scan_hook`
    /// write lock is held for the duration of the call (`parking_lot::RwLock`
    /// is not reentrant), so re-entering would deadlock.
    pub fn set_scan_hook(&self, hook: Option<Box<dyn FnMut() + Send + Sync>>) {
        *self.scan_hook.write() = hook;
    }
}

impl Storage for MockStorage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        self.scan_count.fetch_add(1, Ordering::Relaxed);
        assert!(
            !self.scan_panic.load(Ordering::Acquire),
            "MockStorage::scan: induced panic"
        );
        if let Some(kind) = self.scan_error.read().as_ref() {
            return Err(StorageError::new(*kind).with_backend("Mock"));
        }
        if let Some(hook) = self.scan_hook.write().as_mut() {
            hook();
        }
        Ok(self.documents.read().clone())
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        self.contents.read().get(path).cloned().ok_or_else(|| {
            StorageError::new(StorageErrorKind::NotFound)
                .with_path(path)
                .with_backend("Mock")
        })
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        file_content_type(path)
            .and_then(|_| self.files.get(path).cloned())
            .ok_or_else(|| StorageError::not_found(path).with_backend("Mock"))
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        self.mtimes.read().get(path).copied().ok_or_else(|| {
            StorageError::new(StorageErrorKind::NotFound)
                .with_path(path)
                .with_backend("Mock")
        })
    }

    fn has_changed(&self) -> Result<bool, StorageError> {
        match *self.has_changed.read() {
            Some(Ok(value)) => Ok(value),
            Some(Err(kind)) => Err(StorageError::new(kind).with_backend("Mock")),
            None => Ok(true),
        }
    }

    fn source_dir_in_project(&self) -> Option<String> {
        self.source_dir_in_project.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "invalid mock metadata")]
    fn test_with_document_meta_rejects_invalid_yaml() {
        let _ = MockStorage::new().with_document_meta("guide", "title: [unclosed");
    }

    #[test]
    fn read_file_serves_allowlisted_files_only() {
        let storage = MockStorage::new()
            .with_project_file("docs/img/flow.PNG", b"png".as_slice())
            .with_project_file(".env", b"SECRET=1".as_slice());

        assert_eq!(storage.read_file("docs/img/flow.PNG").unwrap(), b"png");
        for path in [".env", "docs/img/missing.png"] {
            let err = storage.read_file(path).unwrap_err();
            assert_eq!(err.kind, StorageErrorKind::NotFound, "{path}");
        }
    }

    #[test]
    fn test_scan_hook_fires_on_successful_scan() {
        let storage = MockStorage::new().with_document("guide", "Guide");
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_in_hook = Arc::clone(&calls);
        storage.set_scan_hook(Some(Box::new(move || {
            calls_in_hook.fetch_add(1, Ordering::Relaxed);
        })));

        storage.scan().unwrap();
        storage.scan().unwrap();

        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_scan_hook_not_fired_on_error() {
        let storage = MockStorage::new().with_scan_error(StorageErrorKind::Unavailable);
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_in_hook = Arc::clone(&calls);
        storage.set_scan_hook(Some(Box::new(move || {
            calls_in_hook.fetch_add(1, Ordering::Relaxed);
        })));

        assert!(storage.scan().is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }
}
