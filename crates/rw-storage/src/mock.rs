//! Mock storage implementation for testing.
//!
//! Provides [`MockStorage`] for unit testing without filesystem access.
//! This implementation returns metadata exactly as set - no inheritance logic.

use std::collections::HashMap;
use std::sync::{RwLock, mpsc};

use crate::event::{StorageEvent, StorageEventKind, StorageEventReceiver, WatchHandle};
use crate::metadata::Metadata;
use crate::storage::{Document, Storage, StorageError, StorageErrorKind};

/// Mock storage for testing.
///
/// Stores documents and content in memory. Use the builder methods
/// to configure the mock with test data.
///
/// Unlike `FsStorage`, this implementation does NOT apply metadata inheritance.
/// Metadata is returned exactly as set via `with_metadata()`.
///
/// # Example
///
/// ```ignore
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
    event_sender: RwLock<Option<mpsc::Sender<StorageEvent>>>,
}

impl Default for MockStorage {
    fn default() -> Self {
        Self {
            documents: RwLock::new(Vec::new()),
            contents: RwLock::new(HashMap::new()),
            mtimes: RwLock::new(HashMap::new()),
            metadata: RwLock::new(HashMap::new()),
            event_sender: RwLock::new(None),
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
    /// The document has `has_content=true` and no `page_type`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_document(self, path: impl Into<String>, title: impl Into<String>) -> Self {
        self.documents.write().unwrap().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: true,
            page_type: None,
        });
        self
    }

    /// Add a document with a page type (section).
    ///
    /// The document has `has_content=true` and the specified `page_type`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_document_and_type(
        self,
        path: impl Into<String>,
        title: impl Into<String>,
        page_type: impl Into<String>,
    ) -> Self {
        self.documents.write().unwrap().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: true,
            page_type: Some(page_type.into()),
        });
        self
    }

    /// Add a virtual page (no content, with optional type).
    ///
    /// The document has `has_content=false`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_virtual_page(self, path: impl Into<String>, title: impl Into<String>) -> Self {
        self.documents.write().unwrap().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: false,
            page_type: None,
        });
        self
    }

    /// Add a virtual page with a page type.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_virtual_page_and_type(
        self,
        path: impl Into<String>,
        title: impl Into<String>,
        page_type: impl Into<String>,
    ) -> Self {
        self.documents.write().unwrap().push(Document {
            path: path.into(),
            title: title.into(),
            has_content: false,
            page_type: Some(page_type.into()),
        });
        self
    }

    /// Add content for a URL path.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_content(self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.contents
            .write()
            .unwrap()
            .insert(path.into(), content.into());
        self
    }

    /// Add a document with both document entry and content.
    ///
    /// The document has `has_content=true` and no `page_type`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_file(
        self,
        path: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let path: String = path.into();
        self.documents.write().unwrap().push(Document {
            path: path.clone(),
            title: title.into(),
            has_content: true,
            page_type: None,
        });
        self.contents.write().unwrap().insert(path, content.into());
        self
    }

    /// Add metadata for a URL path.
    ///
    /// Note: Unlike `FsStorage`, metadata is returned exactly as set.
    /// No inheritance logic is applied.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_metadata(self, path: impl Into<String>, metadata: Metadata) -> Self {
        self.metadata.write().unwrap().insert(path.into(), metadata);
        self
    }

    /// Set modification time for a URL path.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path
    /// * `mtime` - Modification time as seconds since Unix epoch
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_mtime(self, path: impl Into<String>, mtime: f64) -> Self {
        self.mtimes.write().unwrap().insert(path.into(), mtime);
        self
    }

    /// Emit a storage event.
    ///
    /// Only works if `watch()` has been called first.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn emit(&self, event: StorageEvent) {
        if let Some(sender) = self.event_sender.read().unwrap().as_ref() {
            let _ = sender.send(event);
        }
    }

    /// Emit a Created event.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn emit_created(&self, path: impl Into<String>) {
        self.emit(StorageEvent {
            path: path.into(),
            kind: StorageEventKind::Created,
        });
    }

    /// Emit a Modified event.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn emit_modified(&self, path: impl Into<String>) {
        self.emit(StorageEvent {
            path: path.into(),
            kind: StorageEventKind::Modified,
        });
    }

    /// Emit a Removed event.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn emit_removed(&self, path: impl Into<String>) {
        self.emit(StorageEvent {
            path: path.into(),
            kind: StorageEventKind::Removed,
        });
    }
}

impl Storage for MockStorage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        let guard = self.documents.read().unwrap();
        Ok(guard
            .iter()
            .map(|d| Document {
                path: d.path.clone(),
                title: d.title.clone(),
                has_content: d.has_content,
                page_type: d.page_type.clone(),
            })
            .collect())
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        self.contents
            .read()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| {
                StorageError::new(StorageErrorKind::NotFound)
                    .with_path(path)
                    .with_backend("Mock")
            })
    }

    fn exists(&self, path: &str) -> bool {
        self.contents.read().unwrap().contains_key(path)
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        self.mtimes
            .read()
            .unwrap()
            .get(path)
            .copied()
            .ok_or_else(|| {
                StorageError::new(StorageErrorKind::NotFound)
                    .with_path(path)
                    .with_backend("Mock")
            })
    }

    fn watch(&self) -> Result<(StorageEventReceiver, WatchHandle), StorageError> {
        // Create channel
        let (tx, rx) = mpsc::channel();

        // Store sender for emit methods
        *self.event_sender.write().unwrap() = Some(tx);

        // Return receiver and no-op handle (MockStorage doesn't need cleanup)
        Ok((StorageEventReceiver::new(rx), WatchHandle::no_op()))
    }

    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError> {
        // Simple lookup - no inheritance
        Ok(self.metadata.read().unwrap().get(path).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(docs[0].page_type.is_none());
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
        assert!(docs[0].page_type.is_none());
        assert_eq!(content, "# User Guide\n\nContent.");
    }

    #[test]
    fn test_with_document_and_type() {
        let storage = MockStorage::new().with_document_and_type("domain", "Domain", "domain");

        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, "domain");
        assert!(docs[0].has_content);
        assert_eq!(docs[0].page_type, Some("domain".to_owned()));
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
        assert!(doc.page_type.is_none());
    }

    #[test]
    fn test_with_virtual_page_and_type() {
        let storage =
            MockStorage::new().with_virtual_page_and_type("domain", "Domain Title", "section");

        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert!(!doc.has_content);
        assert_eq!(doc.page_type, Some("section".to_owned()));
    }

    #[test]
    fn test_read_missing() {
        use std::path::Path;

        let storage = MockStorage::new();

        let result = storage.read("missing");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.backend(), Some("Mock"));
        assert_eq!(err.path(), Some(Path::new("missing")));
    }

    #[test]
    fn test_meta_returns_stored_metadata() {
        let meta = Metadata {
            title: Some("Domain Title".to_owned()),
            page_type: Some("domain".to_owned()),
            ..Default::default()
        };
        let storage = MockStorage::new().with_metadata("domain", meta);

        let result = storage.meta("domain").unwrap();

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.title, Some("Domain Title".to_owned()));
        assert_eq!(result.page_type, Some("domain".to_owned()));
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
            vars: [("org".to_owned(), serde_json::json!("acme"))]
                .into_iter()
                .collect(),
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
        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.backend(), Some("Mock"));
        assert_eq!(err.path(), Some(Path::new("missing")));
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

        storage.emit_modified("guide");

        let event = rx.try_recv();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.path, "guide");
        assert_eq!(event.kind, StorageEventKind::Modified);
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
        storage.emit_modified("b");
        storage.emit_removed("c");

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv()).collect();
        assert_eq!(events.len(), 3);

        assert_eq!(events[0].path, "a");
        assert_eq!(events[0].kind, StorageEventKind::Created);

        assert_eq!(events[1].path, "b");
        assert_eq!(events[1].kind, StorageEventKind::Modified);

        assert_eq!(events[2].path, "c");
        assert_eq!(events[2].kind, StorageEventKind::Removed);
    }

    #[test]
    fn test_emit_before_watch_does_nothing() {
        let storage = MockStorage::new();

        // Emit before watch() is called should not panic
        storage.emit_created("test");
    }
}
