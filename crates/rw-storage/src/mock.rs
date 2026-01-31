//! Mock storage implementation for testing.
//!
//! Provides [`MockStorage`] for unit testing without filesystem access.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{RwLock, mpsc};

use crate::event::{StorageEvent, StorageEventKind, StorageEventReceiver, WatchHandle};
use crate::storage::{
    Document, ScanResult, Storage, StorageError, StorageErrorKind, meta_path_for_document,
};

/// Extract directory and filename from a path.
fn split_path(path: &Path) -> (PathBuf, String) {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    (dir, name)
}

/// Mock storage for testing.
///
/// Stores documents and content in memory. Use the builder methods
/// to configure the mock with test data.
///
/// # Example
///
/// ```ignore
/// use std::path::PathBuf;
/// use rw_storage::{MockStorage, Storage};
///
/// let storage = MockStorage::new()
///     .with_document("guide.md", "User Guide")
///     .with_content("guide.md", "# User Guide\n\nContent.");
///
/// let docs = storage.scan().unwrap();
/// let content = storage.read(Path::new("guide.md")).unwrap();
/// ```
#[derive(Debug)]
pub struct MockStorage {
    documents: RwLock<Vec<Document>>,
    contents: RwLock<HashMap<PathBuf, String>>,
    mtimes: RwLock<HashMap<PathBuf, f64>>,
    event_sender: RwLock<Option<mpsc::Sender<StorageEvent>>>,
}

impl Default for MockStorage {
    fn default() -> Self {
        Self {
            documents: RwLock::new(Vec::new()),
            contents: RwLock::new(HashMap::new()),
            mtimes: RwLock::new(HashMap::new()),
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

    /// Add a document with the given path and title.
    ///
    /// The path is split into directory and filename components.
    /// The document has `has_content=true` and `has_metadata=false`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_document(self, path: impl Into<PathBuf>, title: impl Into<String>) -> Self {
        let path: PathBuf = path.into();
        let (dir, name) = split_path(&path);
        self.documents.write().unwrap().push(Document {
            dir,
            name,
            title: title.into(),
            has_content: true,
            has_metadata: false,
        });
        self
    }

    /// Add a document with metadata.
    ///
    /// The path is split into directory and filename components.
    /// The document has `has_content=true` and `has_metadata=true`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_document_and_metadata(
        self,
        path: impl Into<PathBuf>,
        title: impl Into<String>,
    ) -> Self {
        let path: PathBuf = path.into();
        let (dir, name) = split_path(&path);
        self.documents.write().unwrap().push(Document {
            dir,
            name,
            title: title.into(),
            has_content: true,
            has_metadata: true,
        });
        self
    }

    /// Add a virtual page (metadata only, no content).
    ///
    /// Virtual pages always have `name: "index.md"`.
    /// The document has `has_content=false` and `has_metadata=true`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_virtual_page(self, dir: impl Into<PathBuf>, title: impl Into<String>) -> Self {
        self.documents.write().unwrap().push(Document {
            dir: dir.into(),
            name: "index.md".to_string(),
            title: title.into(),
            has_content: false,
            has_metadata: true,
        });
        self
    }

    /// Add content for a path.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_content(self, path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        self.contents
            .write()
            .unwrap()
            .insert(path.into(), content.into());
        self
    }

    /// Add a document with both document entry and content.
    ///
    /// The path is split into directory and filename components.
    /// The document has `has_content=true` and `has_metadata=false`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_file(
        self,
        path: impl Into<PathBuf>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let path: PathBuf = path.into();
        let (dir, name) = split_path(&path);
        self.documents.write().unwrap().push(Document {
            dir,
            name,
            title: title.into(),
            has_content: true,
            has_metadata: false,
        });
        self.contents.write().unwrap().insert(path, content.into());
        self
    }

    /// Set modification time for a path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file
    /// * `mtime` - Modification time as seconds since Unix epoch
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_mtime(self, path: impl Into<PathBuf>, mtime: f64) -> Self {
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
    pub fn emit_created(&self, path: impl Into<PathBuf>) {
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
    pub fn emit_modified(&self, path: impl Into<PathBuf>) {
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
    pub fn emit_removed(&self, path: impl Into<PathBuf>) {
        self.emit(StorageEvent {
            path: path.into(),
            kind: StorageEventKind::Removed,
        });
    }
}

impl Storage for MockStorage {
    fn scan(&self) -> Result<ScanResult, StorageError> {
        Ok(ScanResult {
            documents: self.documents.read().unwrap().clone(),
        })
    }

    fn read(&self, path: &Path) -> Result<String, StorageError> {
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

    fn exists(&self, path: &Path) -> bool {
        self.contents.read().unwrap().contains_key(path)
    }

    fn mtime(&self, path: &Path) -> Result<f64, StorageError> {
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

    fn meta(&self, path: &Path) -> Result<String, StorageError> {
        let meta_path = meta_path_for_document(path, "meta.yaml");
        self.read(&meta_path)
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
        let result = storage.scan().unwrap();

        assert!(result.documents.is_empty());
    }

    #[test]
    fn test_with_document() {
        let storage = MockStorage::new()
            .with_document("guide.md", "Guide")
            .with_document("api.md", "API");

        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 2);
        assert_eq!(result.documents[0].path(), PathBuf::from("guide.md"));
        assert_eq!(result.documents[0].title, "Guide");
        assert!(result.documents[0].has_content);
        assert!(!result.documents[0].has_metadata);
        assert_eq!(result.documents[1].path(), PathBuf::from("api.md"));
        assert_eq!(result.documents[1].title, "API");
    }

    #[test]
    fn test_with_content() {
        let storage = MockStorage::new().with_content("guide.md", "# Guide\n\nContent.");

        let content = storage.read(Path::new("guide.md")).unwrap();

        assert_eq!(content, "# Guide\n\nContent.");
    }

    #[test]
    fn test_with_file() {
        let storage =
            MockStorage::new().with_file("guide.md", "User Guide", "# User Guide\n\nContent.");

        let result = storage.scan().unwrap();
        let content = storage.read(Path::new("guide.md")).unwrap();

        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0].title, "User Guide");
        assert!(result.documents[0].has_content);
        assert!(!result.documents[0].has_metadata);
        assert_eq!(content, "# User Guide\n\nContent.");
    }

    #[test]
    fn test_with_document_and_metadata() {
        let storage = MockStorage::new()
            .with_document_and_metadata("domain/index.md", "Domain")
            .with_content("domain/meta.yaml", "title: Domain");

        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0].path(), PathBuf::from("domain/index.md"));
        assert!(result.documents[0].has_content);
        assert!(result.documents[0].has_metadata);
    }

    #[test]
    fn test_with_virtual_page() {
        let storage = MockStorage::new()
            .with_virtual_page("domain", "Domain Title")
            .with_content("domain/meta.yaml", "title: Domain Title");

        let result = storage.scan().unwrap();

        assert_eq!(result.documents.len(), 1);
        let doc = &result.documents[0];
        assert_eq!(doc.path(), PathBuf::from("domain/index.md"));
        assert_eq!(doc.title, "Domain Title");
        assert!(!doc.has_content);
        assert!(doc.has_metadata);
    }

    #[test]
    fn test_read_missing() {
        let storage = MockStorage::new();

        let result = storage.read(Path::new("missing.md"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.backend(), Some("Mock"));
        assert_eq!(err.path(), Some(Path::new("missing.md")));
    }

    #[test]
    fn test_meta_for_index() {
        let storage = MockStorage::new().with_content("domain/meta.yaml", "title: Domain Title");

        let content = storage.meta(Path::new("domain/index.md")).unwrap();

        assert_eq!(content, "title: Domain Title");
    }

    #[test]
    fn test_meta_for_root_index() {
        let storage = MockStorage::new().with_content("meta.yaml", "title: Home");

        let content = storage.meta(Path::new("index.md")).unwrap();

        assert_eq!(content, "title: Home");
    }

    #[test]
    fn test_meta_not_found() {
        let storage = MockStorage::new();

        let result = storage.meta(Path::new("index.md"));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), StorageErrorKind::NotFound);
    }

    #[test]
    fn test_exists_true() {
        let storage = MockStorage::new().with_content("guide.md", "content");

        assert!(storage.exists(Path::new("guide.md")));
    }

    #[test]
    fn test_exists_false() {
        let storage = MockStorage::new();

        assert!(!storage.exists(Path::new("missing.md")));
    }

    #[test]
    fn test_with_mtime() {
        let storage = MockStorage::new().with_mtime("guide.md", 1_234_567_890.0);

        let mtime = storage.mtime(Path::new("guide.md")).unwrap();

        assert!((mtime - 1_234_567_890.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_mtime_missing() {
        let storage = MockStorage::new();

        let result = storage.mtime(Path::new("missing.md"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.backend(), Some("Mock"));
        assert_eq!(err.path(), Some(Path::new("missing.md")));
    }

    #[test]
    fn test_watch_and_emit_created() {
        let storage = MockStorage::new();
        let (rx, _handle) = storage.watch().unwrap();

        storage.emit_created("new.md");

        let event = rx.try_recv();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.path, PathBuf::from("new.md"));
        assert_eq!(event.kind, StorageEventKind::Created);
    }

    #[test]
    fn test_watch_and_emit_modified() {
        let storage = MockStorage::new();
        let (rx, _handle) = storage.watch().unwrap();

        storage.emit_modified("guide.md");

        let event = rx.try_recv();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.path, PathBuf::from("guide.md"));
        assert_eq!(event.kind, StorageEventKind::Modified);
    }

    #[test]
    fn test_watch_and_emit_removed() {
        let storage = MockStorage::new();
        let (rx, _handle) = storage.watch().unwrap();

        storage.emit_removed("old.md");

        let event = rx.try_recv();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.path, PathBuf::from("old.md"));
        assert_eq!(event.kind, StorageEventKind::Removed);
    }

    #[test]
    fn test_watch_and_emit_multiple_events() {
        let storage = MockStorage::new();
        let (rx, _handle) = storage.watch().unwrap();

        storage.emit_created("a.md");
        storage.emit_modified("b.md");
        storage.emit_removed("c.md");

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv()).collect();
        assert_eq!(events.len(), 3);

        assert_eq!(events[0].path, PathBuf::from("a.md"));
        assert_eq!(events[0].kind, StorageEventKind::Created);

        assert_eq!(events[1].path, PathBuf::from("b.md"));
        assert_eq!(events[1].kind, StorageEventKind::Modified);

        assert_eq!(events[2].path, PathBuf::from("c.md"));
        assert_eq!(events[2].kind, StorageEventKind::Removed);
    }

    #[test]
    fn test_emit_before_watch_does_nothing() {
        let storage = MockStorage::new();

        // Emit before watch() is called should not panic
        storage.emit_created("test.md");
    }
}
