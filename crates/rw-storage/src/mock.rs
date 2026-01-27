//! Mock storage implementation for testing.
//!
//! Provides [`MockStorage`] for unit testing without filesystem access.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::storage::{Document, Storage, StorageError, StorageErrorKind};

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
#[derive(Debug, Default)]
pub struct MockStorage {
    documents: RwLock<Vec<Document>>,
    contents: RwLock<HashMap<PathBuf, String>>,
    mtimes: RwLock<HashMap<PathBuf, f64>>,
}

impl MockStorage {
    /// Create a new empty mock storage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a document with the given path and title.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn with_document(self, path: impl Into<PathBuf>, title: impl Into<String>) -> Self {
        self.documents.write().unwrap().push(Document {
            path: path.into(),
            title: title.into(),
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

    /// Add a document with both metadata and content.
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
        let path = path.into();
        self.documents.write().unwrap().push(Document {
            path: path.clone(),
            title: title.into(),
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
}

impl Storage for MockStorage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        Ok(self.documents.read().unwrap().clone())
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
            .with_document("guide.md", "Guide")
            .with_document("api.md", "API");

        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].path, PathBuf::from("guide.md"));
        assert_eq!(docs[0].title, "Guide");
        assert_eq!(docs[1].path, PathBuf::from("api.md"));
        assert_eq!(docs[1].title, "API");
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

        let docs = storage.scan().unwrap();
        let content = storage.read(Path::new("guide.md")).unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "User Guide");
        assert_eq!(content, "# User Guide\n\nContent.");
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
}
