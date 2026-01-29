//! Site caching for persistent storage.
//!
//! Provides [`SiteCache`] trait and implementations for caching [`SiteState`] structures:
//! - [`FileSiteCache`]: File-based cache using JSON serialization
//! - [`NullSiteCache`]: No-op cache (always returns `None`)
//!
//! # Cache Format
//!
//! Sites are serialized as JSON with the following structure:
//!
//! ```json
//! {
//!     "pages": [
//!         {"title": "Guide", "path": "/guide", "source_path": "guide.md"}
//!     ],
//!     "children": [[1, 2], []],
//!     "parents": [null, 0],
//!     "roots": [0]
//! }
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::site_state::{Page, SectionInfo, SiteState};

/// Cache format for serialization.
#[derive(Serialize, Deserialize)]
struct CachedSiteState {
    pages: Vec<Page>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
    #[serde(default)]
    sections: HashMap<String, SectionInfo>,
}

impl From<&SiteState> for CachedSiteState {
    fn from(site: &SiteState) -> Self {
        Self {
            pages: site.pages().to_vec(),
            children: site.children_indices().to_vec(),
            parents: site.parent_indices().to_vec(),
            roots: site.root_indices().to_vec(),
            sections: site.sections().clone(),
        }
    }
}

impl From<CachedSiteState> for SiteState {
    fn from(cached: CachedSiteState) -> Self {
        SiteState::new(
            cached.pages,
            cached.children,
            cached.parents,
            cached.roots,
            cached.sections,
        )
    }
}

/// Trait for site caching implementations.
pub(crate) trait SiteCache: Send + Sync {
    /// Retrieve cached site state.
    ///
    /// Returns `None` on cache miss or invalid cache.
    fn get(&self) -> Option<SiteState>;

    /// Store site state in cache.
    fn set(&self, site: &SiteState);

    /// Remove cached site state.
    fn invalidate(&self);
}

/// No-op cache that never stores or retrieves data.
///
/// Used when caching is disabled. All operations are no-ops.
pub(crate) struct NullSiteCache;

impl SiteCache for NullSiteCache {
    fn get(&self) -> Option<SiteState> {
        None
    }

    fn set(&self, _site: &SiteState) {}

    fn invalidate(&self) {}
}

/// File-based cache for site structure.
///
/// Stores the site as JSON in `{cache_dir}/site.json`.
pub(crate) struct FileSiteCache {
    cache_dir: PathBuf,
}

impl FileSiteCache {
    /// Create a new file-based site cache.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Directory to store cache file
    #[must_use]
    pub(crate) fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Get path to cache file.
    fn cache_path(&self) -> PathBuf {
        self.cache_dir.join("site.json")
    }
}

impl SiteCache for FileSiteCache {
    fn get(&self) -> Option<SiteState> {
        let cache_path = self.cache_path();
        if !cache_path.exists() {
            return None;
        }

        let content = fs::read_to_string(&cache_path).ok()?;
        let cached: CachedSiteState = serde_json::from_str(&content).ok()?;
        Some(cached.into())
    }

    fn set(&self, site: &SiteState) {
        // Ensure cache directory exists
        if let Err(e) = fs::create_dir_all(&self.cache_dir) {
            // Silently ignore errors - cache is optional
            tracing::debug!(error = %e, "Failed to create cache directory");
            return;
        }

        let cached = CachedSiteState::from(site);
        let content = match serde_json::to_string(&cached) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(error = %e, "Failed to serialize site");
                return;
            }
        };

        if let Err(e) = fs::write(self.cache_path(), content) {
            tracing::debug!(error = %e, "Failed to write site cache");
        }
    }

    fn invalidate(&self) {
        let cache_path = self.cache_path();
        if cache_path.exists()
            && let Err(e) = fs::remove_file(&cache_path)
        {
            tracing::debug!(error = %e, "Failed to remove site cache");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site_state::SiteStateBuilder;

    fn create_test_site() -> SiteState {
        let mut builder = SiteStateBuilder::new();
        let root_idx = builder.add_page(
            "Home".to_string(),
            "/".to_string(),
            PathBuf::from("index.md"),
            None,
            None,
        );
        builder.add_page(
            "Guide".to_string(),
            "/guide".to_string(),
            PathBuf::from("guide.md"),
            Some(root_idx),
            None,
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

        assert!(parsed.get("pages").is_some());
        assert!(parsed.get("children").is_some());
        assert!(parsed.get("parents").is_some());
        assert!(parsed.get("roots").is_some());
    }
}
