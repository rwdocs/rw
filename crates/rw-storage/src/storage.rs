//! Storage trait and error types.
//!
//! Provides the core [`Storage`] trait for abstracting document scanning and retrieval,
//! along with [`StorageError`] for unified error handling across backends.

use std::path::{Path, PathBuf};

/// Document metadata returned by storage scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Storage path (e.g., "guide.md", "domain/index.md").
    pub path: PathBuf,
    /// Document title (extracted or stored).
    pub title: String,
}

/// Semantic error categories (inspired by Object Store + `OpenDAL`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageErrorKind {
    /// Resource does not exist.
    NotFound,
    /// Permission denied.
    PermissionDenied,
    /// Resource already exists (for create operations).
    AlreadyExists,
    /// Invalid path or identifier.
    InvalidPath,
    /// Backend is temporarily unavailable.
    Unavailable,
    /// Too many requests.
    RateLimited,
    /// Operation timed out.
    Timeout,
    /// Other/unknown error category.
    Other,
}

/// Retry guidance (from `OpenDAL`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorStatus {
    /// Don't retry (config error, not found, invalid path).
    #[default]
    Permanent,
    /// Retry immediately (timeout, connection reset).
    Temporary,
    /// Retry with backoff (rate limited, service unavailable).
    Persistent,
}

/// Storage error with semantic kind and backend-specific source.
#[derive(Debug)]
pub struct StorageError {
    kind: StorageErrorKind,
    status: ErrorStatus,
    path: Option<PathBuf>,
    backend: Option<&'static str>,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl StorageError {
    /// Create a new storage error.
    #[must_use]
    pub fn new(kind: StorageErrorKind) -> Self {
        Self {
            kind,
            status: ErrorStatus::Permanent,
            path: None,
            backend: None,
            source: None,
        }
    }

    /// Attach path context.
    #[must_use]
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Attach backend identifier.
    #[must_use]
    pub fn with_backend(mut self, backend: &'static str) -> Self {
        self.backend = Some(backend);
        self
    }

    /// Set retry status.
    #[must_use]
    pub fn with_status(mut self, status: ErrorStatus) -> Self {
        self.status = status;
        self
    }

    /// Attach the underlying error source.
    #[must_use]
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Get the error kind for matching.
    #[must_use]
    pub fn kind(&self) -> StorageErrorKind {
        self.kind
    }

    /// Get the retry status.
    #[must_use]
    pub fn status(&self) -> ErrorStatus {
        self.status
    }

    /// Get the path if available.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Get the backend identifier if available.
    #[must_use]
    pub fn backend(&self) -> Option<&'static str> {
        self.backend
    }

    /// Downcast the source error to a concrete type.
    #[must_use]
    pub fn downcast_source<E: std::error::Error + 'static>(&self) -> Option<&E> {
        self.source.as_ref()?.downcast_ref()
    }

    /// Create a not found error with path.
    #[must_use]
    pub fn not_found(path: impl Into<PathBuf>) -> Self {
        Self::new(StorageErrorKind::NotFound).with_path(path)
    }

    /// Create a storage error from an I/O error.
    #[must_use]
    pub fn io(err: std::io::Error, path: Option<PathBuf>) -> Self {
        let kind = match err.kind() {
            std::io::ErrorKind::NotFound => StorageErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied => StorageErrorKind::PermissionDenied,
            std::io::ErrorKind::AlreadyExists => StorageErrorKind::AlreadyExists,
            std::io::ErrorKind::TimedOut => StorageErrorKind::Timeout,
            _ => StorageErrorKind::Other,
        };
        let status = match err.kind() {
            std::io::ErrorKind::TimedOut => ErrorStatus::Temporary,
            _ => ErrorStatus::Permanent,
        };
        let mut error = Self::new(kind).with_status(status).with_source(err);
        if let Some(p) = path {
            error = error.with_path(p);
        }
        error
    }
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format: "[Backend] Kind: message (path: /foo/bar)"
        if let Some(backend) = self.backend {
            write!(f, "[{backend}] ")?;
        }

        let kind_str = match self.kind {
            StorageErrorKind::NotFound => "Not found",
            StorageErrorKind::PermissionDenied => "Permission denied",
            StorageErrorKind::AlreadyExists => "Already exists",
            StorageErrorKind::InvalidPath => "Invalid path",
            StorageErrorKind::Unavailable => "Unavailable",
            StorageErrorKind::RateLimited => "Rate limited",
            StorageErrorKind::Timeout => "Timeout",
            StorageErrorKind::Other => "Error",
        };

        write!(f, "{kind_str}")?;

        if let Some(source) = &self.source {
            write!(f, ": {source}")?;
        }

        if let Some(path) = &self.path {
            write!(f, " (path: {})", path.display())?;
        }

        Ok(())
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Storage abstraction for document scanning and retrieval.
///
/// Provides a unified interface for accessing documents regardless of backend.
/// Implementations handle backend-specific details like caching, title extraction,
/// and path resolution.
pub trait Storage: Send + Sync {
    /// Scan and return all documents.
    ///
    /// Returns a flat list of documents. Hierarchy is derived by the consumer
    /// (`SiteLoader`) based on path conventions.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if scanning fails (e.g., permission denied,
    /// backend unavailable).
    fn scan(&self) -> Result<Vec<Document>, StorageError>;

    /// Read full content for rendering.
    ///
    /// # Arguments
    ///
    /// * `path` - Relative path within the storage (e.g., "guide.md")
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the document doesn't exist or can't be read.
    fn read(&self, path: &Path) -> Result<String, StorageError>;

    /// Check if a document exists at the given path.
    ///
    /// Used by `SiteLoader` to determine if `index.md` exists for hierarchy building.
    /// Returns `false` on errors (treats errors as "doesn't exist").
    fn exists(&self, path: &Path) -> bool;

    /// Get modification time as seconds since Unix epoch.
    ///
    /// # Arguments
    ///
    /// * `path` - Relative path within the storage (e.g., "guide.md")
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the document doesn't exist or mtime can't be retrieved.
    fn mtime(&self, path: &Path) -> Result<f64, StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_creation() {
        let doc = Document {
            path: PathBuf::from("guide.md"),
            title: "Guide".to_string(),
        };

        assert_eq!(doc.path, PathBuf::from("guide.md"));
        assert_eq!(doc.title, "Guide");
    }

    #[test]
    fn test_storage_error_kind_variants() {
        // Ensure all variants exist and can be compared
        assert_ne!(
            StorageErrorKind::NotFound,
            StorageErrorKind::PermissionDenied
        );
        assert_ne!(
            StorageErrorKind::AlreadyExists,
            StorageErrorKind::InvalidPath
        );
        assert_ne!(StorageErrorKind::Unavailable, StorageErrorKind::RateLimited);
        assert_ne!(StorageErrorKind::Timeout, StorageErrorKind::Other);
    }

    #[test]
    fn test_storage_error_new() {
        let err = StorageError::new(StorageErrorKind::NotFound);

        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.status(), ErrorStatus::Permanent);
        assert!(err.path().is_none());
        assert!(err.backend().is_none());
    }

    #[test]
    fn test_storage_error_with_path() {
        let err = StorageError::new(StorageErrorKind::NotFound).with_path("/foo/bar");

        assert_eq!(err.path(), Some(Path::new("/foo/bar")));
    }

    #[test]
    fn test_storage_error_with_backend() {
        let err = StorageError::new(StorageErrorKind::NotFound).with_backend("Fs");

        assert_eq!(err.backend(), Some("Fs"));
    }

    #[test]
    fn test_storage_error_with_status() {
        let err = StorageError::new(StorageErrorKind::Timeout).with_status(ErrorStatus::Temporary);

        assert_eq!(err.status(), ErrorStatus::Temporary);
    }

    #[test]
    fn test_storage_error_with_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = StorageError::new(StorageErrorKind::NotFound).with_source(io_err);

        assert!(err.downcast_source::<std::io::Error>().is_some());
    }

    #[test]
    fn test_storage_error_not_found() {
        let err = StorageError::not_found("/foo/bar");

        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.path(), Some(Path::new("/foo/bar")));
    }

    #[test]
    fn test_storage_error_io_not_found() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = StorageError::io(io_err, Some(PathBuf::from("/foo/bar")));

        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.status(), ErrorStatus::Permanent);
        assert_eq!(err.path(), Some(Path::new("/foo/bar")));
    }

    #[test]
    fn test_storage_error_io_permission_denied() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err = StorageError::io(io_err, None);

        assert_eq!(err.kind(), StorageErrorKind::PermissionDenied);
    }

    #[test]
    fn test_storage_error_io_timeout() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
        let err = StorageError::io(io_err, None);

        assert_eq!(err.kind(), StorageErrorKind::Timeout);
        assert_eq!(err.status(), ErrorStatus::Temporary);
    }

    #[test]
    fn test_storage_error_display_simple() {
        let err = StorageError::new(StorageErrorKind::NotFound);

        assert_eq!(err.to_string(), "Not found");
    }

    #[test]
    fn test_storage_error_display_with_backend() {
        let err = StorageError::new(StorageErrorKind::NotFound).with_backend("Fs");

        assert_eq!(err.to_string(), "[Fs] Not found");
    }

    #[test]
    fn test_storage_error_display_with_path() {
        let err = StorageError::new(StorageErrorKind::NotFound).with_path("/foo/bar");

        assert_eq!(err.to_string(), "Not found (path: /foo/bar)");
    }

    #[test]
    fn test_storage_error_display_full() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = StorageError::new(StorageErrorKind::NotFound)
            .with_backend("Fs")
            .with_path("/foo/bar")
            .with_source(io_err);

        assert_eq!(
            err.to_string(),
            "[Fs] Not found: file not found (path: /foo/bar)"
        );
    }

    #[test]
    fn test_storage_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StorageError>();
    }

    #[test]
    fn test_error_status_default() {
        let status = ErrorStatus::default();
        assert_eq!(status, ErrorStatus::Permanent);
    }
}
