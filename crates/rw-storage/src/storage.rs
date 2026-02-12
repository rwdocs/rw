//! Storage trait and error types.
//!
//! Provides the core [`Storage`] trait for abstracting document scanning and retrieval,
//! along with [`StorageError`] for unified error handling across backends.
//!
//! # URL Path Convention
//!
//! All path parameters in Storage methods are **URL paths**, not file paths:
//! - `""` - root (home page)
//! - `"guide"` - standalone page
//! - `"domain"` - directory with index
//! - `"domain/billing"` - nested page
//!
//! Storage implementations handle the mapping from URL paths to their internal storage format.

use std::path::PathBuf;

use crate::event::{StorageEventReceiver, WatchHandle};
use crate::metadata::Metadata;

/// Document metadata returned by storage scan.
///
/// Documents can be either real pages (with content) or virtual pages (metadata only).
/// Virtual pages are directories with metadata but no `index.md`.
///
/// # Path Convention
///
/// The `path` field contains URL paths, not file paths:
/// - `""` - root (maps to `index.md`)
/// - `"guide"` - standalone page (maps to `guide.md` or `guide/index.md`)
/// - `"domain"` - directory section (maps to `domain/index.md`)
/// - `"domain/billing"` - nested page
#[derive(Debug, PartialEq, Eq)]
pub struct Document {
    /// URL path (e.g., "", "guide", "domain", "domain/billing").
    pub path: String,
    /// Document title (resolved: metadata.title > H1 > filename).
    pub title: String,
    /// True if .md file exists.
    pub has_content: bool,
    /// Page type from metadata (e.g., "domain", "guide").
    /// Used for section detection. Not inherited.
    pub page_type: Option<String>,
    /// Page description from metadata.
    /// Not inherited.
    pub description: Option<String>,
}

/// Semantic error categories (inspired by Object Store + `OpenDAL`).
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq, Default)]
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
    /// Semantic error category.
    pub kind: StorageErrorKind,
    /// Retry guidance.
    pub status: ErrorStatus,
    /// Path context (if applicable).
    pub path: Option<PathBuf>,
    /// Backend identifier (e.g., "Fs", "Mock").
    pub backend: Option<&'static str>,
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
///
/// # URL Paths
///
/// All path parameters are **URL paths**, not file paths:
/// - `""` - root (home page)
/// - `"guide"` - standalone page
/// - `"domain"` - directory with index
/// - `"domain/billing"` - nested page
///
/// Storage implementations map URL paths to their internal storage format.
pub trait Storage: Send + Sync {
    /// Scan and return all documents.
    ///
    /// Returns documents with URL paths. Hierarchy is derived by the consumer
    /// (`Site`) based on path conventions.
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
    /// * `path` - URL path (e.g., "guide", "domain/billing", "" for root)
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the document doesn't exist or can't be read.
    fn read(&self, path: &str) -> Result<String, StorageError>;

    /// Check if a document exists at the given URL path.
    ///
    /// Returns `false` on errors (treats errors as "doesn't exist").
    fn exists(&self, path: &str) -> bool;

    /// Get modification time as seconds since Unix epoch.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path (e.g., "guide", "domain/billing", "" for root)
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the document doesn't exist or mtime can't be retrieved.
    fn mtime(&self, path: &str) -> Result<f64, StorageError>;

    /// Start watching for document changes.
    ///
    /// Returns a receiver for events and a handle to stop watching.
    /// Events contain URL paths, not file paths.
    /// Default implementation returns a no-op receiver for backends
    /// that don't support change notification.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if watching cannot be started (e.g., backend unavailable).
    fn watch(&self) -> Result<(StorageEventReceiver, WatchHandle), StorageError> {
        Ok((StorageEventReceiver::no_op(), WatchHandle::no_op()))
    }

    /// Read metadata for a URL path with inheritance applied.
    ///
    /// Returns full [`Metadata`] with vars merged from ancestors.
    /// Each backend handles its own format and inheritance strategy.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path (e.g., "domain/billing", "" for root)
    ///
    /// # Returns
    ///
    /// - `Ok(Some(metadata))` - Metadata exists and was parsed successfully
    /// - `Ok(None)` - No metadata file exists for this path or any ancestor
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] on I/O error or metadata parse error.
    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError>;
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn test_document_root() {
        let doc = Document {
            path: String::new(),
            title: "Home".to_owned(),
            has_content: true,
            page_type: None,
            description: None,
        };

        assert_eq!(doc.path, "");
        assert_eq!(doc.title, "Home");
        assert!(doc.has_content);
        assert!(doc.page_type.is_none());
    }

    #[test]
    fn test_document_standalone() {
        let doc = Document {
            path: "guide".to_owned(),
            title: "Guide".to_owned(),
            has_content: true,
            page_type: None,
            description: None,
        };

        assert_eq!(doc.path, "guide");
        assert_eq!(doc.title, "Guide");
        assert!(doc.has_content);
        assert!(doc.page_type.is_none());
    }

    #[test]
    fn test_document_nested_with_type() {
        let doc = Document {
            path: "domain/billing".to_owned(),
            title: "Billing".to_owned(),
            has_content: true,
            page_type: Some("domain".to_owned()),
            description: None,
        };

        assert_eq!(doc.path, "domain/billing");
        assert!(doc.has_content);
        assert_eq!(doc.page_type, Some("domain".to_owned()));
    }

    #[test]
    fn test_virtual_document() {
        let doc = Document {
            path: "domains".to_owned(),
            title: "Domains".to_owned(),
            has_content: false,
            page_type: Some("section".to_owned()),
            description: None,
        };

        assert_eq!(doc.path, "domains");
        assert!(!doc.has_content);
        assert_eq!(doc.page_type, Some("section".to_owned()));
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

        assert_eq!(err.kind, StorageErrorKind::NotFound);
        assert_eq!(err.status, ErrorStatus::Permanent);
        assert!(err.path.as_deref().is_none());
        assert!(err.backend.is_none());
    }

    #[test]
    fn test_storage_error_with_path() {
        let err = StorageError::new(StorageErrorKind::NotFound).with_path("/foo/bar");

        assert_eq!(err.path.as_deref(), Some(Path::new("/foo/bar")));
    }

    #[test]
    fn test_storage_error_with_backend() {
        let err = StorageError::new(StorageErrorKind::NotFound).with_backend("Fs");

        assert_eq!(err.backend, Some("Fs"));
    }

    #[test]
    fn test_storage_error_with_status() {
        let err = StorageError::new(StorageErrorKind::Timeout).with_status(ErrorStatus::Temporary);

        assert_eq!(err.status, ErrorStatus::Temporary);
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

        assert_eq!(err.kind, StorageErrorKind::NotFound);
        assert_eq!(err.path.as_deref(), Some(Path::new("/foo/bar")));
    }

    #[test]
    fn test_storage_error_io_not_found() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = StorageError::io(io_err, Some(PathBuf::from("/foo/bar")));

        assert_eq!(err.kind, StorageErrorKind::NotFound);
        assert_eq!(err.status, ErrorStatus::Permanent);
        assert_eq!(err.path.as_deref(), Some(Path::new("/foo/bar")));
    }

    #[test]
    fn test_storage_error_io_permission_denied() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err = StorageError::io(io_err, None);

        assert_eq!(err.kind, StorageErrorKind::PermissionDenied);
    }

    #[test]
    fn test_storage_error_io_timeout() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
        let err = StorageError::io(io_err, None);

        assert_eq!(err.kind, StorageErrorKind::Timeout);
        assert_eq!(err.status, ErrorStatus::Temporary);
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
