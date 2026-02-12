//! Unified site loading and rendering.
//!
//! Provides [`Site`] for building [`SiteState`] structures from a [`Storage`]
//! backend, with integrated page rendering. Includes optional file-based caching.
//!
//! # Architecture
//!
//! The [`Site`] combines site structure loading and page rendering:
//! - `index.md` files become section landing pages
//! - Other `.md` files become standalone pages
//! - Directories without `index.md` have their children promoted to parent level
//!
//! # Thread Safety
//!
//! `Site` is designed for concurrent access:
//! - `snapshot()` returns `Arc<SiteSnapshot>` with minimal locking (just Arc clone)
//! - `reload_if_needed()` uses double-checked locking for efficient cache validation
//! - `invalidate()` is lock-free (atomic flag)
//!
//! # Example
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use rw_site::{Site, PageRendererConfig};
//! use rw_cache::NullCache;
//! use rw_storage_fs::FsStorage;
//!
//! let storage = Arc::new(FsStorage::new(PathBuf::from("docs")));
//! let config = PageRendererConfig::default();
//! let cache = Arc::new(NullCache);
//! let site = Arc::new(Site::new(storage, cache, config));
//!
//! // Load site structure
//! let nav = site.navigation("");
//!
//! // Render a page
//! let result = site.render("guide")?;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use crate::page::{
    BreadcrumbItem, PageRenderResult, PageRenderer, PageRendererConfig, RenderError,
};
use crate::site_state::{Navigation, SiteState, SiteStateBuilder};
use crate::typed_page_registry::TypedPageRegistry;
use rw_cache::{Cache, CacheBucket};
use rw_diagrams::{EntityInfo, MetaIncludeSource};
use rw_storage::Storage;

/// Get the depth of a URL path.
///
/// Examples:
/// - `""` -> 0 (root)
/// - `"guide"` -> 1
/// - `"domain/billing"` -> 2
fn url_depth(path: &str) -> usize {
    if path.is_empty() {
        0
    } else {
        path.matches('/').count() + 1
    }
}

/// Bundled site state and typed page registry.
///
/// Ensures `SiteState` and `TypedPageRegistry` are always consistent —
/// they are built together and swapped atomically as a single `Arc`.
pub(crate) struct SiteSnapshot {
    pub(crate) state: SiteState,
    registry: TypedPageRegistry,
}

impl MetaIncludeSource for SiteSnapshot {
    fn get_entity(&self, entity_type: &str, name: &str) -> Option<EntityInfo> {
        self.registry.get_entity(entity_type, name)
    }
}

/// Unified site structure and page rendering.
///
/// Combines site structure loading from a [`Storage`] implementation with
/// page rendering functionality. Uses `index.md` files as section landing pages.
/// Titles are provided by the storage (extracted or stored depending on backend).
///
/// # Thread Safety
///
/// This struct is designed for concurrent access without external locking:
/// - Uses internal `RwLock<Arc<SiteSnapshot>>` for the current site snapshot
/// - Uses `Mutex<()>` for serializing reload operations
/// - Uses `AtomicBool` for cache validity tracking
pub struct Site {
    storage: Arc<dyn Storage>,
    // Buckets
    #[allow(clippy::struct_field_names)]
    site_bucket: Box<dyn CacheBucket>,
    /// Generation counter for site structure etag.
    generation: AtomicU64,
    /// Mutex for serializing reload operations.
    reload_lock: Mutex<()>,
    /// Current site snapshot (atomically swappable).
    current_snapshot: RwLock<Arc<SiteSnapshot>>,
    /// Cache validity flag.
    cache_valid: AtomicBool,
    /// Page rendering pipeline.
    renderer: PageRenderer,
}

impl Site {
    /// Create a new site with storage, configuration, and cache.
    ///
    /// # Arguments
    ///
    /// * `storage` - Storage implementation for document scanning and reading
    /// * `cache` - Cache implementation for site structure, pages, and diagrams
    /// * `config` - Page renderer configuration
    #[must_use]
    pub fn new(
        storage: Arc<dyn Storage>,
        cache: Arc<dyn Cache>,
        config: PageRendererConfig,
    ) -> Self {
        let initial_state = SiteStateBuilder::new().build();
        let initial_registry = TypedPageRegistry::from_site_state(&initial_state);
        let initial_snapshot = Arc::new(SiteSnapshot {
            state: initial_state,
            registry: initial_registry,
        });
        let site_bucket = cache.bucket("site");
        let renderer = PageRenderer::new(Arc::clone(&storage), cache, config);

        Self {
            storage,
            site_bucket,
            generation: AtomicU64::new(0),
            reload_lock: Mutex::new(()),
            current_snapshot: RwLock::new(initial_snapshot),
            cache_valid: AtomicBool::new(false),
            renderer,
        }
    }

    fn snapshot(&self) -> Arc<SiteSnapshot> {
        Arc::clone(&self.current_snapshot.read().unwrap())
    }

    /// Get scoped navigation tree.
    ///
    /// Reloads site if needed and returns navigation scoped to the specified section.
    ///
    /// # Arguments
    ///
    /// * `scope_path` - Path to scope (without leading slash), empty for root scope.
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    #[must_use]
    pub fn navigation(&self, scope_path: &str) -> Navigation {
        self.reload_if_needed().state.navigation(scope_path)
    }

    /// Get navigation scope for a page.
    ///
    /// Reloads site if needed and returns the scope path for the given page.
    ///
    /// # Arguments
    ///
    /// * `page_path` - URL path without leading slash.
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    #[must_use]
    pub fn get_navigation_scope(&self, page_path: &str) -> String {
        self.reload_if_needed()
            .state
            .get_navigation_scope(page_path)
    }

    /// Check if a page exists at the given URL path.
    ///
    /// Reloads site if needed and returns whether the page exists.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path without leading slash (e.g., "guide", "domain/page", "" for root)
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    #[must_use]
    pub fn has_page(&self, path: &str) -> bool {
        self.reload_if_needed().state.get_page(path).is_some()
    }

    /// Get breadcrumbs for a page.
    ///
    /// Reloads site if needed and returns breadcrumb navigation items
    /// for a given URL path.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path without leading slash (e.g., "guide/setup", "" for root)
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    #[must_use]
    pub fn get_breadcrumbs(&self, path: &str) -> Vec<BreadcrumbItem> {
        self.reload_if_needed().state.get_breadcrumbs(path)
    }

    /// Reload site from storage if cache is invalid.
    ///
    /// Uses double-checked locking pattern:
    /// 1. Fast path: return current snapshot if cache valid
    /// 2. Slow path: acquire `reload_lock`, recheck, then reload
    ///
    /// # Returns
    ///
    /// `Arc<SiteSnapshot>` containing the current site snapshot.
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    pub(crate) fn reload_if_needed(&self) -> Arc<SiteSnapshot> {
        // Fast path: cache valid
        if self.cache_valid.load(Ordering::Acquire) {
            return self.snapshot();
        }

        // Slow path: acquire reload lock
        let _guard = self.reload_lock.lock().unwrap();

        // Double-check after acquiring lock
        if self.cache_valid.load(Ordering::Acquire) {
            return self.snapshot();
        }

        let etag = self.generation.load(Ordering::Acquire).to_string();

        // Load state from bucket cache or storage
        let state = if let Some(cached) = SiteState::from_cache(self.site_bucket.as_ref(), &etag) {
            cached
        } else {
            let state = self.load_from_storage();
            state.to_cache(self.site_bucket.as_ref(), &etag);
            state
        };

        // Build registry and bundle into snapshot
        let registry =
            TypedPageRegistry::from_site_state_with_storage(&state, self.storage.as_ref());
        let snapshot = Arc::new(SiteSnapshot { state, registry });

        *self.current_snapshot.write().unwrap() = Arc::clone(&snapshot);
        self.cache_valid.store(true, Ordering::Release);

        snapshot
    }

    /// Invalidate cached site state.
    ///
    /// Marks cache as invalid and bumps the generation counter so the
    /// old site bucket entry won't match. Next `reload_if_needed()` will reload.
    /// Current readers continue using their existing `Arc<SiteSnapshot>`.
    pub fn invalidate(&self) {
        self.cache_valid.store(false, Ordering::Release);
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Render a page by URL path.
    ///
    /// Reloads site if needed, looks up the page, and renders it.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path without leading slash (e.g., "guide", "domain/page", "" for root)
    ///
    /// # Returns
    ///
    /// `PageRenderResult` with HTML, title, table of contents, and metadata.
    ///
    /// # Errors
    ///
    /// Returns `RenderError::PageNotFound` if page doesn't exist in site.
    /// Returns `RenderError::FileNotFound` if source file doesn't exist.
    /// Returns `RenderError::Io` if file cannot be read.
    pub fn render(&self, path: &str) -> Result<PageRenderResult, RenderError> {
        let snapshot = self.reload_if_needed();
        let page = snapshot
            .state
            .get_page(path)
            .ok_or_else(|| RenderError::PageNotFound(path.to_owned()))?;
        let breadcrumbs = snapshot.state.get_breadcrumbs(path);
        let meta = Arc::clone(&snapshot) as Arc<dyn MetaIncludeSource>;
        self.renderer.render(path, page, breadcrumbs, Some(meta))
    }

    /// Load site state from storage and build hierarchy.
    ///
    /// Uses `storage.scan()` to get documents (including virtual pages), then builds
    /// hierarchy based on path conventions. Virtual pages are identified by
    /// `has_content=false` flag.
    ///
    /// Page titles are determined by:
    /// 1. Metadata title from storage (if page has `page_type`)
    /// 2. Document title from storage (extracted from H1 or filename)
    fn load_from_storage(&self) -> SiteState {
        let mut builder = SiteStateBuilder::new();

        // Scan storage for documents (including virtual pages)
        let mut documents = match self.storage.scan() {
            Ok(docs) => docs,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to scan storage");
                return builder.build();
            }
        };

        // Sort documents: parents before children, real pages before virtual, by path
        documents.sort_by(|a, b| {
            url_depth(&a.path)
                .cmp(&url_depth(&b.path))
                .then_with(|| a.has_content.cmp(&b.has_content).reverse())
                .then_with(|| a.path.cmp(&b.path))
        });

        if documents.is_empty() {
            return builder.build();
        }

        // Track URL paths to page indices for parent lookup
        let mut url_to_idx: HashMap<String, usize> = HashMap::new();

        // Process documents in sorted order
        for doc in &documents {
            let parent_idx = Self::find_parent_from_url(&doc.path, &url_to_idx);

            let idx = builder.add_page(
                doc.title.clone(),
                doc.path.clone(),
                doc.has_content,
                parent_idx,
                doc.page_type.as_deref(),
            );
            url_to_idx.insert(doc.path.clone(), idx);
        }

        builder.build()
    }

    /// Find parent page index from URL path.
    ///
    /// Walks up the path hierarchy to find the nearest existing ancestor.
    fn find_parent_from_url(url_path: &str, url_to_idx: &HashMap<String, usize>) -> Option<usize> {
        let mut current = url_path;
        while !current.is_empty() {
            let parent_url = current.rsplit_once('/').map_or("", |(parent, _)| parent);
            if let Some(&idx) = url_to_idx.get(parent_url) {
                return Some(idx);
            }
            current = parent_url;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    // Ensure Site is Send + Sync for use with Arc
    static_assertions::assert_impl_all!(super::Site: Send, Sync);

    use std::sync::Arc;

    use rw_storage::MockStorage;

    use super::*;

    fn create_site_with_storage(storage: MockStorage) -> Site {
        let config = PageRendererConfig::default();
        Site::new(Arc::new(storage), Arc::new(rw_cache::NullCache), config)
    }

    // ========================================================================
    // Site structure tests
    // ========================================================================

    #[test]
    fn test_reload_if_needed_empty_storage_returns_empty_site() {
        let storage = MockStorage::new();
        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        assert!(snapshot.state.get_root_pages().is_empty());
    }

    #[test]
    fn test_reload_if_needed_flat_structure_builds_site() {
        let storage = MockStorage::new()
            .with_document("guide", "User Guide")
            .with_document("api", "API Reference");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        assert_eq!(snapshot.state.get_root_pages().len(), 2);
        assert!(snapshot.state.get_page("guide").is_some());
        assert!(snapshot.state.get_page("api").is_some());
    }

    #[test]
    fn test_reload_if_needed_root_index_adds_home_page() {
        let storage =
            MockStorage::new().with_file("", "Welcome", "# Welcome\n\nHome page content.");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        let page = snapshot.state.get_page("");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Welcome");
        assert_eq!(page.path, "");
        assert!(page.has_content);
    }

    #[test]
    fn test_reload_if_needed_nested_structure_builds_site() {
        let storage = MockStorage::new()
            .with_file("domain-a", "Domain A", "# Domain A\n\nOverview.")
            .with_file("domain-a/guide", "Setup Guide", "# Setup Guide\n\nSteps.");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        let domain = snapshot.state.get_page("domain-a");
        assert!(domain.is_some());
        let domain = domain.unwrap();
        assert_eq!(domain.title, "Domain A");
        assert!(domain.has_content);

        // Verify child via root navigation (non-section pages expand their children)
        let nav = snapshot.state.navigation("");
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].path, "domain-a");
        assert_eq!(nav.items[0].children.len(), 1);
        assert_eq!(nav.items[0].children[0].title, "Setup Guide");

        // Verify child page details
        let child = snapshot.state.get_page("domain-a/guide").unwrap();
        assert!(child.has_content);
    }

    #[test]
    fn test_reload_if_needed_page_titles_from_storage() {
        let storage = MockStorage::new().with_document("guide", "My Custom Title");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        let page = snapshot.state.get_page("guide");
        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "My Custom Title");
    }

    #[test]
    fn test_reload_if_needed_cyrillic_path() {
        let storage = MockStorage::new().with_document("руководство", "Руководство");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        let page = snapshot.state.get_page("руководство");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Руководство");
        assert_eq!(page.path, "руководство");
        assert!(page.has_content);
    }

    #[test]
    fn test_reload_if_needed_directory_without_index_promotes_children() {
        // MockStorage simulates child promotion by just providing the child at path
        let storage = MockStorage::new().with_document("no-index/child", "Child Page");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        // Child should be at root level (promoted)
        let roots = snapshot.state.get_root_pages();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "no-index/child");
        assert!(roots[0].has_content);
    }

    #[test]
    fn test_snapshot_returns_same_arc() {
        let storage = MockStorage::new().with_document("guide", "Guide");

        let site = create_site_with_storage(storage);

        // First reload to populate
        let _ = site.reload_if_needed();

        // snapshot() should return the same Arc
        let snapshot1 = site.snapshot();
        let snapshot2 = site.snapshot();

        assert!(Arc::ptr_eq(&snapshot1, &snapshot2));
    }

    #[test]
    fn test_reload_if_needed_caches_result() {
        let storage = MockStorage::new().with_document("guide", "Guide");

        let site = create_site_with_storage(storage);

        let state1 = site.reload_if_needed();
        let state2 = site.reload_if_needed();

        // Should return the same Arc (cached)
        assert!(Arc::ptr_eq(&state1, &state2));
    }

    #[test]
    fn test_invalidate_clears_cached_state() {
        let storage = MockStorage::new().with_document("guide", "Guide");

        let site = create_site_with_storage(storage);

        // First reload
        let snapshot1 = site.reload_if_needed();
        assert!(snapshot1.state.get_page("guide").is_some());

        // Invalidate cache
        site.invalidate();

        // Second reload - should be a different Arc
        let snapshot2 = site.reload_if_needed();
        assert!(!Arc::ptr_eq(&snapshot1, &snapshot2));
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let storage = MockStorage::new().with_document("guide", "Guide");

        let site = Arc::new(create_site_with_storage(storage));

        // Spawn multiple threads accessing concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let site = Arc::clone(&site);
                thread::spawn(move || {
                    let snapshot = site.reload_if_needed();
                    assert!(snapshot.state.get_page("guide").is_some());
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

        let storage = MockStorage::new().with_document("guide", "Guide");

        let site = Arc::new(create_site_with_storage(storage));

        // Initial load
        let _ = site.reload_if_needed();

        // Spawn threads that invalidate and reload concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let site = Arc::clone(&site);
                thread::spawn(move || {
                    if i % 2 == 0 {
                        site.invalidate();
                    } else {
                        let snapshot = site.reload_if_needed();
                        // Site should always be valid
                        assert!(snapshot.state.get_page("guide").is_some());
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Final state should be valid
        let snapshot = site.reload_if_needed();
        assert!(snapshot.state.get_page("guide").is_some());
    }

    #[test]
    fn test_nested_hierarchy_with_multiple_levels() {
        let storage = MockStorage::new()
            .with_file("", "Home", "# Home")
            .with_file("level1", "Level 1", "# Level 1")
            .with_file("level1/level2", "Level 2", "# Level 2")
            .with_file("level1/level2/page", "Deep Page", "# Deep Page");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        // Check root
        let root = snapshot.state.get_page("").unwrap();
        assert_eq!(root.title, "Home");

        // Check level 1
        let level1 = snapshot.state.get_page("level1").unwrap();
        assert_eq!(level1.title, "Level 1");

        // Check level 2
        let level2 = snapshot.state.get_page("level1/level2").unwrap();
        assert_eq!(level2.title, "Level 2");

        // Check deep page
        let deep = snapshot.state.get_page("level1/level2/page").unwrap();
        assert_eq!(deep.title, "Deep Page");

        // Verify nested hierarchy via root navigation (non-section pages expand children)
        let root_nav = snapshot.state.navigation("");
        assert_eq!(root_nav.items.len(), 1);
        assert_eq!(root_nav.items[0].path, "level1");
        // level1 contains level2
        assert_eq!(root_nav.items[0].children.len(), 1);
        assert_eq!(root_nav.items[0].children[0].path, "level1/level2");
        // level2 contains deep page
        assert_eq!(root_nav.items[0].children[0].children.len(), 1);
        assert_eq!(
            root_nav.items[0].children[0].children[0].path,
            "level1/level2/page"
        );
    }

    // ========================================================================
    // Rendering tests
    // ========================================================================

    #[test]
    fn test_render_simple_markdown() {
        let storage = MockStorage::new()
            .with_file("test", "Hello", "# Hello\n\nWorld")
            .with_mtime("test", 1000.0);

        let config = PageRendererConfig {
            extract_title: true,
            ..Default::default()
        };
        let site = Site::new(Arc::new(storage), Arc::new(rw_cache::NullCache), config);

        let result = site.render("test").unwrap();
        assert!(result.html.contains("<p>World</p>"));
        assert_eq!(result.title, Some("Hello".to_owned()));
        assert!(!result.from_cache);
        assert!(result.has_content);
    }

    #[test]
    fn test_render_page_not_found() {
        let storage = MockStorage::new().with_document("exists", "Exists");

        let site = create_site_with_storage(storage);

        let result = site.render("nonexistent");
        assert!(matches!(result, Err(RenderError::PageNotFound(_))));
    }

    #[test]
    fn test_render_with_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let storage = MockStorage::new()
            .with_file("test", "Cached", "# Cached\n\nContent")
            .with_mtime("test", 1000.0);

        let cache: Arc<dyn rw_cache::Cache> =
            Arc::new(rw_cache::FileCache::new(cache_dir, "1.0.0"));
        let config = PageRendererConfig {
            extract_title: true,
            ..Default::default()
        };
        let site = Site::new(Arc::new(storage), cache, config);

        // First render - cache miss
        let result1 = site.render("test").unwrap();
        assert!(!result1.from_cache);
        assert_eq!(result1.title, Some("Cached".to_owned()));

        // Second render - cache hit
        let result2 = site.render("test").unwrap();
        assert!(result2.from_cache);
        assert_eq!(result2.title, Some("Cached".to_owned()));
        assert_eq!(result1.html, result2.html);
    }

    #[test]
    fn test_render_includes_breadcrumbs() {
        let storage = MockStorage::new()
            .with_file("", "Home", "# Home")
            .with_file("domain", "Domain", "# Domain")
            .with_file("domain/page", "Page", "# Page")
            .with_mtime("domain/page", 1000.0);

        let site = create_site_with_storage(storage);

        let result = site.render("domain/page").unwrap();

        assert_eq!(result.breadcrumbs.len(), 2);
        assert_eq!(result.breadcrumbs[0].title, "Home");
        assert_eq!(result.breadcrumbs[0].path, "");
        assert_eq!(result.breadcrumbs[1].title, "Domain");
        assert_eq!(result.breadcrumbs[1].path, "domain");
    }

    #[test]
    fn test_render_toc_generation() {
        let storage = MockStorage::new()
            .with_file("test", "Title", "# Title\n\n## Section 1\n\n## Section 2")
            .with_mtime("test", 1000.0);

        let site = create_site_with_storage(storage);

        let result = site.render("test").unwrap();
        assert_eq!(result.toc.len(), 2);
        assert_eq!(result.toc[0].title, "Section 1");
        assert_eq!(result.toc[0].level, 2);
        assert_eq!(result.toc[1].title, "Section 2");
    }

    // ========================================================================
    // Virtual page tests
    // ========================================================================

    #[test]
    fn test_virtual_page_discovered_from_storage() {
        let storage =
            MockStorage::new().with_virtual_page_and_type("my-domain", "My Domain", "domain");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        let page = snapshot.state.get_page("my-domain");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "My Domain");
        assert!(!page.has_content); // Virtual page

        // page_type is tracked via sections map
        let section = snapshot.state.sections().get("my-domain");
        assert!(section.is_some());
        assert_eq!(section.unwrap().section_type, "domain");
    }

    #[test]
    fn test_real_page_with_type() {
        // Has both content and page_type
        let storage =
            MockStorage::new().with_document_and_type("real-domain", "Meta Title", "domain");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        let page = snapshot.state.get_page("real-domain");
        assert!(page.is_some());
        let page = page.unwrap();
        // Should have content
        assert!(page.has_content);
        // Title from storage
        assert_eq!(page.title, "Meta Title");
    }

    #[test]
    fn test_virtual_page_renders_title_only() {
        let storage = MockStorage::new()
            .with_virtual_page_and_type("my-domain", "My Domain", "domain")
            .with_mtime("my-domain", 1000.0)
            .with_document("my-domain/child1", "Child One")
            .with_document("my-domain/child2", "Child Two");

        let site = create_site_with_storage(storage);

        let result = site.render("my-domain").unwrap();

        // Virtual pages render h1 with title only
        assert_eq!(result.html, "<h1>My Domain</h1>\n");
        assert_eq!(result.title, Some("My Domain".to_owned()));
        assert!(!result.has_content); // Virtual
        assert!(result.toc.is_empty()); // No TOC for virtual
    }

    #[test]
    fn test_virtual_page_in_navigation() {
        let storage = MockStorage::new()
            .with_virtual_page_and_type("my-domain", "My Domain", "domain")
            .with_document("my-domain/child", "Child Page");

        let site = create_site_with_storage(storage);

        let nav = site.navigation("");

        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].title, "My Domain");
        assert_eq!(nav.items[0].path, "my-domain");
        // Section is a leaf in root scope (scoped navigation)
        assert!(nav.items[0].children.is_empty());
    }

    #[test]
    fn test_nested_virtual_pages() {
        let storage = MockStorage::new()
            .with_file("", "Home", "# Home")
            // Parent virtual page
            .with_virtual_page_and_type("domains", "Domains", "section")
            // Nested virtual page
            .with_virtual_page_and_type("domains/billing", "Billing", "domain")
            // Real page in nested virtual
            .with_document("domains/billing/overview", "Overview");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed();

        // Check parent virtual
        let domains = snapshot.state.get_page("domains");
        assert!(domains.is_some());
        assert!(!domains.unwrap().has_content);

        // Check child virtual
        let billing = snapshot.state.get_page("domains/billing");
        assert!(billing.is_some());
        assert!(!billing.unwrap().has_content);

        // Check real page has correct parent
        let overview = snapshot.state.get_page("domains/billing/overview");
        assert!(overview.is_some());
        assert!(overview.unwrap().has_content);

        // Check navigation structure via scoped navigation
        // Domains section in root scope
        let root_nav = site.navigation("");
        assert_eq!(root_nav.items.len(), 1);
        assert_eq!(root_nav.items[0].title, "Domains");
        // Sections are leaves in root scope
        assert!(root_nav.items[0].children.is_empty());

        // Navigate into Domains section
        let domains_nav = site.navigation("domains");
        assert_eq!(domains_nav.items.len(), 1);
        assert_eq!(domains_nav.items[0].title, "Billing");
        // Billing is also a section, so it's a leaf in domains scope
        assert!(domains_nav.items[0].children.is_empty());

        // Navigate into Billing section
        let billing_nav = site.navigation("domains/billing");
        assert_eq!(billing_nav.items.len(), 1);
        assert_eq!(billing_nav.items[0].title, "Overview");
    }

    // ========================================================================
    // Cache version tests
    // ========================================================================

    #[test]
    fn test_version_change_wipes_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let storage = Arc::new(
            MockStorage::new()
                .with_file("test", "Test", "# Test\n\nContent")
                .with_mtime("test", 1000.0),
        ) as Arc<dyn rw_storage::Storage>;

        // First run with version 1.0.0 — render to populate cache
        let cache_v1: Arc<dyn rw_cache::Cache> =
            Arc::new(rw_cache::FileCache::new(cache_dir.clone(), "1.0.0"));
        let site_v1 = Site::new(
            Arc::clone(&storage),
            cache_v1,
            PageRendererConfig {
                extract_title: true,
                ..Default::default()
            },
        );
        let result1 = site_v1.render("test").unwrap();
        assert!(!result1.from_cache);

        // Verify cache is populated
        let result1b = site_v1.render("test").unwrap();
        assert!(result1b.from_cache);

        // Second run with version 2.0.0 — cache should be wiped
        let cache_v2: Arc<dyn rw_cache::Cache> =
            Arc::new(rw_cache::FileCache::new(cache_dir.clone(), "2.0.0"));
        let site_v2 = Site::new(
            Arc::clone(&storage),
            cache_v2,
            PageRendererConfig {
                extract_title: true,
                ..Default::default()
            },
        );

        // VERSION file should be updated
        assert_eq!(
            std::fs::read_to_string(cache_dir.join("VERSION")).unwrap(),
            "2.0.0"
        );

        // First render with new version should be a cache miss
        let result2 = site_v2.render("test").unwrap();
        assert!(!result2.from_cache);
    }
}
