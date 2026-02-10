//! Page caching infrastructure.
//!
//! Provides a trait for page caching and implementations:
//! - [`PageCache`]: Trait for cache implementations
//! - [`NullPageCache`]: No-op cache (disabled caching)
//! - [`FilePageCache`]: File-based cache for page HTML and metadata

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use rw_renderer::TocEntry;

/// Cached page metadata structure.
///
/// Stored as JSON alongside the rendered HTML.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedMetadata {
    /// Extracted page title (from first H1).
    pub title: Option<String>,
    /// Source file modification time (for invalidation).
    pub source_mtime: f64,
    /// Table of contents entries.
    pub toc: Vec<TocEntry>,
    /// Build version for cache invalidation.
    pub build_version: String,
}

/// Result of cache lookup.
#[derive(Clone, Debug)]
pub struct CacheEntry {
    /// Rendered HTML content.
    pub html: String,
    /// Cached metadata.
    pub meta: CachedMetadata,
}

/// Trait for page caching implementations.
///
/// Implemented by both `FilePageCache` (persistent storage) and `NullPageCache` (no-op).
pub trait PageCache: Send + Sync {
    /// Directory for cached diagrams (None if caching disabled).
    fn diagrams_dir(&self) -> Option<&Path>;

    /// Retrieve cached entry if valid.
    ///
    /// # Arguments
    /// * `path` - Document path (e.g., "domain-a/subdomain/guide")
    /// * `source_mtime` - Current mtime of source file
    ///
    /// # Returns
    /// `CacheEntry` if cache hit and valid, `None` otherwise.
    fn get(&self, path: &str, source_mtime: f64) -> Option<CacheEntry>;

    /// Store entry in cache.
    ///
    /// # Arguments
    /// * `path` - Document path (e.g., "domain-a/subdomain/guide")
    /// * `html` - Rendered HTML content
    /// * `title` - Extracted title (or None)
    /// * `source_mtime` - Source file mtime for invalidation
    /// * `toc` - Table of contents entries
    fn set(&self, path: &str, html: &str, title: Option<&str>, source_mtime: f64, toc: &[TocEntry]);

    /// Remove entry from cache.
    ///
    /// # Arguments
    /// * `path` - Document path to invalidate
    fn invalidate(&self, path: &str);
}

/// No-op cache implementation.
///
/// Always returns cache misses and discards stored content.
/// Use when caching is disabled.
#[derive(Debug, Default)]
pub struct NullPageCache;

impl PageCache for NullPageCache {
    fn diagrams_dir(&self) -> Option<&Path> {
        None
    }

    fn get(&self, _path: &str, _source_mtime: f64) -> Option<CacheEntry> {
        None
    }

    fn set(
        &self,
        _path: &str,
        _html: &str,
        _title: Option<&str>,
        _source_mtime: f64,
        _toc: &[TocEntry],
    ) {
    }

    fn invalidate(&self, _path: &str) {}
}

/// File-based page cache.
///
/// Stores rendered HTML and metadata in a directory structure:
/// ```text
/// {cache_dir}/
/// ├── pages/
/// │   └── {path}.html       # Rendered HTML
/// ├── meta/
/// │   └── {path}.json       # Metadata (title, mtime, toc, version)
/// └── diagrams/
///     └── {hash}.{format}   # Cached diagrams (managed by DiagramCache)
/// ```
///
/// Uses source file mtime and build version for invalidation. Cache entries
/// are considered valid when:
/// - The cached mtime matches the current source file mtime
/// - The cached build version matches the current version
#[derive(Debug)]
pub struct FilePageCache {
    cache_dir: PathBuf,
    pages_dir: PathBuf,
    meta_dir: PathBuf,
    diagrams_dir: PathBuf,
    version: String,
}

impl FilePageCache {
    /// Create a new file-based page cache.
    ///
    /// # Arguments
    /// * `cache_dir` - Root directory for cache files (e.g., `.rw/cache/`)
    /// * `version` - Build version for cache invalidation
    #[must_use]
    pub fn new(cache_dir: PathBuf, version: String) -> Self {
        let pages_dir = cache_dir.join("pages");
        let meta_dir = cache_dir.join("meta");
        let diagrams_dir = cache_dir.join("diagrams");
        Self {
            cache_dir,
            pages_dir,
            meta_dir,
            diagrams_dir,
            version,
        }
    }

    /// Ensure cache directory exists.
    fn ensure_cache_dir(&self) {
        if !self.cache_dir.exists() {
            if let Err(e) = fs::create_dir_all(&self.cache_dir) {
                eprintln!("Warning: Failed to create cache directory: {e}");
            }
        }
    }

    /// Read and validate metadata file.
    fn read_meta(&self, meta_path: &Path) -> Option<CachedMetadata> {
        let content = fs::read_to_string(meta_path).ok()?;
        let meta: CachedMetadata = serde_json::from_str(&content).ok()?;

        // Validate build version
        if meta.build_version != self.version {
            return None;
        }

        Some(meta)
    }
}

impl PageCache for FilePageCache {
    fn diagrams_dir(&self) -> Option<&Path> {
        Some(&self.diagrams_dir)
    }

    fn get(&self, path: &str, source_mtime: f64) -> Option<CacheEntry> {
        let html_path = self.pages_dir.join(format!("{path}.html"));
        let meta_path = self.meta_dir.join(format!("{path}.json"));

        if !html_path.exists() || !meta_path.exists() {
            return None;
        }

        let meta = self.read_meta(&meta_path)?;

        // Validate source mtime (1ms tolerance for JSON serialization precision loss)
        if (meta.source_mtime - source_mtime).abs() > 0.001 {
            return None;
        }

        let html = fs::read_to_string(html_path).ok()?;

        Some(CacheEntry { html, meta })
    }

    fn set(
        &self,
        path: &str,
        html: &str,
        title: Option<&str>,
        source_mtime: f64,
        toc: &[TocEntry],
    ) {
        self.ensure_cache_dir();

        let html_path = self.pages_dir.join(format!("{path}.html"));
        let meta_path = self.meta_dir.join(format!("{path}.json"));

        // Create parent directories (html and meta paths share the same parent structure)
        for path in [&html_path, &meta_path] {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
        }

        // Write HTML
        if let Err(e) = fs::write(&html_path, html) {
            eprintln!("Warning: Failed to write cache HTML: {e}");
            return;
        }

        // Write metadata
        let meta = CachedMetadata {
            title: title.map(String::from),
            source_mtime,
            toc: toc.to_vec(),
            build_version: self.version.clone(),
        };

        if let Ok(json) = serde_json::to_string(&meta) {
            let _ = fs::write(&meta_path, json);
        }
    }

    fn invalidate(&self, path: &str) {
        let html_path = self.pages_dir.join(format!("{path}.html"));
        let meta_path = self.meta_dir.join(format!("{path}.json"));

        let _ = fs::remove_file(html_path);
        let _ = fs::remove_file(meta_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_toc() -> Vec<TocEntry> {
        vec![TocEntry {
            level: 1,
            title: "Test Heading".to_string(),
            id: "test-heading".to_string(),
        }]
    }

    fn make_cache(dir: &Path) -> FilePageCache {
        FilePageCache::new(dir.to_path_buf(), "1.0.0".to_string())
    }

    #[test]
    fn test_null_cache_always_misses() {
        let cache = NullPageCache;

        assert!(cache.get("test/path", 1234.0).is_none());

        cache.set("test/path", "<html>test</html>", Some("Title"), 1234.0, &[]);
        assert!(cache.get("test/path", 1234.0).is_none());
        assert!(cache.diagrams_dir().is_none());
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_file_cache_store_and_retrieve() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = make_cache(temp_dir.path());
        let toc = make_toc();

        assert!(cache.get("domain/guide", 1234.0).is_none());

        cache.set(
            "domain/guide",
            "<html>content</html>",
            Some("Guide"),
            1234.0,
            &toc,
        );

        let entry = cache.get("domain/guide", 1234.0).unwrap();
        assert_eq!(entry.html, "<html>content</html>");
        assert_eq!(entry.meta.title, Some("Guide".to_string()));
        assert_eq!(entry.meta.source_mtime, 1234.0);
        assert_eq!(entry.meta.toc.len(), 1);
        assert_eq!(entry.meta.build_version, "1.0.0");
    }

    #[test]
    fn test_file_cache_mtime_invalidation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = make_cache(temp_dir.path());

        cache.set("test", "<html>old</html>", None, 1000.0, &[]);
        assert!(cache.get("test", 1000.0).is_some());
        assert!(cache.get("test", 2000.0).is_none());
    }

    #[test]
    fn test_file_cache_version_invalidation() {
        let temp_dir = tempfile::tempdir().unwrap();

        let cache_v1 = make_cache(temp_dir.path());
        cache_v1.set("test", "<html>v1</html>", None, 1234.0, &[]);
        assert!(cache_v1.get("test", 1234.0).is_some());

        let cache_v2 = FilePageCache::new(temp_dir.path().to_path_buf(), "2.0.0".to_string());
        assert!(cache_v2.get("test", 1234.0).is_none());
    }

    #[test]
    fn test_file_cache_invalidate() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = make_cache(temp_dir.path());

        cache.set("test", "<html>test</html>", None, 1234.0, &[]);
        assert!(cache.get("test", 1234.0).is_some());

        cache.invalidate("test");
        assert!(cache.get("test", 1234.0).is_none());
    }

    #[test]
    fn test_file_cache_diagrams_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = make_cache(temp_dir.path());
        assert_eq!(
            cache.diagrams_dir(),
            Some(temp_dir.path().join("diagrams").as_path())
        );
    }

    #[test]
    fn test_file_cache_nested_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = make_cache(temp_dir.path());

        cache.set(
            "domain/subdomain/deep/guide",
            "<html>nested</html>",
            Some("Nested Guide"),
            1234.0,
            &[],
        );

        let entry = cache.get("domain/subdomain/deep/guide", 1234.0).unwrap();
        assert_eq!(entry.html, "<html>nested</html>");
        assert_eq!(entry.meta.title, Some("Nested Guide".to_string()));
    }
}
