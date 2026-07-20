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

use chrono::{DateTime, Utc};

use crate::event::{StorageEventReceiver, WatchHandle};
use crate::metadata::Metadata;

/// Serde default for [`Document::is_dir`]: a bundle published before this
/// field existed has no leaf pages, so treating its pages as directories
/// preserves the link resolution they were rendered with.
fn default_is_dir() -> bool {
    true
}

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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Document {
    /// URL path (e.g., "", "guide", "domain", "domain/billing").
    pub path: String,
    /// Document title (resolved: metadata.title > H1 > filename).
    pub title: String,
    /// True if .md file exists.
    pub has_content: bool,
    /// Page kind from metadata (e.g., "domain", "guide").
    /// Used for section detection. Not inherited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_kind: Option<String>,
    /// Section namespace declared by this page's own metadata.
    /// Un-inherited (like `page_kind`); `rw-site` inherits it down the tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Page description from metadata.
    /// Not inherited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Source directory name for content originating outside `source_dir`.
    /// When set (e.g., `"docs"`), the renderer strips this prefix from
    /// relative links so they resolve correctly in URL space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Ordered list of child page slugs for navigation ordering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<String>>,
    /// True when this page's URL denotes a directory — its content comes from a
    /// directory index file (`index.md`, or the README homepage) — rather than a
    /// single file (a leaf `name.md`).
    ///
    /// Leaf pages resolve relative links against their parent directory, so the
    /// renderer drops the page's own URL slug from the link base. Defaults to
    /// `true` for backward compatibility with S3 bundles published before this
    /// field existed (preserving their directory-style resolution).
    #[serde(default = "default_is_dir")]
    pub is_dir: bool,
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

impl std::fmt::Display for StorageErrorKind {
    /// Human-readable label for the category. Mirrors [`std::io::ErrorKind`],
    /// which is `Display` but deliberately not an `Error` — this is a `Copy`
    /// category tag (used as a value, e.g. injected by the mock storage), not a
    /// failure value with a cause.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            StorageErrorKind::NotFound => "Not found",
            StorageErrorKind::PermissionDenied => "Permission denied",
            StorageErrorKind::AlreadyExists => "Already exists",
            StorageErrorKind::InvalidPath => "Invalid path",
            StorageErrorKind::Unavailable => "Unavailable",
            StorageErrorKind::RateLimited => "Rate limited",
            StorageErrorKind::Timeout => "Timeout",
            StorageErrorKind::Other => "Error",
        };
        f.write_str(label)
    }
}

/// Storage error with a semantic [`kind`](Self::kind) and a backend-specific
/// source.
///
/// Deliberately modeled on [`std::io::Error`]: an opaque struct carrying a
/// `Copy` category ([`StorageErrorKind`]), optional path/backend context, and a
/// type-erased `Box<dyn Error>` source. The `Display`/`Error` impls are
/// hand-written rather than derived via `thiserror` because the rendered form
/// is conditional on three independently-optional parts (the `[backend]`
/// prefix, the `: source`, and the ` (path: …)` suffix), which a static
/// `#[error("…")]` template cannot express. The crate's tagged-union errors
/// (e.g. [`MetadataError`](crate::MetadataError)) do use `thiserror`.
#[derive(Debug)]
pub struct StorageError {
    /// Semantic error category.
    pub kind: StorageErrorKind,
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

    /// Attach the underlying error source.
    #[must_use]
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Format the error with its full source chain.
    ///
    /// Unlike `Display` (which shows only the immediate source), this walks
    /// the entire `.source()` chain so wrapped errors like the AWS SDK's
    /// "dispatch failure" reveal the root cause.
    #[must_use]
    pub fn display_chain(&self) -> String {
        let mut msg = self.to_string();
        // Display already includes the immediate source, so start one level deeper.
        let mut next = self.source.as_deref().and_then(std::error::Error::source);
        while let Some(cause) = next {
            msg.push_str(": ");
            msg.push_str(&cause.to_string());
            next = cause.source();
        }
        msg
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
        let mut error = Self::new(kind).with_source(err);
        if let Some(p) = path {
            error = error.with_path(p);
        }
        error
    }
}

impl std::fmt::Display for StorageError {
    /// Renders `"[Backend] Kind: source (path: …)"`, delegating the kind label
    /// to [`StorageErrorKind`]'s `Display`. See [`StorageError`] for why this is
    /// hand-written rather than `thiserror`-derived.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(backend) = self.backend {
            write!(f, "[{backend}] ")?;
        }

        write!(f, "{}", self.kind)?;

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
    /// The returned value is unvalidated: a backend reads it from wherever it
    /// keeps it (a `stat` call, a git commit, a manifest written by another
    /// tool) and reports it as-is. Any guarantee about its sign or finiteness
    /// is that backend's own, not the trait's. Convert it with
    /// [`mtime_to_datetime`] rather than converting it by hand — both
    /// `Duration::from_secs_f64` and chrono's `From<SystemTime>` panic on
    /// values an untrusted backend can produce.
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

    /// Read metadata for a URL path.
    ///
    /// Returns only what the page's own metadata declares — nothing is
    /// inherited from ancestor paths. Each backend handles its own format.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path (e.g., "domain/billing", "" for root)
    ///
    /// # Returns
    ///
    /// - `Ok(Some(metadata))` - Metadata exists and was parsed successfully
    /// - `Ok(None)` - No metadata file exists for this path
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] on I/O error or metadata parse error.
    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError>;

    /// Check whether content has changed since the last successful scan.
    ///
    /// Returns `true` if content may have changed, `false` if definitely unchanged.
    /// Default returns `true` — safe for backends without change detection.
    /// Backends with efficient change detection (e.g., S3 `ETag`s) override this.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the check itself fails (e.g., network error).
    fn has_changed(&self) -> Result<bool, StorageError> {
        Ok(true)
    }
}

/// Convert an mtime in seconds since the Unix epoch into a [`DateTime<Utc>`].
///
/// Returns a `DateTime` rather than a `SystemTime` because that is what every
/// caller reports, and because the panic this exists to prevent is spread
/// across *both* steps of the conversion. `Duration::from_secs_f64` panics on
/// negative and non-finite input; chrono's `From<SystemTime>` then panics again
/// for anything outside its ±262143-year range — which a `SystemTime` holds
/// happily. Handing back a `SystemTime` would leave that second panic to the
/// caller, so the two steps stay together.
///
/// [`Storage::mtime`] values are unvalidated — an `S3Storage` manifest is
/// written by another tool and can be hand-edited or truncated — so this
/// tolerates anything an `f64` can hold.
///
/// A negative value is a real instant before the epoch and converts to one.
/// A value that denotes no representable instant — NaN, an infinity, or a
/// magnitude beyond chrono's range — becomes the Unix epoch, which is already
/// how an unknown mtime is reported. Note that a seconds/nanoseconds mix-up
/// (`1.75e18`, a nanosecond timestamp in a seconds field) lands here.
#[must_use]
pub fn mtime_to_datetime(mtime: f64) -> DateTime<Utc> {
    let seconds = mtime.floor();
    // Keeps the cast below in range; chrono rejects far smaller values anyway.
    if !seconds.is_finite() || seconds.abs() >= 9.0e18 {
        return DateTime::UNIX_EPOCH;
    }
    // The fraction is nominally in [0, 1), but `mtime - seconds` is not exact:
    // for a tiny negative mtime (|mtime| <= 2^-54, e.g. -1e-20) it rounds to
    // exactly 1.0, scaling to a full 1e9 nanoseconds. chrono reads that as the
    // leap-second slot and renders `:60`, a second that most date parsers
    // reject — so clamp it into the range `from_timestamp` expects.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "`seconds` is range-checked above; the fraction is non-negative and below 1"
    )]
    let (seconds, nanoseconds) = (
        seconds as i64,
        (((mtime - seconds) * 1e9) as u32).min(999_999_999),
    );

    DateTime::from_timestamp(seconds, nanoseconds).unwrap_or(DateTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// Magnitudes that denote no representable instant. The wrong-unit epoch
    /// timestamps are the realistic case: a foreign tool writing microseconds
    /// or nanoseconds into a seconds field. (Milliseconds still land inside
    /// chrono's range, so they convert to a real — if absurd — date.) NaN and the
    /// infinities cannot arrive from a manifest (see the note in
    /// `rw-storage-s3`'s `format.rs`) — they are here because `Storage` is a
    /// trait and an `f64` can hold them.
    const UNREPRESENTABLE_MTIMES: [f64; 8] = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e300,
        -1e300,
        1.75e15,
        1.75e18,
        -1.75e18,
    ];

    #[test]
    fn unrepresentable_mtimes_degrade_to_the_epoch() {
        for mtime in UNREPRESENTABLE_MTIMES {
            assert_eq!(
                mtime_to_datetime(mtime),
                DateTime::UNIX_EPOCH,
                "{mtime:e} should degrade to the epoch, not panic"
            );
        }
    }

    #[test]
    fn negative_mtime_is_a_real_instant_before_the_epoch() {
        let converted = mtime_to_datetime(-1.0);

        assert_eq!(
            converted.to_rfc3339(),
            "1969-12-31T23:59:59+00:00",
            "a negative mtime denotes a pre-epoch instant, not an unknown one"
        );
    }

    #[test]
    fn tiny_negative_mtime_does_not_render_a_leap_second() {
        // `-1e-20 - (-1.0)` rounds to exactly 1.0, which chrono reads as the
        // leap-second slot; `:60` is a second most date parsers reject.
        let converted = mtime_to_datetime(-1e-20);

        assert_eq!(
            converted.to_rfc3339(),
            "1969-12-31T23:59:59.999999999+00:00"
        );
    }

    #[test]
    fn ordinary_mtime_converts_to_its_instant() {
        // Expectation spelled out independently of the conversion under test.
        let converted = mtime_to_datetime(1_752_000_000.5);

        assert_eq!(converted.to_rfc3339(), "2025-07-08T18:40:00.500+00:00");
    }

    #[test]
    fn test_document_root() {
        let doc = Document {
            path: String::new(),
            title: "Home".to_owned(),
            has_content: true,
            page_kind: None,
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        };

        assert_eq!(doc.path, "");
        assert_eq!(doc.title, "Home");
        assert!(doc.has_content);
        assert!(doc.page_kind.is_none());
    }

    #[test]
    fn test_document_standalone() {
        let doc = Document {
            path: "guide".to_owned(),
            title: "Guide".to_owned(),
            has_content: true,
            page_kind: None,
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        };

        assert_eq!(doc.path, "guide");
        assert_eq!(doc.title, "Guide");
        assert!(doc.has_content);
        assert!(doc.page_kind.is_none());
    }

    #[test]
    fn test_document_nested_with_type() {
        let doc = Document {
            path: "domain/billing".to_owned(),
            title: "Billing".to_owned(),
            has_content: true,
            page_kind: Some("domain".to_owned()),
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        };

        assert_eq!(doc.path, "domain/billing");
        assert!(doc.has_content);
        assert_eq!(doc.page_kind, Some("domain".to_owned()));
    }

    #[test]
    fn test_virtual_document() {
        let doc = Document {
            path: "domains".to_owned(),
            title: "Domains".to_owned(),
            has_content: false,
            page_kind: Some("section".to_owned()),
            namespace: None,
            description: None,
            origin: None,
            pages: None,
            is_dir: true,
        };

        assert_eq!(doc.path, "domains");
        assert!(!doc.has_content);
        assert_eq!(doc.page_kind, Some("section".to_owned()));
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
    fn test_storage_error_with_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = StorageError::new(StorageErrorKind::NotFound).with_source(io_err);

        assert!(std::error::Error::source(&err).is_some());
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
    }

    #[test]
    fn test_storage_error_io_unmapped_kind_falls_back_to_other() {
        // Any io::ErrorKind without an explicit arm in io() maps to Other.
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let err = StorageError::io(io_err, None);

        assert_eq!(err.kind, StorageErrorKind::Other);
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
    fn test_storage_error_kind_display_labels() {
        // StorageErrorKind::Display is the single source of truth for the
        // user-visible error vocabulary (StorageError::Display delegates to it),
        // so lock every label.
        assert_eq!(StorageErrorKind::NotFound.to_string(), "Not found");
        assert_eq!(
            StorageErrorKind::PermissionDenied.to_string(),
            "Permission denied"
        );
        assert_eq!(
            StorageErrorKind::AlreadyExists.to_string(),
            "Already exists"
        );
        assert_eq!(StorageErrorKind::InvalidPath.to_string(), "Invalid path");
        assert_eq!(StorageErrorKind::Unavailable.to_string(), "Unavailable");
        assert_eq!(StorageErrorKind::RateLimited.to_string(), "Rate limited");
        assert_eq!(StorageErrorKind::Timeout.to_string(), "Timeout");
        assert_eq!(StorageErrorKind::Other.to_string(), "Error");
    }

    #[test]
    fn document_is_dir_defaults_true_when_absent() {
        // An S3 bundle published before `is_dir` existed has no such key.
        let json = r#"{"path":"guide","title":"Guide","has_content":true}"#;
        let doc: Document = serde_json::from_str(json).unwrap();
        assert!(doc.is_dir);
    }
}
