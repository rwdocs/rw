//! Site caching for persistent storage.
//!
//! Provides [`SiteCache`] trait and implementations for caching [`Site`] structures:
//! - [`FileSiteCache`]: File-based cache using JSON serialization
//! - [`NullSiteCache`]: No-op cache (always returns `None`)
//!
//! # Cache Format
//!
//! Sites are serialized as JSON with the following structure:
//!
//! ```json
//! {
//!     "source_dir": "/path/to/docs",
//!     "pages": [
//!         {"title": "Guide", "path": "/guide", "source_path": "guide.md"}
//!     ],
//!     "children": [[1, 2], []],
//!     "parents": [null, 0],
//!     "roots": [0]
//! }
//! ```

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::site::{Page, Site};

/// Cache format for serialization.
#[derive(Serialize, Deserialize)]
struct CachedSite {
    source_dir: PathBuf,
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
}

impl From<&Site> for CachedSite {
    fn from(site: &Site) -> Self {
        Self {
            source_dir: site.source_dir().to_path_buf(),
            pages: site.pages().to_vec(),
            children: site.children_indices().to_vec(),
            parents: site.parent_indices().to_vec(),
            roots: site.root_indices().to_vec(),
        }
    }
}

impl From<CachedSite> for Site {
    fn from(cached: CachedSite) -> Self {
        Site::new(
            cached.source_dir,
            cached.pages,
            cached.children,
            cached.parents,
            cached.roots,
        )
    }
}

/// Trait for site caching implementations.
pub trait SiteCache: Send + Sync {
    /// Retrieve cached site.
    ///
    /// Returns `None` on cache miss or invalid cache.
    fn get(&self) -> Option<Site>;

    /// Store site in cache.
    fn set(&self, site: &Site);

    /// Remove cached site.
    fn invalidate(&self);
}

/// No-op cache that never stores or retrieves data.
///
/// Used when caching is disabled. All operations are no-ops.
pub struct NullSiteCache;

impl SiteCache for NullSiteCache {
    fn get(&self) -> Option<Site> {
        None
    }

    fn set(&self, _site: &Site) {}

    fn invalidate(&self) {}
}

/// File-based cache for site structure.
///
/// Stores the site as JSON in `{cache_dir}/site.json`.
pub struct FileSiteCache {
    cache_dir: PathBuf,
}

impl FileSiteCache {
    /// Create a new file-based site cache.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Directory to store cache file
    #[must_use]
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Get path to cache file.
    fn cache_path(&self) -> PathBuf {
        self.cache_dir.join("site.json")
    }
}

impl SiteCache for FileSiteCache {
    fn get(&self) -> Option<Site> {
        let cache_path = self.cache_path();
        if !cache_path.exists() {
            return None;
        }

        let content = fs::read_to_string(&cache_path).ok()?;
        let cached: CachedSite = serde_json::from_str(&content).ok()?;
        Some(cached.into())
    }

    fn set(&self, site: &Site) {
        // Ensure cache directory exists
        if let Err(e) = fs::create_dir_all(&self.cache_dir) {
            // Silently ignore errors - cache is optional
            tracing::debug!("Failed to create cache directory: {e}");
            return;
        }

        let cached = CachedSite::from(site);
        let content = match serde_json::to_string(&cached) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("Failed to serialize site: {e}");
                return;
            }
        };

        if let Err(e) = fs::write(self.cache_path(), content) {
            tracing::debug!("Failed to write site cache: {e}");
        }
    }

    fn invalidate(&self) {
        let cache_path = self.cache_path();
        if cache_path.exists()
            && let Err(e) = fs::remove_file(&cache_path)
        {
            tracing::debug!("Failed to remove site cache: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::SiteBuilder;
    use std::path::Path;

    fn create_test_site() -> Site {
        let mut builder = SiteBuilder::new(PathBuf::from("/docs"));
        let root_idx = builder.add_page(
            "Home".to_string(),
            "/".to_string(),
            PathBuf::from("index.md"),
            None,
        );
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            Some(root_idx),
        );
        builder.build()
    }

    // NullSiteCache tests

    #[test]
    fn test_null_cache_get_returns_none() {
        let cache = NullSiteCache;
        assert!(cache.get().is_none());
    }

    #[test]
    fn test_null_cache_set_is_noop() {
        let cache = NullSiteCache;
        let site = create_test_site();
        cache.set(&site);
        assert!(cache.get().is_none());
    }

    #[test]
    fn test_null_cache_invalidate_is_noop() {
        let cache = NullSiteCache;
        cache.invalidate(); // Should not panic
    }

    // FileSiteCache tests

    #[test]
    fn test_file_cache_get_missing_returns_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSiteCache::new(temp_dir.path().join("cache"));

        assert!(cache.get().is_none());
    }

    #[test]
    fn test_file_cache_set_and_get() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSiteCache::new(temp_dir.path().join("cache"));
        let site = create_test_site();

        cache.set(&site);
        let loaded = cache.get();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.source_dir(), Path::new("/docs"));
        assert!(loaded.get_page("/").is_some());
        assert!(loaded.get_page("/guide").is_some());
    }

    #[test]
    fn test_file_cache_preserves_structure() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSiteCache::new(temp_dir.path().join("cache"));
        let site = create_test_site();

        cache.set(&site);
        let loaded = cache.get().unwrap();

        // Check page data
        let home = loaded.get_page("/").unwrap();
        assert_eq!(home.title, "Home");
        assert_eq!(home.source_path, PathBuf::from("index.md"));

        let guide = loaded.get_page("/guide").unwrap();
        assert_eq!(guide.title, "Guide");
        assert_eq!(guide.source_path, PathBuf::from("guide.md"));

        // Check hierarchy
        let children = loaded.get_children("/");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].path, "/guide");
    }

    #[test]
    fn test_file_cache_invalidate() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSiteCache::new(temp_dir.path().join("cache"));
        let site = create_test_site();

        cache.set(&site);
        assert!(cache.get().is_some());

        cache.invalidate();
        assert!(cache.get().is_none());
    }

    #[test]
    fn test_file_cache_invalidate_missing_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache = FileSiteCache::new(temp_dir.path().join("cache"));

        // Should not panic when file doesn't exist
        cache.invalidate();
    }

    #[test]
    fn test_file_cache_get_invalid_json_returns_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&cache_dir).unwrap();
        fs::write(cache_dir.join("site.json"), "invalid json").unwrap();

        let cache = FileSiteCache::new(cache_dir);
        assert!(cache.get().is_none());
    }

    #[test]
    fn test_cached_site_format() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let cache = FileSiteCache::new(cache_dir.clone());
        let site = create_test_site();

        cache.set(&site);

        // Verify JSON format is compatible
        let content = fs::read_to_string(cache_dir.join("site.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert!(parsed.get("source_dir").is_some());
        assert!(parsed.get("pages").is_some());
        assert!(parsed.get("children").is_some());
        assert!(parsed.get("parents").is_some());
        assert!(parsed.get("roots").is_some());
    }
}
