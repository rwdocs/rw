//! Mock storage implementation for testing.
//!
//! Provides [`MockStorage`] for unit testing without filesystem access.
//! Metadata is returned exactly as configured, with no cascading or merging.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

use parking_lot::RwLock;

use crate::event::{StorageEvent, StorageEventKind, StorageEventReceiver, WatchHandle};
use crate::metadata::Metadata;
use crate::storage::{Document, Storage, StorageError, StorageErrorKind};

/// A one-shot/repeatable hook invoked inside a *successful* `scan()`.
///
/// Wrapped in a newtype with a manual `Debug` impl so `MockStorage` can keep
/// `#[derive(Debug)]` (a boxed closure is not `Debug`). The closure is
/// `Send + Sync` so `MockStorage` stays `Send + Sync`.
#[derive(Default)]
struct ScanHook(Option<Box<dyn FnMut() + Send + Sync>>);

impl std::fmt::Debug for ScanHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ScanHook")
            .field(&self.0.as_ref().map(|_| "<hook>"))
            .finish()
    }
}

/// Mock storage for testing.
///
/// Stores documents and content in memory. Use the builder methods
/// to configure the mock with test data.
///
/// Metadata is returned exactly as set via `with_metadata()` — no inheritance
/// or merging is applied.
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
#[derive(Debug)]
pub struct MockStorage {
    documents: RwLock<Vec<Document>>,
    /// Contents keyed by URL path.
    contents: RwLock<HashMap<String, String>>,
    /// Modification times keyed by URL path.
    mtimes: RwLock<HashMap<String, f64>>,
    /// Metadata keyed by URL path.
    metadata: RwLock<HashMap<String, Metadata>>,
    /// If set, `scan()` returns this error kind.
    scan_error: RwLock<Option<StorageErrorKind>>,
    /// If `true`, `scan()` panics instead of returning.
    scan_panic: AtomicBool,
    /// If set, overrides the default `has_changed()` return value.
    has_changed: RwLock<Option<Result<bool, StorageErrorKind>>>,
    event_sender: RwLock<Option<mpsc::Sender<StorageEvent>>>,
    /// Number of times `scan()` has been called (including failed calls).
    scan_count: AtomicUsize,
    /// Optional hook run inside a successful `scan()` (test injection point).
    scan_hook: RwLock<ScanHook>,
}

impl Default for MockStorage {
    fn default() -> Self {
        Self {
            documents: RwLock::new(Vec::new()),
            contents: RwLock::new(HashMap::new()),
            mtimes: RwLock::new(HashMap::new()),
            metadata: RwLock::new(HashMap::new()),
            scan_error: RwLock::new(None),
            scan_panic: AtomicBool::new(false),
            has_changed: RwLock::new(None),
            event_sender: RwLock::new(None),
            scan_count: AtomicUsize::new(0),
            scan_hook: RwLock::new(ScanHook::default()),
        }
    }
}

impl MockStorage {
    /// Create a new empty mock storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a document with the given URL path and title.
    ///
    /// The document has `has_content=true` and no `page_kind`.
    #[must_use]
    pub fn with_document(self, path: impl Into<String>, title: impl Into<String>) -> Self {
        self.documents.write().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: true,
            page_kind: None,
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        });
        self
    }

    /// Add a document with an ordered `pages` list.
    ///
    /// The document has `has_content=true` and the specified `pages` ordering.
    #[must_use]
    pub fn with_document_and_pages(
        self,
        path: impl Into<String>,
        title: impl Into<String>,
        pages: Vec<String>,
    ) -> Self {
        self.documents.write().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: true,
            page_kind: None,
            namespace: None,
            description: None,
            origin: None,
            pages: Some(pages),
            is_dir: true,
        });
        self
    }

    /// Add a document with a page kind (section).
    ///
    /// The document has `has_content=true` and the specified `page_kind`.
    #[must_use]
    pub fn with_document_and_kind(
        self,
        path: impl Into<String>,
        title: impl Into<String>,
        page_kind: impl Into<String>,
    ) -> Self {
        self.documents.write().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: true,
            page_kind: Some(page_kind.into()),
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        });
        self
    }

    /// Add a content document with an explicit `page_kind` and `namespace`.
    ///
    /// The document has `has_content = true`.
    #[must_use]
    pub fn with_document_kind_namespace(
        self,
        path: impl Into<String>,
        title: impl Into<String>,
        page_kind: impl Into<String>,
        namespace: impl Into<String>,
    ) -> Self {
        self.documents.write().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: true,
            page_kind: Some(page_kind.into()),
            namespace: Some(namespace.into()),
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        });
        self
    }

    /// Add a virtual page (no content, with optional kind).
    ///
    /// The document has `has_content=false`.
    #[must_use]
    pub fn with_virtual_page(self, path: impl Into<String>, title: impl Into<String>) -> Self {
        self.documents.write().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: false,
            page_kind: None,
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        });
        self
    }

    /// Add a virtual page with a page kind.
    #[must_use]
    pub fn with_virtual_page_and_kind(
        self,
        path: impl Into<String>,
        title: impl Into<String>,
        page_kind: impl Into<String>,
    ) -> Self {
        self.documents.write().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: false,
            page_kind: Some(page_kind.into()),
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        });
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
        self.documents.write().push(Document {
            path: path.clone(),
            title: title.into(),
            has_content: true,
            page_kind: None,
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        });
        self.contents.write().insert(path, content.into());
        self
    }

    /// Add metadata for a URL path.
    ///
    /// Metadata is returned exactly as set, with no inheritance applied.
    #[must_use]
    pub fn with_metadata(self, path: impl Into<String>, metadata: Metadata) -> Self {
        self.metadata.write().insert(path.into(), metadata);
        self
    }

    /// Set modification time for a URL path.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path
    /// * `mtime` - Modification time as seconds since Unix epoch
    #[must_use]
    pub fn with_mtime(self, path: impl Into<String>, mtime: f64) -> Self {
        self.mtimes.write().insert(path.into(), mtime);
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
        self.scan_hook.write().0 = hook;
    }

    /// Emit a storage event.
    ///
    /// Only works if `watch()` has been called first.
    pub fn emit(&self, event: StorageEvent) {
        if let Some(sender) = self.event_sender.read().as_ref() {
            let _ = sender.send(event);
        }
    }

    /// Emit a Created event.
    pub fn emit_created(&self, path: impl Into<String>) {
        self.emit(StorageEvent {
            path: path.into(),
            kind: StorageEventKind::Created,
        });
    }

    /// Emit a Modified event with the given title.
    pub fn emit_modified(&self, path: impl Into<String>, title: impl Into<String>) {
        self.emit(StorageEvent {
            path: path.into(),
            kind: StorageEventKind::Modified {
                title: title.into(),
                pages: None,
            },
        });
    }

    /// Emit a Removed event.
    pub fn emit_removed(&self, path: impl Into<String>) {
        self.emit(StorageEvent {
            path: path.into(),
            kind: StorageEventKind::Removed,
        });
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
        if let Some(hook) = self.scan_hook.write().0.as_mut() {
            hook();
        }
        let guard = self.documents.read();
        Ok(guard
            .iter()
            .map(|d| Document {
                path: d.path.clone(),
                title: d.title.clone(),
                has_content: d.has_content,
                page_kind: d.page_kind.clone(),
                namespace: d.namespace.clone(),
                description: d.description.clone(),
                origin: d.origin.clone(),
                pages: d.pages.clone(),
                is_dir: d.is_dir,
            })
            .collect())
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        self.contents.read().get(path).cloned().ok_or_else(|| {
            StorageError::new(StorageErrorKind::NotFound)
                .with_path(path)
                .with_backend("Mock")
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.contents.read().contains_key(path)
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        self.mtimes.read().get(path).copied().ok_or_else(|| {
            StorageError::new(StorageErrorKind::NotFound)
                .with_path(path)
                .with_backend("Mock")
        })
    }

    fn watch(&self) -> Result<(StorageEventReceiver, WatchHandle), StorageError> {
        // Create channel
        let (tx, rx) = mpsc::channel();

        // Store sender for emit methods
        *self.event_sender.write() = Some(tx);

        // Return receiver and no-op handle (MockStorage doesn't need cleanup)
        Ok((StorageEventReceiver::new(rx), WatchHandle::no_op()))
    }

    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError> {
        // Simple lookup, returning metadata exactly as configured
        Ok(self.metadata.read().get(path).map(|m| Metadata {
            title: m.title.clone(),
            description: m.description.clone(),
            page_kind: m.page_kind.clone(),
            pages: m.pages.clone(),
        }))
    }

    fn has_changed(&self) -> Result<bool, StorageError> {
        match *self.has_changed.read() {
            Some(Ok(value)) => Ok(value),
            Some(Err(kind)) => Err(StorageError::new(kind).with_backend("Mock")),
            None => Ok(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_mock_storage_is_send_sync() {
        assert_send_sync::<MockStorage>();
    }

    #[test]
    fn test_new_empty() {
        let storage = MockStorage::new();
        let docs = storage.scan().unwrap();

        assert!(docs.is_empty());
    }

    #[test]
    fn test_with_document() {
        let storage = MockStorage::new()
            .with_document("guide", "Guide")
            .with_document("api", "API");

        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].path, "guide");
        assert_eq!(docs[0].title, "Guide");
        assert!(docs[0].has_content);
        assert!(docs[0].page_kind.is_none());
        assert_eq!(docs[1].path, "api");
        assert_eq!(docs[1].title, "API");
    }

    #[test]
    fn test_with_content() {
        let storage = MockStorage::new().with_content("guide", "# Guide\n\nContent.");

        let content = storage.read("guide").unwrap();

        assert_eq!(content, "# Guide\n\nContent.");
    }

    #[test]
    fn test_with_file() {
        let storage =
            MockStorage::new().with_file("guide", "User Guide", "# User Guide\n\nContent.");

        let docs = storage.scan().unwrap();
        let content = storage.read("guide").unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "User Guide");
        assert!(docs[0].has_content);
        assert!(docs[0].page_kind.is_none());
        assert_eq!(content, "# User Guide\n\nContent.");
    }

    #[test]
    fn test_with_document_and_kind() {
        let storage = MockStorage::new().with_document_and_kind("domain", "Domain", "domain");

        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, "domain");
        assert!(docs[0].has_content);
        assert_eq!(docs[0].page_kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_with_virtual_page() {
        let storage = MockStorage::new().with_virtual_page("domain", "Domain Title");

        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.path, "domain");
        assert_eq!(doc.title, "Domain Title");
        assert!(!doc.has_content);
        assert!(doc.page_kind.is_none());
    }

    #[test]
    fn test_with_virtual_page_and_kind() {
        let storage =
            MockStorage::new().with_virtual_page_and_kind("domain", "Domain Title", "section");

        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert!(!doc.has_content);
        assert_eq!(doc.page_kind, Some("section".to_owned()));
    }

    #[test]
    fn test_read_missing() {
        use std::path::Path;

        let storage = MockStorage::new();

        let result = storage.read("missing");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::NotFound);
        assert_eq!(err.backend, Some("Mock"));
        assert_eq!(err.path.as_deref(), Some(Path::new("missing")));
    }

    #[test]
    fn test_meta_returns_stored_metadata() {
        let meta = Metadata {
            title: Some("Domain Title".to_owned()),
            page_kind: Some("domain".to_owned()),
            ..Default::default()
        };
        let storage = MockStorage::new().with_metadata("domain", meta);

        let result = storage.meta("domain").unwrap();

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.title, Some("Domain Title".to_owned()));
        assert_eq!(result.page_kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_meta_returns_none_when_no_metadata() {
        let storage = MockStorage::new();

        let result = storage.meta("").unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_meta_no_inheritance() {
        // MockStorage does NOT implement inheritance
        let root_meta = Metadata {
            title: Some("Root Title".to_owned()),
            ..Default::default()
        };
        let storage = MockStorage::new().with_metadata("", root_meta);

        // Child path has no metadata set - should return None
        let result = storage.meta("child").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_exists_true() {
        let storage = MockStorage::new().with_content("guide", "content");

        assert!(storage.exists("guide"));
    }

    #[test]
    fn test_exists_false() {
        let storage = MockStorage::new();

        assert!(!storage.exists("missing"));
    }

    #[test]
    fn test_with_mtime() {
        let storage = MockStorage::new().with_mtime("guide", 1_234_567_890.0);

        let mtime = storage.mtime("guide").unwrap();

        assert!((mtime - 1_234_567_890.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_mtime_missing() {
        use std::path::Path;

        let storage = MockStorage::new();

        let result = storage.mtime("missing");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::NotFound);
        assert_eq!(err.backend, Some("Mock"));
        assert_eq!(err.path.as_deref(), Some(Path::new("missing")));
    }

    #[test]
    fn test_watch_and_emit_created() {
        let storage = MockStorage::new();
        let (rx, _handle) = storage.watch().unwrap();

        storage.emit_created("new");

        let event = rx.try_recv();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.path, "new");
        assert_eq!(event.kind, StorageEventKind::Created);
    }

    #[test]
    fn test_watch_and_emit_modified() {
        let storage = MockStorage::new();
        let (rx, _handle) = storage.watch().unwrap();

        storage.emit_modified("guide", "Guide");

        let event = rx.try_recv();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.path, "guide");
        assert_matches!(event.kind, StorageEventKind::Modified { .. });
    }

    #[test]
    fn test_watch_and_emit_removed() {
        let storage = MockStorage::new();
        let (rx, _handle) = storage.watch().unwrap();

        storage.emit_removed("old");

        let event = rx.try_recv();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.path, "old");
        assert_eq!(event.kind, StorageEventKind::Removed);
    }

    #[test]
    fn test_watch_and_emit_multiple_events() {
        let storage = MockStorage::new();
        let (rx, _handle) = storage.watch().unwrap();

        storage.emit_created("a");
        storage.emit_modified("b", "B Title");
        storage.emit_removed("c");

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv()).collect();
        assert_eq!(events.len(), 3);

        assert_eq!(events[0].path, "a");
        assert_eq!(events[0].kind, StorageEventKind::Created);

        assert_eq!(events[1].path, "b");
        assert_matches!(events[1].kind, StorageEventKind::Modified { .. });

        assert_eq!(events[2].path, "c");
        assert_eq!(events[2].kind, StorageEventKind::Removed);
    }

    #[test]
    fn test_emit_before_watch_does_nothing() {
        let storage = MockStorage::new();

        // Emit before watch() is called should not panic
        storage.emit_created("test");
    }

    #[test]
    fn test_scan_error() {
        let storage = MockStorage::new().with_scan_error(StorageErrorKind::Unavailable);

        let result = storage.scan();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, StorageErrorKind::Unavailable);
        assert_eq!(err.backend, Some("Mock"));
    }

    #[test]
    fn test_scan_hook_fires_on_successful_scan() {
        use std::sync::Arc;

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
        use std::sync::Arc;

        let storage = MockStorage::new().with_scan_error(StorageErrorKind::Unavailable);
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_in_hook = Arc::clone(&calls);
        storage.set_scan_hook(Some(Box::new(move || {
            calls_in_hook.fetch_add(1, Ordering::Relaxed);
        })));

        assert!(storage.scan().is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_has_changed_default_returns_true() {
        let storage = MockStorage::new();

        assert!(storage.has_changed().unwrap());
    }

    #[test]
    fn test_document_kind_namespace() {
        let storage = MockStorage::new().with_document_kind_namespace(
            "domains/billing",
            "Billing",
            "domain",
            "payments",
        );
        let docs = storage.scan().unwrap();
        let doc = docs.iter().find(|d| d.path == "domains/billing").unwrap();
        assert_eq!(doc.page_kind.as_deref(), Some("domain"));
        assert_eq!(doc.namespace.as_deref(), Some("payments"));
    }
}
