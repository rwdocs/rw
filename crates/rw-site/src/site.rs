//! Site loading, caching, and rendering orchestration.
//!
//! This module provides [`Site`], the main entry point for the crate. See
//! the [crate-level docs](crate) for an overview of sections, virtual pages,
//! and the lazy reload pattern.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use crate::page::{
    BreadcrumbItem, Page, PageRenderResult, PageRenderer, PageRendererConfig, RenderContext,
    RenderError, SearchDocument,
};
use crate::site_state::{Navigation, SiteState, SiteStateBuilder};
use rw_cache::{Cache, CacheBucket};
use rw_diagrams::{EntityInfo, MetaIncludeSource};
use rw_renderer::TitleResolver;
use rw_sections::Sections;
use rw_storage::{Storage, StorageError};

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

/// Bundled site state for atomic swaps.
///
/// Wraps `SiteState` and implements `MetaIncludeSource` for diagram
/// include resolution using the state's name-based section index.
pub(crate) struct SiteSnapshot {
    pub(crate) state: SiteState,
    pub(crate) sections: Arc<Sections>,
}

impl MetaIncludeSource for SiteSnapshot {
    fn get_entity(&self, entity_type: &str, name: &str) -> Option<EntityInfo> {
        let raw_name = name.replace('_', "-");
        let (section_path, _section) = self
            .state
            .find_sections_by_name(&raw_name)
            .into_iter()
            .find(|(_, s)| s.kind == entity_type)?;

        let page = self.state.get_page(section_path);
        let has_content = page.is_some_and(|p| p.has_content);

        let title = if entity_type == "service" {
            raw_name
        } else {
            page.map_or_else(|| section_path.to_owned(), |p| p.title.clone())
        };

        Some(EntityInfo {
            title,
            description: page.and_then(|p| p.description.clone()),
            url_path: has_content.then(|| format!("/{section_path}")),
        })
    }
}

/// Resolves page paths to titles using the site snapshot.
///
/// Owns an `Arc<SiteSnapshot>` so it satisfies the `'static` bound
/// required by `Box<dyn TitleResolver>`.
pub(crate) struct SiteTitleResolver {
    pub(crate) snapshot: Arc<SiteSnapshot>,
}

impl TitleResolver for SiteTitleResolver {
    fn resolve_title(&self, path: &str) -> Option<String> {
        let page = self.snapshot.state.get_page(path)?;
        Some(page.title.clone())
    }
}

/// Manages the document hierarchy and renders pages on demand.
///
/// `Site` scans documents from a [`Storage`] backend, builds a tree of
/// parent/child page relationships, and renders markdown to HTML through
/// an internal page renderer. Results are cached so repeated requests
/// for the same page are fast.
///
/// Callers typically wrap `Site` in `Arc` and share it across threads.
/// All public methods use internal synchronization — no external locking
/// is required.
///
/// # Lazy reload
///
/// `Site` does not re-scan storage on every request. Call
/// [`invalidate`](Self::invalidate) to mark the cached site structure as
/// stale; the next read method ([`navigation`](Self::navigation),
/// [`render`](Self::render), etc.) will reload from storage before
/// returning. Concurrent readers see the previous snapshot until the
/// reload completes.
///
/// # Examples
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use std::sync::Arc;
/// use std::path::PathBuf;
/// use rw_site::{Site, PageRendererConfig};
/// use rw_cache::NullCache;
/// use rw_storage_fs::FsStorage;
///
/// let storage = Arc::new(FsStorage::new(PathBuf::from("docs")));
/// let site = Arc::new(Site::new(
///     storage,
///     Arc::new(NullCache),
///     PageRendererConfig::default(),
/// ));
///
/// // First call triggers a scan of the storage backend
/// let nav = site.navigation(None)?;
///
/// // Render a page — cached on second call if the source file hasn't changed
/// let result = site.render("guide")?;
/// assert!(result.has_content);
/// # Ok(())
/// # }
/// ```
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
    /// Whether the site has successfully loaded at least once.
    has_loaded: AtomicBool,
    /// Page rendering pipeline.
    renderer: PageRenderer,
}

impl Site {
    /// Creates a new site backed by the given storage and cache.
    ///
    /// The site starts empty — no storage scan happens until the first read
    /// method is called. Pass [`NullCache`](rw_cache::NullCache) to disable
    /// caching entirely.
    #[must_use]
    pub fn new(
        storage: Arc<dyn Storage>,
        cache: Arc<dyn Cache>,
        config: PageRendererConfig,
    ) -> Self {
        let initial_state = SiteStateBuilder::new().build();
        let initial_snapshot = Arc::new(SiteSnapshot {
            state: initial_state,
            sections: Arc::new(Sections::default()),
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
            has_loaded: AtomicBool::new(false),
            renderer,
        }
    }

    fn snapshot(&self) -> Arc<SiteSnapshot> {
        Arc::clone(&self.current_snapshot.read().unwrap())
    }

    /// Returns the navigation tree scoped to a section.
    ///
    /// Pass `None` for root navigation, or a
    /// [section ref](crate#sections-and-scoped-navigation) (e.g.,
    /// `"domain:default/billing"`) to get that section's children.
    /// Triggers a reload on first call or after [`invalidate`](Self::invalidate).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the initial site load fails (storage
    /// unreachable). Subsequent reload failures are logged and stale data
    /// is returned instead.
    pub fn navigation(&self, section_ref: Option<&str>) -> Result<Navigation, StorageError> {
        let snapshot = self.reload_if_needed()?;
        let scope_path = section_ref
            .and_then(|r| snapshot.sections.find_by_ref(r).map(str::to_owned))
            .unwrap_or_default();
        let mut nav = snapshot.state.navigation(&scope_path);
        nav.apply_sections(&snapshot.sections);
        Ok(nav)
    }

    /// Returns the current [`Sections`] map for cross-section link resolution.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the initial site load fails.
    pub fn sections(&self) -> Result<Arc<Sections>, StorageError> {
        let snapshot = self.reload_if_needed()?;
        Ok(Arc::clone(&snapshot.sections))
    }

    /// Returns the [section ref](crate#sections-and-scoped-navigation) for
    /// the section that contains `page_path`.
    ///
    /// Walks up the path hierarchy to find the nearest section ancestor.
    /// Falls back to the implicit root section (`"section:default/root"`)
    /// when no explicit section is found.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the initial site load fails.
    pub fn get_section_ref(&self, page_path: &str) -> Result<String, StorageError> {
        Ok(self.reload_if_needed()?.state.get_section_ref(page_path))
    }

    /// Returns `true` if a page exists at `path` in the site structure.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the initial site load fails.
    pub fn has_page(&self, path: &str) -> Result<bool, StorageError> {
        Ok(self.reload_if_needed()?.state.get_page(path).is_some())
    }

    /// Returns the title of a page from the current cached snapshot, or
    /// `None` if the page does not exist.
    ///
    /// Unlike other methods, this does **not** trigger a reload — it reads
    /// whatever snapshot is currently in memory. This makes it suitable for
    /// use in tight loops (e.g., resolving wikilink display text) where
    /// staleness is acceptable.
    #[must_use]
    pub fn page_title(&self, path: &str) -> Option<String> {
        self.snapshot()
            .state
            .get_page(path)
            .map(|p| p.title.clone())
    }

    /// Returns the `pages` ordering for a page from the current cached snapshot,
    /// or `None` if the page does not exist or has no ordering.
    ///
    /// Like [`page_title`](Self::page_title), does **not** trigger a reload.
    #[must_use]
    pub fn page_pages(&self, path: &str) -> Option<Vec<String>> {
        self.snapshot()
            .state
            .get_page(path)
            .and_then(|p| p.pages.clone())
    }

    /// Returns the [`BreadcrumbItem`] trail for a page.
    ///
    /// The trail starts with "Home" (path `""`) and includes each ancestor
    /// up to but not including the page itself. Returns an empty `Vec` for
    /// the root page.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the initial site load fails.
    pub fn get_breadcrumbs(&self, path: &str) -> Result<Vec<BreadcrumbItem>, StorageError> {
        Ok(self.reload_if_needed()?.state.get_breadcrumbs(path))
    }

    /// Returns the current snapshot, reloading from storage if stale.
    ///
    /// Uses a double-checked locking pattern: the fast path (cache valid)
    /// requires only an atomic load and an `RwLock` read; the slow path
    /// acquires a `Mutex` to serialize reloads.
    ///
    /// On the **initial** load, storage errors propagate to the caller so
    /// that misconfigured storage is surfaced immediately. On subsequent
    /// reloads, errors are logged and the previous snapshot is kept.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the initial site load fails.
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    pub(crate) fn reload_if_needed(&self) -> Result<Arc<SiteSnapshot>, StorageError> {
        // Fast path: cache valid
        if self.cache_valid.load(Ordering::Acquire) {
            return Ok(self.snapshot());
        }

        // Slow path: acquire reload lock
        let _guard = self.reload_lock.lock().unwrap();

        // Double-check after acquiring lock
        if self.cache_valid.load(Ordering::Acquire) {
            return Ok(self.snapshot());
        }

        let has_loaded = self.has_loaded.load(Ordering::Acquire);
        let etag = self.generation.load(Ordering::Acquire).to_string();

        // Load state: skip bucket cache on initial load to verify storage connectivity
        let state = if has_loaded {
            if let Some(cached) = SiteState::from_cache(self.site_bucket.as_ref(), &etag) {
                cached
            } else {
                match self.load_from_storage() {
                    Ok(state) => {
                        state.to_cache(self.site_bucket.as_ref(), &etag);
                        state
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to reload site from storage, keeping stale data");
                        return Ok(self.snapshot());
                    }
                }
            }
        } else {
            let state = self.load_from_storage()?;
            state.to_cache(self.site_bucket.as_ref(), &etag);
            state
        };

        let sections = state.build_sections();
        let snapshot = Arc::new(SiteSnapshot { state, sections });

        *self.current_snapshot.write().unwrap() = Arc::clone(&snapshot);
        self.cache_valid.store(true, Ordering::Release);
        self.has_loaded.store(true, Ordering::Release);

        Ok(snapshot)
    }

    /// Reloads the site, optionally checking for changes first.
    ///
    /// - `reload(true)` — unconditional reload. Always returns `Ok(true)`.
    /// - `reload(false)` — checks [`Storage::has_changed()`] first.
    ///   Returns `Ok(false)` if the backend reports no changes.
    ///   Returns `Ok(true)` if a reload was attempted.
    ///
    /// `Ok(true)` means "reload was attempted," not "new content was loaded."
    /// If the storage scan fails after `has_changed()` returns `true`,
    /// the site keeps stale data (existing behavior of
    /// [`reload_if_needed`](Self::reload_if_needed)).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if `has_changed()` fails (e.g., S3 connectivity)
    /// or if this is the first load and storage is unreachable.
    pub fn reload(&self, force: bool) -> Result<bool, StorageError> {
        if !force && !self.storage.has_changed()? {
            return Ok(false);
        }
        self.renderer.clear_pages();
        self.invalidate();
        self.reload_if_needed()?;
        Ok(true)
    }

    /// Marks the cached site structure as stale.
    ///
    /// The next call to any read method ([`navigation`](Self::navigation),
    /// [`render`](Self::render), etc.) will re-scan storage before returning.
    /// Readers that already hold a snapshot are unaffected — they continue
    /// using the previous data. This method is lock-free.
    pub fn invalidate(&self) {
        self.cache_valid.store(false, Ordering::Release);
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Renders a page to HTML by its URL path.
    ///
    /// Looks up the page in the site structure, computes breadcrumbs, and
    /// runs the markdown rendering pipeline (or returns a cached result if
    /// the source file has not changed).
    ///
    /// `path` is a URL path without leading slash (e.g., `"guide"`,
    /// `"domain/billing"`, `""` for root).
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::PageNotFound`] if no page with this path
    /// exists in the site structure.
    /// Returns [`RenderError::FileNotFound`] if the page exists but its
    /// markdown source is missing from storage.
    /// Returns [`RenderError::Storage`] if the storage backend itself fails.
    pub fn render(&self, path: &str) -> Result<PageRenderResult, RenderError> {
        let snapshot = self.reload_if_needed().map_err(RenderError::Storage)?;
        let page = snapshot
            .state
            .get_page(path)
            .ok_or_else(|| RenderError::PageNotFound(path.to_owned()))?;
        let breadcrumbs = snapshot.state.get_breadcrumbs(path);
        let ctx = Self::render_context(&snapshot);
        self.renderer.render(path, page, breadcrumbs, &ctx)
    }

    /// Render a page as plain text for search indexing.
    ///
    /// Returns `None` for virtual pages (directories without content).
    /// Does not cache results or make network calls (no Kroki, no syntax highlighting).
    ///
    /// # Errors
    ///
    /// Same error conditions as [`render()`](Self::render).
    pub fn render_search_document(
        &self,
        path: &str,
    ) -> Result<Option<SearchDocument>, RenderError> {
        let snapshot = self.reload_if_needed().map_err(RenderError::Storage)?;
        let page = snapshot
            .state
            .get_page(path)
            .ok_or_else(|| RenderError::PageNotFound(path.to_owned()))?;
        let ctx = Self::render_context(&snapshot);
        self.renderer.render_search_document(path, page, &ctx)
    }

    fn render_context(snapshot: &Arc<SiteSnapshot>) -> RenderContext {
        RenderContext {
            sections: Arc::clone(&snapshot.sections),
            meta_include_source: Some(Arc::clone(snapshot) as Arc<dyn MetaIncludeSource>),
            snapshot: Some(Arc::clone(snapshot)),
        }
    }

    /// Load site state from storage and build hierarchy.
    ///
    /// Uses `storage.scan()` to get documents (including virtual pages), then builds
    /// hierarchy based on path conventions. Virtual pages are identified by
    /// `has_content=false` flag.
    ///
    /// Page titles are determined by:
    /// 1. Metadata title from storage (if page has `page_kind`)
    /// 2. Document title from storage (extracted from H1 or filename)
    fn load_from_storage(&self) -> Result<SiteState, StorageError> {
        let mut builder = SiteStateBuilder::new();
        let mut documents = self.storage.scan()?;

        // Sort documents: parents before children, real pages before virtual, by path
        documents.sort_by(|a, b| {
            url_depth(&a.path)
                .cmp(&url_depth(&b.path))
                .then_with(|| a.has_content.cmp(&b.has_content).reverse())
                .then_with(|| a.path.cmp(&b.path))
        });

        if documents.is_empty() {
            return Ok(builder.build());
        }

        // Track URL paths to page indices for parent lookup
        let mut url_to_idx: HashMap<String, usize> = HashMap::new();

        // Process documents in sorted order
        for doc in &documents {
            let parent_idx = Self::find_parent_from_url(&doc.path, &url_to_idx);

            let idx = builder.add_page(
                Page {
                    title: doc.title.clone(),
                    path: doc.path.clone(),
                    has_content: doc.has_content,
                    description: doc.description.clone(),
                    origin: doc.origin.clone(),
                    pages: doc.pages.clone(),
                },
                parent_idx,
                doc.page_kind.as_deref(),
            );
            url_to_idx.insert(doc.path.clone(), idx);
        }

        // Apply custom page ordering from `pages` metadata
        for doc in &documents {
            if let Some(pages) = &doc.pages
                && let Some(&idx) = url_to_idx.get(&doc.path)
            {
                builder.reorder_children(idx, pages);
            }
        }

        Ok(builder.build())
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

    use rw_storage::{MockStorage, StorageErrorKind};

    use super::*;
    use crate::page::RenderError;

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

        let snapshot = site.reload_if_needed().unwrap();

        assert!(snapshot.state.get_root_pages().is_empty());
    }

    #[test]
    fn test_reload_if_needed_flat_structure_builds_site() {
        let storage = MockStorage::new()
            .with_document("guide", "User Guide")
            .with_document("api", "API Reference");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed().unwrap();

        assert_eq!(snapshot.state.get_root_pages().len(), 2);
        assert!(snapshot.state.get_page("guide").is_some());
        assert!(snapshot.state.get_page("api").is_some());
    }

    #[test]
    fn test_reload_if_needed_root_index_adds_home_page() {
        let storage =
            MockStorage::new().with_file("", "Welcome", "# Welcome\n\nHome page content.");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed().unwrap();

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

        let snapshot = site.reload_if_needed().unwrap();

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

        let snapshot = site.reload_if_needed().unwrap();

        let page = snapshot.state.get_page("guide");
        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "My Custom Title");
    }

    #[test]
    fn test_reload_if_needed_cyrillic_path() {
        let storage = MockStorage::new().with_document("руководство", "Руководство");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed().unwrap();

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

        let snapshot = site.reload_if_needed().unwrap();

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
        let _ = site.reload_if_needed().unwrap();

        // snapshot() should return the same Arc
        let snapshot1 = site.snapshot();
        let snapshot2 = site.snapshot();

        assert!(Arc::ptr_eq(&snapshot1, &snapshot2));
    }

    #[test]
    fn test_reload_if_needed_caches_result() {
        let storage = MockStorage::new().with_document("guide", "Guide");

        let site = create_site_with_storage(storage);

        let state1 = site.reload_if_needed().unwrap();
        let state2 = site.reload_if_needed().unwrap();

        // Should return the same Arc (cached)
        assert!(Arc::ptr_eq(&state1, &state2));
    }

    #[test]
    fn test_invalidate_clears_cached_state() {
        let storage = MockStorage::new().with_document("guide", "Guide");

        let site = create_site_with_storage(storage);

        // First reload
        let snapshot1 = site.reload_if_needed().unwrap();
        assert!(snapshot1.state.get_page("guide").is_some());

        // Invalidate cache
        site.invalidate();

        // Second reload - should be a different Arc
        let snapshot2 = site.reload_if_needed().unwrap();
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
                    let snapshot = site.reload_if_needed().unwrap();
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
        let _ = site.reload_if_needed().unwrap();

        // Spawn threads that invalidate and reload concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let site = Arc::clone(&site);
                thread::spawn(move || {
                    if i % 2 == 0 {
                        site.invalidate();
                    } else {
                        let snapshot = site.reload_if_needed().unwrap();
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
        let snapshot = site.reload_if_needed().unwrap();
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

        let snapshot = site.reload_if_needed().unwrap();

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
            MockStorage::new().with_virtual_page_and_kind("my-domain", "My Domain", "domain");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed().unwrap();

        let page = snapshot.state.get_page("my-domain");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "My Domain");
        assert!(!page.has_content); // Virtual page

        // page_kind is tracked via sections index
        let sections = snapshot.state.find_sections_by_name("my-domain");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].1.kind, "domain");
    }

    #[test]
    fn test_real_page_with_type() {
        // Has both content and page_kind
        let storage =
            MockStorage::new().with_document_and_kind("real-domain", "Meta Title", "domain");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed().unwrap();

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
            .with_virtual_page_and_kind("my-domain", "My Domain", "domain")
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
            .with_virtual_page_and_kind("my-domain", "My Domain", "domain")
            .with_document("my-domain/child", "Child Page");

        let site = create_site_with_storage(storage);

        let nav = site.navigation(None).unwrap();

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
            .with_virtual_page_and_kind("domains", "Domains", "section")
            // Nested virtual page
            .with_virtual_page_and_kind("domains/billing", "Billing", "domain")
            // Real page in nested virtual
            .with_document("domains/billing/overview", "Overview");

        let site = create_site_with_storage(storage);

        let snapshot = site.reload_if_needed().unwrap();

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
        let root_nav = site.navigation(None).unwrap();
        assert_eq!(root_nav.items.len(), 1);
        assert_eq!(root_nav.items[0].title, "Domains");
        // Sections are leaves in root scope
        assert!(root_nav.items[0].children.is_empty());

        // Navigate into Domains section
        let domains_nav = site.navigation(Some("section:default/domains")).unwrap();
        assert_eq!(domains_nav.items.len(), 1);
        assert_eq!(domains_nav.items[0].title, "Billing");
        // Billing is also a section, so it's a leaf in domains scope
        assert!(domains_nav.items[0].children.is_empty());

        // Navigate into Billing section
        let billing_nav = site.navigation(Some("domain:default/billing")).unwrap();
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

    // ========================================================================
    // page_title tests
    // ========================================================================

    #[test]
    fn test_page_title_returns_title_for_known_page() {
        let storage = MockStorage::new().with_document("guide", "User Guide");
        let site = create_site_with_storage(storage);

        // Trigger initial load
        let _ = site.reload_if_needed().unwrap();

        assert_eq!(site.page_title("guide"), Some("User Guide".to_owned()));
    }

    #[test]
    fn test_page_title_returns_none_for_unknown_page() {
        let storage = MockStorage::new().with_document("guide", "Guide");
        let site = create_site_with_storage(storage);

        let _ = site.reload_if_needed().unwrap();

        assert_eq!(site.page_title("nonexistent"), None);
    }

    #[test]
    fn test_page_title_reads_cached_snapshot() {
        let storage = MockStorage::new().with_document("guide", "Old Title");
        let site = create_site_with_storage(storage);

        // Load initial state
        let _ = site.reload_if_needed().unwrap();

        // page_title reads cached snapshot, not triggering reload
        assert_eq!(site.page_title("guide"), Some("Old Title".to_owned()));
    }

    // ========================================================================
    // Storage error propagation tests
    // ========================================================================

    #[test]
    fn test_reload_if_needed_scan_error_on_initial_load() {
        let storage = MockStorage::new().with_scan_error(StorageErrorKind::Unavailable);
        let site = create_site_with_storage(storage);

        let result = site.reload_if_needed();

        assert!(result.is_err());
        assert_eq!(result.err().unwrap().kind, StorageErrorKind::Unavailable);
    }

    #[test]
    fn test_reload_keeps_stale_data_on_subsequent_scan_error() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let config = PageRendererConfig::default();
        let site = Site::new(
            Arc::clone(&storage) as Arc<dyn rw_storage::Storage>,
            Arc::new(rw_cache::NullCache),
            config,
        );

        // Initial load succeeds
        let snapshot = site.reload_if_needed().unwrap();
        assert!(snapshot.state.get_page("guide").is_some());

        // Make storage fail
        storage.set_scan_error(Some(StorageErrorKind::Unavailable));
        site.invalidate();

        // Reload should succeed with stale data
        let snapshot2 = site.reload_if_needed().unwrap();
        assert!(snapshot2.state.get_page("guide").is_some());
    }

    #[test]
    fn test_navigation_propagates_storage_error_on_initial_failure() {
        let storage = MockStorage::new().with_scan_error(StorageErrorKind::Unavailable);
        let site = create_site_with_storage(storage);

        let result = site.navigation(None);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, StorageErrorKind::Unavailable);
    }

    #[test]
    fn test_render_propagates_storage_error_on_initial_failure() {
        let storage = MockStorage::new().with_scan_error(StorageErrorKind::Unavailable);
        let site = create_site_with_storage(storage);

        let result = site.render("test");
        assert!(matches!(result, Err(RenderError::Storage(_))));
    }

    #[test]
    fn test_has_page_propagates_storage_error_on_initial_failure() {
        let storage = MockStorage::new().with_scan_error(StorageErrorKind::Unavailable);
        let site = create_site_with_storage(storage);

        let result = site.has_page("test");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, StorageErrorKind::Unavailable);
    }

    // ========================================================================
    // reload() tests
    // ========================================================================

    #[test]
    fn test_reload_false_reloads_when_has_changed_returns_true() {
        let storage = MockStorage::new().with_document("guide", "Guide");
        let site = create_site_with_storage(storage);

        // Initial load
        site.reload_if_needed().unwrap();

        // MockStorage inherits default has_changed() -> true,
        // so reload(false) should still trigger a reload.
        let result = site.reload(false).unwrap();
        assert!(result);
    }

    #[test]
    fn test_reload_true_always_reloads() {
        let storage = MockStorage::new().with_document("guide", "Guide");
        let site = create_site_with_storage(storage);

        // Initial load
        site.reload_if_needed().unwrap();

        let result = site.reload(true).unwrap();
        assert!(result);
    }

    #[test]
    fn test_reload_propagates_storage_errors_on_initial_load() {
        let storage = MockStorage::new().with_scan_error(StorageErrorKind::Unavailable);
        let site = create_site_with_storage(storage);

        let result = site.reload(true);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, StorageErrorKind::Unavailable);
    }

    #[test]
    fn test_reload_false_skips_when_has_changed_returns_false() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = Site::new(
            Arc::clone(&storage) as Arc<dyn rw_storage::Storage>,
            Arc::new(rw_cache::NullCache),
            PageRendererConfig::default(),
        );

        // Initial load
        site.reload_if_needed().unwrap();

        // Simulate no changes
        storage.set_has_changed(Some(Ok(false)));

        let result = site.reload(false).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_reload_false_reloads_before_first_scan() {
        // Before any scan(), has_changed() defaults to true, so
        // reload(false) should perform the initial load.
        let storage = MockStorage::new().with_document("guide", "Guide");
        let site = create_site_with_storage(storage);

        let result = site.reload(false).unwrap();
        assert!(result);

        // Verify data was actually loaded
        assert!(site.has_page("guide").unwrap());
    }

    #[test]
    fn test_reload_false_propagates_has_changed_error() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = Site::new(
            Arc::clone(&storage) as Arc<dyn rw_storage::Storage>,
            Arc::new(rw_cache::NullCache),
            PageRendererConfig::default(),
        );

        // Initial load
        site.reload_if_needed().unwrap();

        // Simulate has_changed error
        storage.set_has_changed(Some(Err(StorageErrorKind::Unavailable)));

        let result = site.reload(false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, StorageErrorKind::Unavailable);
    }

    #[test]
    fn test_navigation_ordered_by_pages_field() {
        let storage = MockStorage::new()
            .with_document_and_pages(
                "",
                "Home",
                vec!["getting-started".to_owned(), "config".to_owned()],
            )
            .with_document("advanced", "Advanced Topics")
            .with_document("config", "Configuration")
            .with_document("getting-started", "Getting Started");

        let site = create_site_with_storage(storage);
        let snapshot = site.reload_if_needed().unwrap();
        let nav = snapshot.state.navigation("");

        assert_eq!(nav.items[0].path, "getting-started");
        assert_eq!(nav.items[1].path, "config");
        assert_eq!(nav.items[2].path, "advanced"); // unlisted
    }

    #[test]
    fn test_page_ordering_end_to_end_with_fs() {
        use std::fs;

        use rw_storage_fs::FsStorage;

        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();

        fs::write(docs.join("index.md"), "# Home").unwrap();
        fs::write(docs.join("advanced.md"), "# Advanced").unwrap();
        fs::write(docs.join("getting-started.md"), "# Getting Started").unwrap();
        fs::write(docs.join("configuration.md"), "# Configuration").unwrap();
        fs::write(
            docs.join("meta.yaml"),
            "pages:\n  - getting-started\n  - configuration",
        )
        .unwrap();

        let storage = FsStorage::new(docs);
        let config = PageRendererConfig::default();
        let site = Site::new(Arc::new(storage), Arc::new(rw_cache::NullCache), config);
        let snapshot = site.reload_if_needed().unwrap();
        let nav = snapshot.state.navigation("");

        assert_eq!(nav.items.len(), 3);
        assert_eq!(nav.items[0].path, "getting-started");
        assert_eq!(nav.items[1].path, "configuration");
        assert_eq!(nav.items[2].path, "advanced"); // unlisted, alphabetical
    }

    #[test]
    fn test_reload_clears_page_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        // Use mtime 0.0 to simulate S3 storage (always returns zero)
        let storage = MockStorage::new()
            .with_file("", "Home", "# Home\n\nWelcome")
            .with_mtime("", 0.0);
        let storage = Arc::new(storage);

        let cache: Arc<dyn rw_cache::Cache> =
            Arc::new(rw_cache::FileCache::new(cache_dir, "1.0.0"));
        let config = PageRendererConfig::default();
        let site = Site::new(Arc::clone(&storage) as Arc<dyn Storage>, cache, config);

        // First render: cache miss
        let result1 = site.render("").unwrap();
        assert!(!result1.from_cache);

        // Second render: cache hit (mtime is still 0.0, etag matches)
        let result2 = site.render("").unwrap();
        assert!(result2.from_cache);

        // Simulate content change and reload
        storage.set_content("", "# Home\n\nUpdated");
        site.reload(true).unwrap();

        // Render after reload: cache miss (clear wiped the page cache)
        let result3 = site.render("").unwrap();
        assert!(!result3.from_cache);
        assert!(result3.html.contains("Updated"));
    }
}
