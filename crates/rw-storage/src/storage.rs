//! Storage trait and error types.
//!
//! Provides the core [`Storage`] trait for abstracting document scanning and retrieval,
//! along with [`StorageError`] for unified error handling across backends.

use std::collections::BTreeMap;
use std::error::Error;
use std::ops::Not;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rw_meta::{Diagnostic, Meta};
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// URL path; see [`Storage`].
    pub path: String,
    /// True if .md file exists.
    pub has_content: bool,
    /// Resolved page metadata shared across storage consumers.
    pub meta: Arc<Meta>,
    /// Source directory name, set only on a homepage served from the project's
    /// `README.md` outside the source directory. When set (e.g., `"docs"`), the
    /// renderer resolves relative links with this prefix from the source
    /// directory.
    pub origin: Option<String>,
    /// True when this page's URL denotes a directory — its content comes from a
    /// directory page file (`index.md` or `README.md`) — rather than a single
    /// file (a leaf `name.md`).
    ///
    /// Defaults to `true` for backward compatibility with S3 bundles published
    /// before this field existed (preserving their directory-style resolution).
    pub is_dir: bool,
    /// The content comes from a `README.md`, so links to that file lead here.
    pub readme: bool,
    /// Recoverable metadata problems from parsing this document's sources.
    /// Never serialized: the wire carries the resolved `meta` only, so a
    /// deserialized `Document` always has these empty.
    pub diagnostics: Arc<[Diagnostic]>,
}

impl Document {
    /// The markdown file this page is published from, relative to the source
    /// directory, or `None` for a page without content.
    ///
    /// The project `README.md` homepage is `../README.md`: like `origin`, this
    /// takes the project directory to be the source directory's parent.
    #[must_use]
    pub fn source_path(&self) -> Option<String> {
        if !self.has_content {
            return None;
        }
        if self.origin.is_some() {
            return Some("../README.md".to_owned());
        }
        if !self.is_dir {
            return Some(format!("{}.md", self.path));
        }
        let name = if self.readme { "README.md" } else { "index.md" };
        Some(if self.path.is_empty() {
            name.to_owned()
        } else {
            format!("{}/{name}", self.path)
        })
    }
}

#[derive(Serialize, Deserialize)]
struct DocumentWire<S, P, A> {
    path: S,
    title: S,
    has_content: bool,
    #[serde(rename = "page_kind", default, skip_serializing_if = "Option::is_none")]
    kind: Option<S>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    namespace: Option<S>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<S>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<S>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<S>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pages: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attrs: Option<A>,
    #[serde(default = "default_is_dir")]
    is_dir: bool,
    #[serde(default, skip_serializing_if = "Not::not")]
    readme: bool,
}

impl Serialize for Document {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        DocumentWire::<&str, &[String], &BTreeMap<String, serde_json::Value>> {
            path: self.path.as_str(),
            title: self.meta.title.as_str(),
            has_content: self.has_content,
            kind: self.meta.kind.as_deref(),
            namespace: self.meta.namespace.as_deref(),
            name: self.meta.name.as_deref(),
            description: self.meta.description.as_deref(),
            origin: self.origin.as_deref(),
            pages: self.meta.pages.as_deref(),
            attrs: (!self.meta.attrs.is_empty()).then_some(&self.meta.attrs),
            is_dir: self.is_dir,
            readme: self.readme,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire =
            DocumentWire::<String, Vec<String>, BTreeMap<String, serde_json::Value>>::deserialize(
                deserializer,
            )?;

        Ok(Self {
            path: wire.path,
            has_content: wire.has_content,
            meta: Arc::new(Meta {
                attrs: wire.attrs.unwrap_or_default(),
                name: wire.name,
                title: wire.title,
                description: wire.description,
                kind: wire.kind,
                namespace: wire.namespace,
                pages: wire.pages,
            }),
            origin: wire.origin,
            is_dir: wire.is_dir,
            readme: wire.readme,
            diagnostics: Vec::new().into(),
        })
    }
}

/// Semantic error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageErrorKind {
    /// Resource does not exist.
    NotFound,
    /// Permission denied.
    PermissionDenied,
    /// Invalid path or identifier.
    InvalidPath,
    /// Backend is temporarily unavailable.
    Unavailable,
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
            StorageErrorKind::InvalidPath => "Invalid path",
            StorageErrorKind::Unavailable => "Unavailable",
            StorageErrorKind::Timeout => "Timeout",
            StorageErrorKind::Other => "Error",
        };
        f.write_str(label)
    }
}

/// Storage error with a semantic [`kind`](Self::kind) and a backend-specific
/// source, modeled on [`std::io::Error`].
#[derive(Debug)]
pub struct StorageError {
    /// Semantic error category.
    pub kind: StorageErrorKind,
    /// Path context (if applicable).
    pub path: Option<PathBuf>,
    /// Backend identifier (e.g., "Fs", "Mock").
    pub backend: Option<&'static str>,
    source: Option<Box<dyn Error + Send + Sync>>,
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
    pub fn with_source(mut self, source: impl Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
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
    /// Renders `"[Backend] Kind (path: …)"`, delegating the kind label to
    /// [`StorageErrorKind`]'s `Display`. The source is left to
    /// [`source()`](Error::source).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(backend) = self.backend {
            write!(f, "[{backend}] ")?;
        }

        write!(f, "{}", self.kind)?;

        if let Some(path) = &self.path {
            write!(f, " (path: {})", path.display())?;
        }

        Ok(())
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn Error + 'static))
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
    /// # Errors
    ///
    /// Returns [`StorageError`] if the document doesn't exist or can't be read.
    fn read(&self, path: &str) -> Result<String, StorageError>;

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
    /// # Errors
    ///
    /// Returns [`StorageError`] if the document doesn't exist or mtime can't be retrieved.
    fn mtime(&self, path: &str) -> Result<f64, StorageError>;

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

    /// The source directory's path relative to the project root,
    /// `/`-separated: `docs`, `site/docs`, or `""` when the source directory
    /// is the project root. Joined with [`Document::source_path`], folding
    /// `..`, it places a page's file in the project.
    ///
    /// `None` when the source directory lies outside the project root, or the
    /// backend does not know where it lies.
    fn source_dir_in_project(&self) -> Option<String>;
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
    use std::sync::Arc;

    use rw_meta::{Diagnostic, DiagnosticSource, Meta, Severity};

    use super::*;

    /// Magnitudes that denote no representable instant. The wrong-unit epoch
    /// timestamps are the realistic case: a foreign tool writing microseconds
    /// or nanoseconds into a seconds field. (Milliseconds still land inside
    /// chrono's range, so they convert to a real — if absurd — date.) JSON
    /// manifests cannot carry NaN or the infinities, but any other `Storage`
    /// can return them.
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
    fn attrs_f64_bits_roundtrip_document_bytes() {
        for (yaml, expected_bits) in [
            ("51.248178375505404", 0x4049_9fc4_4f1b_2f60),
            ("0.1", 0.1_f64.to_bits()),
            ("-0.125", (-0.125_f64).to_bits()),
            ("1.2345678901234567", 1.234_567_890_123_456_7_f64.to_bits()),
        ] {
            let resolved = Meta::resolve_with_diagnostics(
                None,
                Some(&format!("attrs: {{value: {yaml}}}")),
                "guide",
            );
            assert_eq!(resolved.diagnostics, [] as [rw_meta::Diagnostic; 0]);
            assert_eq!(
                resolved.meta.attrs["value"].as_f64().unwrap().to_bits(),
                expected_bits
            );
            let document = Document {
                path: "guide".into(),
                has_content: true,
                meta: Arc::new(resolved.meta),
                origin: None,
                is_dir: false,
                readme: false,
                diagnostics: Vec::new().into(),
            };
            let bytes = serde_json::to_vec(&document).unwrap();
            let decoded: Document = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                decoded.meta.attrs["value"].as_f64().unwrap().to_bits(),
                expected_bits,
                "{yaml}"
            );
        }
    }

    #[test]
    fn document_omits_absent_optional_fields_and_defaults_is_dir() {
        let minimal = serde_json::json!({"path": "guide", "title": "Guide", "has_content": true});

        let document: Document = serde_json::from_value(minimal).unwrap();

        assert_eq!(
            document,
            Document {
                path: "guide".to_owned(),
                has_content: true,
                meta: Arc::new(Meta {
                    attrs: BTreeMap::new(),
                    name: None,
                    title: "Guide".to_owned(),
                    description: None,
                    kind: None,
                    namespace: None,
                    pages: None,
                }),
                origin: None,
                is_dir: true,
                readme: false,
                diagnostics: Vec::new().into(),
            }
        );
        assert_eq!(
            serde_json::to_value(document).unwrap(),
            serde_json::json!({"path": "guide", "title": "Guide", "has_content": true, "is_dir": true})
        );
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
        assert!(err.source().is_some());
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
    fn test_storage_error_display() {
        use StorageErrorKind::{
            InvalidPath, NotFound, Other, PermissionDenied, Timeout, Unavailable,
        };

        for (err, expected) in [
            (StorageError::new(NotFound), "Not found"),
            (StorageError::new(PermissionDenied), "Permission denied"),
            (StorageError::new(InvalidPath), "Invalid path"),
            (StorageError::new(Unavailable), "Unavailable"),
            (StorageError::new(Timeout), "Timeout"),
            (StorageError::new(Other), "Error"),
            (
                StorageError::new(NotFound).with_backend("Fs"),
                "[Fs] Not found",
            ),
            (
                StorageError::new(NotFound).with_path("/foo/bar"),
                "Not found (path: /foo/bar)",
            ),
            (
                StorageError::new(NotFound)
                    .with_backend("Fs")
                    .with_path("/foo/bar")
                    .with_source(std::io::Error::other("file not found")),
                "[Fs] Not found (path: /foo/bar)",
            ),
        ] {
            assert_eq!(err.to_string(), expected);
        }
    }

    #[test]
    fn document_carries_readme_only_when_set() {
        let wire = serde_json::json!({
            "path": "a", "title": "A", "has_content": true, "is_dir": true, "readme": true
        });

        let document: Document = serde_json::from_value(wire.clone()).unwrap();

        assert!(document.readme);
        assert_eq!(serde_json::to_value(document).unwrap(), wire);
    }

    #[test]
    fn source_path_follows_the_published_file() {
        for (wire, source_path) in [
            (
                serde_json::json!({"is_dir": false, "path": "a/b"}),
                Some("a/b.md"),
            ),
            (serde_json::json!({"path": "a"}), Some("a/index.md")),
            (serde_json::json!({"path": ""}), Some("index.md")),
            (
                serde_json::json!({"readme": true, "path": "a"}),
                Some("a/README.md"),
            ),
            (
                serde_json::json!({"readme": true, "path": ""}),
                Some("README.md"),
            ),
            (
                serde_json::json!({"readme": true, "path": "", "origin": "docs"}),
                Some("../README.md"),
            ),
            (serde_json::json!({"path": "a", "has_content": false}), None),
        ] {
            let mut document = serde_json::json!({"title": "T", "has_content": true});
            document
                .as_object_mut()
                .unwrap()
                .extend(wire.as_object().unwrap().clone());
            let document: Document = serde_json::from_value(document).unwrap();
            assert_eq!(document.source_path().as_deref(), source_path, "{wire}");
        }
    }

    #[test]
    fn diagnostics_never_reach_the_wire() {
        let clean = Document {
            path: "guide".to_owned(),
            has_content: true,
            meta: Arc::new(Meta {
                attrs: BTreeMap::new(),
                name: None,
                title: "Guide".to_owned(),
                description: Some("Getting started".to_owned()),
                kind: Some("domain".to_owned()),
                namespace: Some("payments".to_owned()),
                pages: Some(vec!["intro".to_owned()]),
            }),
            origin: Some("docs".to_owned()),
            is_dir: false,
            readme: false,
            diagnostics: Vec::new().into(),
        };
        let mut noisy = clean.clone();
        noisy.diagnostics = Arc::from(vec![Diagnostic {
            source: DiagnosticSource::Sidecar,
            field: Some("title".to_owned()),
            severity: Severity::Warning,
            message: "expected a string, found a number".to_owned(),
        }]);

        assert_eq!(
            serde_json::to_value(&clean).unwrap(),
            serde_json::to_value(&noisy).unwrap()
        );
        assert!(
            serde_json::from_value::<Document>(serde_json::to_value(&clean).unwrap())
                .unwrap()
                .diagnostics
                .is_empty()
        );
    }
}
