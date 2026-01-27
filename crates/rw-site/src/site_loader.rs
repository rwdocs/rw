//! Site loading from storage.
//!
//! Provides [`SiteLoader`] for building [`Site`] structures from a [`Storage`]
//! backend. Includes optional file-based caching.
//!
//! # Architecture
//!
//! The loader uses a [`Storage`] implementation to scan for documents:
//! - `index.md` files become section landing pages
//! - Other `.md` files become standalone pages
//! - Directories without `index.md` have their children promoted to parent level
//!
//! # Thread Safety
//!
//! `SiteLoader` is designed for concurrent access:
//! - `get()` returns `Arc<Site>` with minimal locking (just Arc clone)
//! - `reload_if_needed()` uses double-checked locking for efficient cache validation
//! - `invalidate()` is lock-free (atomic flag)
//!
//! # Example
//!
//! ```ignore
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use rw_site::{SiteLoader, SiteLoaderConfig};
//!
//! let config = SiteLoaderConfig {
//!     source_dir: PathBuf::from("docs"),
//!     cache_dir: Some(PathBuf::from(".cache")),
//! };
//! let loader = Arc::new(SiteLoader::new(config));
//!
//! // Concurrent access from multiple threads
//! let site = loader.reload_if_needed();
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use rw_storage::{FsStorage, Storage};

use crate::site::{Site, SiteBuilder};
use crate::site_cache::{FileSiteCache, NullSiteCache, SiteCache};

/// Convert Duration to milliseconds as f64.
fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Configuration for [`SiteLoader`].
#[derive(Clone, Debug)]
pub struct SiteLoaderConfig {
    /// Root directory containing markdown sources.
    pub source_dir: PathBuf,
    /// Cache directory for site structure (`None` disables caching).
    pub cache_dir: Option<PathBuf>,
}

/// Loads site structure from storage.
///
/// Uses a [`Storage`] implementation to scan for markdown files and builds a
/// [`Site`] structure. Uses `index.md` files as section landing pages. Titles
/// are provided by the storage (extracted or stored depending on backend).
///
/// # Thread Safety
///
/// This struct is designed for concurrent access without external locking:
/// - Uses internal `RwLock<Arc<Site>>` for the current site snapshot
/// - Uses `Mutex<()>` for serializing reload operations
/// - Uses `AtomicBool` for cache validity tracking
pub struct SiteLoader {
    config: SiteLoaderConfig,
    storage: Arc<dyn Storage>,
    file_cache: Box<dyn SiteCache>,
    /// Mutex for serializing reload operations.
    reload_lock: Mutex<()>,
    /// Current site snapshot (atomically swappable).
    current_site: RwLock<Arc<Site>>,
    /// Cache validity flag.
    cache_valid: AtomicBool,
}

impl SiteLoader {
    /// Create a new site loader with filesystem storage.
    ///
    /// # Arguments
    ///
    /// * `config` - Loader configuration
    #[must_use]
    pub fn new(config: SiteLoaderConfig) -> Self {
        let storage = Arc::new(FsStorage::new(config.source_dir.clone()));
        Self::with_storage(config, storage)
    }

    /// Create a new site loader with custom storage.
    ///
    /// # Arguments
    ///
    /// * `config` - Loader configuration
    /// * `storage` - Storage implementation for document scanning
    #[must_use]
    pub fn with_storage(config: SiteLoaderConfig, storage: Arc<dyn Storage>) -> Self {
        let file_cache: Box<dyn SiteCache> = match &config.cache_dir {
            Some(dir) => Box::new(FileSiteCache::new(dir.clone())),
            None => Box::new(NullSiteCache),
        };

        // Create initial empty site
        let initial_site = Arc::new(SiteBuilder::new(config.source_dir.clone()).build());

        Self {
            config,
            storage,
            file_cache,
            reload_lock: Mutex::new(()),
            current_site: RwLock::new(initial_site),
            cache_valid: AtomicBool::new(false),
        }
    }

    /// Get current site snapshot.
    ///
    /// Returns an `Arc<Site>` that can be used without holding any lock.
    /// The site is guaranteed to be internally consistent.
    ///
    /// Note: This returns the current snapshot without checking cache validity.
    /// For most use cases, prefer `reload_if_needed()` which ensures the site
    /// is up-to-date.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[must_use]
    pub fn get(&self) -> Arc<Site> {
        self.current_site.read().unwrap().clone()
    }

    /// Reload site from storage if cache is invalid.
    ///
    /// Uses double-checked locking pattern:
    /// 1. Fast path: return current site if cache valid
    /// 2. Slow path: acquire `reload_lock`, recheck, then reload
    ///
    /// # Returns
    ///
    /// `Arc<Site>` containing the current site snapshot.
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    pub fn reload_if_needed(&self) -> Arc<Site> {
        let start = Instant::now();

        // Fast path: cache valid
        if self.cache_valid.load(Ordering::Acquire) {
            return self.get();
        }

        // Slow path: acquire reload lock
        let _guard = self.reload_lock.lock().unwrap();

        // Double-check after acquiring lock
        if self.cache_valid.load(Ordering::Acquire) {
            return self.get();
        }

        // Try file cache first
        let file_cache_start = Instant::now();
        if let Some(site) = self.file_cache.get() {
            let file_cache_ms = elapsed_ms(file_cache_start);
            let site = Arc::new(site);
            *self.current_site.write().unwrap() = site.clone();
            self.cache_valid.store(true, Ordering::Release);
            tracing::info!(
                source = "file_cache",
                file_cache_ms,
                elapsed_ms = elapsed_ms(start),
                "Site reloaded"
            );
            return site;
        }
        let file_cache_ms = elapsed_ms(file_cache_start);

        // Load from storage
        let storage_start = Instant::now();
        let site = self.load_from_storage();
        let storage_scan_ms = elapsed_ms(storage_start);

        let site = Arc::new(site);

        // Store in file cache
        let cache_store_start = Instant::now();
        self.file_cache.set(&site);
        let cache_store_ms = elapsed_ms(cache_store_start);

        // Update current site
        *self.current_site.write().unwrap() = site.clone();
        self.cache_valid.store(true, Ordering::Release);

        let page_count = site.pages().len();
        tracing::info!(
            source = "storage",
            page_count,
            file_cache_check_ms = file_cache_ms,
            storage_scan_ms,
            cache_store_ms,
            elapsed_ms = elapsed_ms(start),
            "Site reloaded"
        );

        site
    }

    /// Invalidate cached site.
    ///
    /// Marks cache as invalid. Next `reload_if_needed()` will reload.
    /// Current readers continue using their existing `Arc<Site>`.
    pub fn invalidate(&self) {
        self.cache_valid.store(false, Ordering::Release);
        self.file_cache.invalidate();
    }

    /// Get source directory.
    #[must_use]
    pub fn source_dir(&self) -> &Path {
        &self.config.source_dir
    }

    /// Load site from storage and build hierarchy.
    ///
    /// Uses storage.scan() to get documents, then builds hierarchy based on
    /// path conventions.
    fn load_from_storage(&self) -> Site {
        let mut builder = SiteBuilder::new(self.config.source_dir.clone());

        // Scan storage for documents
        let documents = match self.storage.scan() {
            Ok(docs) => docs,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to scan storage");
                return builder.build();
            }
        };

        if documents.is_empty() {
            return builder.build();
        }

        // Sort documents: index.md files first (at each level), then alphabetical
        // This ensures parent pages are created before their children
        let mut sorted_docs: Vec<_> = documents.iter().collect();
        sorted_docs.sort_by(|a, b| {
            let a_is_index = a.path.file_name().is_some_and(|n| n == "index.md");
            let b_is_index = b.path.file_name().is_some_and(|n| n == "index.md");
            let a_depth = a.path.components().count();
            let b_depth = b.path.components().count();

            // Sort by depth first (shallower first), then index.md before others,
            // then alphabetical
            a_depth
                .cmp(&b_depth)
                .then_with(|| b_is_index.cmp(&a_is_index)) // true > false, so reverse
                .then_with(|| a.path.cmp(&b.path))
        });

        // Track added pages by their source path for parent lookup
        let mut path_to_idx: HashMap<PathBuf, usize> = HashMap::new();

        // Process documents in sorted order
        for doc in sorted_docs {
            let url_path = Self::source_path_to_url(&doc.path);
            let parent_idx = Self::find_parent(&doc.path, &path_to_idx);

            let idx = builder.add_page(doc.title.clone(), url_path, doc.path.clone(), parent_idx);
            path_to_idx.insert(doc.path.clone(), idx);
        }

        tracing::debug!(document_count = documents.len(), "Site scan completed");

        builder.build()
    }

    /// Convert source path to URL path.
    ///
    /// Examples:
    /// - `"index.md"` -> `"/"`
    /// - `"guide.md"` -> `"/guide"`
    /// - `"domain/index.md"` -> `"/domain"`
    /// - `"domain/setup.md"` -> `"/domain/setup"`
    fn source_path_to_url(source_path: &Path) -> String {
        let path_str = source_path.to_string_lossy();

        // Handle root index.md
        if path_str == "index.md" {
            return "/".to_string();
        }

        // Remove .md extension
        let without_ext = path_str.strip_suffix(".md").unwrap_or(&path_str);

        // Handle directory index files
        if let Some(without_index) = without_ext.strip_suffix("/index") {
            return format!("/{without_index}");
        }
        if without_ext == "index" {
            return "/".to_string();
        }

        format!("/{without_ext}")
    }

    /// Find parent page index for a document.
    ///
    /// Uses path conventions to determine parent:
    /// - `"guide.md"` -> parent is `"/"` (root) if `"index.md"` exists
    /// - `"domain/setup.md"` -> parent is `"/domain"` if `"domain/index.md"` exists
    /// - Directories without `index.md` promote children to grandparent
    fn find_parent(source_path: &Path, path_to_idx: &HashMap<PathBuf, usize>) -> Option<usize> {
        let path_str = source_path.to_string_lossy();

        // Root index.md has no parent
        if path_str == "index.md" {
            return None;
        }

        // Get the parent directory
        let parent_dir = source_path.parent()?;

        if parent_dir.as_os_str().is_empty() {
            // Top-level file (e.g., "guide.md")
            // Parent is root index.md if it exists
            let root_index = PathBuf::from("index.md");
            return path_to_idx.get(&root_index).copied();
        }

        // Look for index.md in parent directory
        let parent_index = parent_dir.join("index.md");
        if let Some(&idx) = path_to_idx.get(&parent_index) {
            return Some(idx);
        }

        // No index.md in parent - recursively check grandparent
        // We need to find the grandparent by looking at parent_dir's parent
        let grandparent_dir = parent_dir.parent()?;
        if grandparent_dir.as_os_str().is_empty() {
            // Parent is at root level, check for root index.md
            let root_index = PathBuf::from("index.md");
            return path_to_idx.get(&root_index).copied();
        }

        // Look for index.md in grandparent
        let grandparent_index = grandparent_dir.join("index.md");
        if let Some(&idx) = path_to_idx.get(&grandparent_index) {
            return Some(idx);
        }

        // Continue recursion with grandparent's index
        Self::find_parent_up(grandparent_dir, path_to_idx)
    }

    /// Helper to find parent by walking up directory tree.
    fn find_parent_up(dir: &Path, path_to_idx: &HashMap<PathBuf, usize>) -> Option<usize> {
        let parent_dir = dir.parent()?;

        if parent_dir.as_os_str().is_empty() {
            // At root, check for root index.md
            let root_index = PathBuf::from("index.md");
            return path_to_idx.get(&root_index).copied();
        }

        // Check for index.md in parent
        let parent_index = parent_dir.join("index.md");
        if let Some(&idx) = path_to_idx.get(&parent_index) {
            return Some(idx);
        }

        // Continue up
        Self::find_parent_up(parent_dir, path_to_idx)
    }
}

#[cfg(test)]
mod tests {
    // Ensure SiteLoader is Send + Sync for use with Arc
    static_assertions::assert_impl_all!(super::SiteLoader: Send, Sync);
    use std::fs;
    use std::sync::Arc;

    use super::*;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_reload_if_needed_missing_dir_returns_empty_site() {
        let temp_dir = create_test_dir();
        let config = SiteLoaderConfig {
            source_dir: temp_dir.path().join("nonexistent"),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert!(site.get_root_pages().is_empty());
    }

    #[test]
    fn test_reload_if_needed_empty_dir_returns_empty_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert!(site.get_root_pages().is_empty());
    }

    #[test]
    fn test_reload_if_needed_flat_structure_builds_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# User Guide\n\nContent.").unwrap();
        fs::write(source_dir.join("api.md"), "# API Reference\n\nDocs.").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert_eq!(site.get_root_pages().len(), 2);
        assert!(site.get_page("/guide").is_some());
        assert!(site.get_page("/api").is_some());
    }

    #[test]
    fn test_reload_if_needed_root_index_adds_home_page() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(
            source_dir.join("index.md"),
            "# Welcome\n\nHome page content.",
        )
        .unwrap();

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        let page = site.get_page("/");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Welcome");
        assert_eq!(page.path, "/");
        assert_eq!(page.source_path, PathBuf::from("index.md"));
        // resolve_source_path returns canonicalized path
        let resolved = site.resolve_source_path("/");
        let expected = source_dir.join("index.md").canonicalize().unwrap();
        assert_eq!(resolved, Some(expected));
    }

    #[test]
    fn test_reload_if_needed_nested_structure_builds_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        let domain_dir = source_dir.join("domain-a");
        fs::create_dir_all(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain A\n\nOverview.").unwrap();
        fs::write(domain_dir.join("guide.md"), "# Setup Guide\n\nSteps.").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        let domain = site.get_page("/domain-a");
        assert!(domain.is_some());
        let domain = domain.unwrap();
        assert_eq!(domain.title, "Domain A");
        assert_eq!(domain.source_path, PathBuf::from("domain-a/index.md"));

        let children = site.get_children("/domain-a");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].title, "Setup Guide");
        assert_eq!(children[0].source_path, PathBuf::from("domain-a/guide.md"));
    }

    #[test]
    fn test_reload_if_needed_extracts_title_from_h1() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# My Custom Title\n\nContent.").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        let page = site.get_page("/guide");
        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "My Custom Title");
    }

    #[test]
    fn test_reload_if_needed_falls_back_to_filename() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(
            source_dir.join("setup-guide.md"),
            "Content without heading.",
        )
        .unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        let page = site.get_page("/setup-guide");
        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "Setup Guide");
    }

    #[test]
    fn test_reload_if_needed_cyrillic_filename() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(
            source_dir.join("руководство.md"),
            "# Руководство\n\nСодержимое.",
        )
        .unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        let page = site.get_page("/руководство");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Руководство");
        assert_eq!(page.path, "/руководство");
        assert_eq!(page.source_path, PathBuf::from("руководство.md"));
    }

    #[test]
    fn test_reload_if_needed_skips_hidden_files() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join(".hidden.md"), "# Hidden").unwrap();
        fs::write(source_dir.join("visible.md"), "# Visible").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert!(site.get_page("/.hidden").is_none());
        assert!(site.get_page("/visible").is_some());
    }

    #[test]
    fn test_reload_if_needed_skips_underscore_files() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("_partial.md"), "# Partial").unwrap();
        fs::write(source_dir.join("main.md"), "# Main").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert!(site.get_page("/_partial").is_none());
        assert!(site.get_page("/main").is_some());
    }

    #[test]
    fn test_reload_if_needed_directory_without_index_promotes_children() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        let no_index_dir = source_dir.join("no-index");
        fs::create_dir_all(&no_index_dir).unwrap();
        fs::write(no_index_dir.join("child.md"), "# Child Page").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        // Child should be at root level (promoted)
        let roots = site.get_root_pages();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/no-index/child");
        assert_eq!(roots[0].source_path, PathBuf::from("no-index/child.md"));
    }

    #[test]
    fn test_get_returns_same_arc() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        // First reload to populate
        let _ = loader.reload_if_needed();

        // Get should return the same Arc
        let site1 = loader.get();
        let site2 = loader.get();

        assert!(Arc::ptr_eq(&site1, &site2));
    }

    #[test]
    fn test_reload_if_needed_caches_result() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site1 = loader.reload_if_needed();
        let site2 = loader.reload_if_needed();

        // Should return the same Arc (cached)
        assert!(Arc::ptr_eq(&site1, &site2));
    }

    #[test]
    fn test_invalidate_clears_cached_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        // First reload - should NOT have /new
        let site1 = loader.reload_if_needed();
        assert!(site1.get_page("/new").is_none());

        // Add new file and invalidate
        fs::write(source_dir.join("new.md"), "# New").unwrap();
        loader.invalidate();

        // Second reload - should have /new now
        let site2 = loader.reload_if_needed();
        assert!(site2.get_page("/new").is_some());

        // Should be a different Arc (reloaded)
        assert!(!Arc::ptr_eq(&site1, &site2));
    }

    #[test]
    fn test_reload_if_needed_site_has_source_dir() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        assert_eq!(site.source_dir(), source_dir);
    }

    #[test]
    fn test_source_dir_getter() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        assert_eq!(loader.source_dir(), source_dir);
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = Arc::new(SiteLoader::new(config));

        // Spawn multiple threads accessing concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let loader = Arc::clone(&loader);
                thread::spawn(move || {
                    let site = loader.reload_if_needed();
                    assert!(site.get_page("/guide").is_some());
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_concurrent_invalidate_and_reload() {
        use std::thread;

        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = Arc::new(SiteLoader::new(config));

        // Initial load
        let _ = loader.reload_if_needed();

        // Spawn threads that invalidate and reload concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let loader = Arc::clone(&loader);
                thread::spawn(move || {
                    if i % 2 == 0 {
                        loader.invalidate();
                    } else {
                        let site = loader.reload_if_needed();
                        // Site should always be valid
                        assert!(site.get_page("/guide").is_some());
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Final state should be valid
        let site = loader.reload_if_needed();
        assert!(site.get_page("/guide").is_some());
    }

    #[test]
    fn test_mtime_cache_reuses_titles() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Original Title").unwrap();

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        // First load
        let site1 = loader.reload_if_needed();
        assert_eq!(site1.get_page("/guide").unwrap().title, "Original Title");

        // Invalidate and reload without changing file - should use cached title
        loader.invalidate();
        let site2 = loader.reload_if_needed();
        assert_eq!(site2.get_page("/guide").unwrap().title, "Original Title");
    }

    #[test]
    fn test_mtime_cache_detects_changes() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Original Title").unwrap();

        let config = SiteLoaderConfig {
            source_dir: source_dir.clone(),
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        // First load
        let site1 = loader.reload_if_needed();
        assert_eq!(site1.get_page("/guide").unwrap().title, "Original Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify file
        fs::write(source_dir.join("guide.md"), "# Updated Title").unwrap();
        loader.invalidate();

        // Reload should see new title
        let site2 = loader.reload_if_needed();
        assert_eq!(site2.get_page("/guide").unwrap().title, "Updated Title");
    }

    #[test]
    fn test_source_path_to_url() {
        assert_eq!(SiteLoader::source_path_to_url(Path::new("index.md")), "/");
        assert_eq!(
            SiteLoader::source_path_to_url(Path::new("guide.md")),
            "/guide"
        );
        assert_eq!(
            SiteLoader::source_path_to_url(Path::new("domain/index.md")),
            "/domain"
        );
        assert_eq!(
            SiteLoader::source_path_to_url(Path::new("domain/setup.md")),
            "/domain/setup"
        );
        assert_eq!(
            SiteLoader::source_path_to_url(Path::new("a/b/c.md")),
            "/a/b/c"
        );
    }

    #[test]
    fn test_nested_hierarchy_with_multiple_levels() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir_all(source_dir.join("level1/level2")).unwrap();
        fs::write(source_dir.join("index.md"), "# Home").unwrap();
        fs::write(source_dir.join("level1/index.md"), "# Level 1").unwrap();
        fs::write(source_dir.join("level1/level2/index.md"), "# Level 2").unwrap();
        fs::write(source_dir.join("level1/level2/page.md"), "# Deep Page").unwrap();

        let config = SiteLoaderConfig {
            source_dir,
            cache_dir: None,
        };
        let loader = SiteLoader::new(config);

        let site = loader.reload_if_needed();

        // Check root
        let root = site.get_page("/").unwrap();
        assert_eq!(root.title, "Home");

        // Check level 1
        let level1 = site.get_page("/level1").unwrap();
        assert_eq!(level1.title, "Level 1");

        // Check level 1 is child of root
        let root_children = site.get_children("/");
        assert!(root_children.iter().any(|c| c.path == "/level1"));

        // Check level 2
        let level2 = site.get_page("/level1/level2").unwrap();
        assert_eq!(level2.title, "Level 2");

        // Check level 2 is child of level 1
        let level1_children = site.get_children("/level1");
        assert!(level1_children.iter().any(|c| c.path == "/level1/level2"));

        // Check deep page
        let deep = site.get_page("/level1/level2/page").unwrap();
        assert_eq!(deep.title, "Deep Page");

        // Check deep page is child of level 2
        let level2_children = site.get_children("/level1/level2");
        assert!(
            level2_children
                .iter()
                .any(|c| c.path == "/level1/level2/page")
        );
    }
}
