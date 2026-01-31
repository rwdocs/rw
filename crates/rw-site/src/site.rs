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
//! - `state()` returns `Arc<SiteState>` with minimal locking (just Arc clone)
//! - `reload_if_needed()` uses double-checked locking for efficient cache validation
//! - `invalidate()` is lock-free (atomic flag)
//!
//! # Example
//!
//! ```ignore
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use rw_site::{Site, SiteConfig};
//! use rw_storage_fs::FsStorage;
//!
//! let storage = Arc::new(FsStorage::new(PathBuf::from("docs")));
//! let config = SiteConfig {
//!     cache_dir: Some(PathBuf::from(".cache")),
//!     version: "1.0.0".to_string(),
//!     ..Default::default()
//! };
//! let site = Arc::new(Site::new(storage, config));
//!
//! // Load site structure
//! let state = site.reload_if_needed();
//!
//! // Render a page
//! let result = site.render("/guide")?;
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use crate::page_cache::{FilePageCache, NullPageCache, PageCache};
use crate::site_cache::{FileSiteCache, NullSiteCache, SiteCache};
use rw_diagrams::{DiagramProcessor, FileCache};
use rw_renderer::directive::DirectiveProcessor;
use rw_renderer::{HtmlBackend, MarkdownRenderer, TabsDirective, TocEntry, escape_html};
use rw_storage::{Metadata, Storage, StorageError, StorageErrorKind};

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

// Re-import from crate root for public types, and direct module for internal
pub(crate) use crate::site_state::{BreadcrumbItem, Navigation, Page, SiteState, SiteStateBuilder};

/// Result of rendering a markdown page.
#[derive(Clone, Debug)]
pub struct PageRenderResult {
    /// Rendered HTML content.
    pub html: String,
    /// Title extracted from first H1 heading (if enabled).
    pub title: Option<String>,
    /// Table of contents entries.
    pub toc: Vec<TocEntry>,
    /// Warnings generated during conversion (e.g., unresolved includes).
    pub warnings: Vec<String>,
    /// Whether result was served from cache.
    pub from_cache: bool,
    /// Whether the page has content (real page vs virtual page).
    pub has_content: bool,
    /// Source file modification time (Unix timestamp).
    pub source_mtime: f64,
    /// Breadcrumb navigation items.
    pub breadcrumbs: Vec<BreadcrumbItem>,
    /// Page metadata from YAML sidecar file.
    pub metadata: Option<Metadata>,
}

/// Error returned when page rendering fails.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// Content not found for page.
    #[error("Content not found: {0}")]
    FileNotFound(String),
    /// Page not found in site structure.
    #[error("Page not found: {0}")]
    PageNotFound(String),
    /// I/O error reading source file.
    #[error("I/O error: {0}")]
    Io(#[source] std::io::Error),
}

impl From<StorageError> for RenderError {
    fn from(e: StorageError) -> Self {
        match e.kind() {
            StorageErrorKind::NotFound => Self::FileNotFound(
                e.path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ),
            _ => Self::Io(std::io::Error::other(e.to_string())),
        }
    }
}

/// Configuration for [`Site`].
#[derive(Clone, Debug)]
pub struct SiteConfig {
    /// Cache directory for site structure and rendered pages.
    ///
    /// If `None`, caching is disabled.
    pub cache_dir: Option<PathBuf>,
    /// Application version for cache invalidation.
    pub version: String,
    /// Extract title from first H1 heading.
    pub extract_title: bool,
    /// Kroki URL for diagram rendering.
    ///
    /// If `None`, diagrams are rendered as syntax-highlighted code blocks.
    pub kroki_url: Option<String>,
    /// Directories to search for `PlantUML` includes.
    pub include_dirs: Vec<PathBuf>,
    /// `PlantUML` config file name (searched in `include_dirs`).
    pub config_file: Option<String>,
    /// DPI for diagram rendering (default: 192 for retina).
    pub dpi: u32,
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            cache_dir: None,
            version: String::new(),
            extract_title: true,
            kroki_url: None,
            include_dirs: Vec::new(),
            config_file: None,
            dpi: 192,
        }
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
/// - Uses internal `RwLock<Arc<SiteState>>` for the current site state snapshot
/// - Uses `Mutex<()>` for serializing reload operations
/// - Uses `AtomicBool` for cache validity tracking
pub struct Site {
    storage: Arc<dyn Storage>,
    // Structure caching
    structure_cache: Box<dyn SiteCache>,
    /// Mutex for serializing reload operations.
    reload_lock: Mutex<()>,
    /// Current site state snapshot (atomically swappable).
    current_state: RwLock<Arc<SiteState>>,
    /// Cache validity flag.
    cache_valid: AtomicBool,
    // Page rendering
    page_cache: Box<dyn PageCache>,
    extract_title: bool,
    kroki_url: Option<String>,
    include_dirs: Vec<PathBuf>,
    config_file: Option<String>,
    dpi: u32,
}

impl Site {
    /// Create a new site with storage and configuration.
    ///
    /// # Arguments
    ///
    /// * `storage` - Storage implementation for document scanning and reading
    /// * `config` - Site configuration
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, config: SiteConfig) -> Self {
        let structure_cache: Box<dyn SiteCache> = match &config.cache_dir {
            Some(dir) => Box::new(FileSiteCache::new(dir.clone())),
            None => Box::new(NullSiteCache),
        };

        let page_cache: Box<dyn PageCache> = match &config.cache_dir {
            Some(dir) => Box::new(FilePageCache::new(dir.clone(), config.version.clone())),
            None => Box::new(NullPageCache),
        };

        // Create initial empty site state
        let initial_state = Arc::new(SiteStateBuilder::new().build());

        Self {
            storage,
            structure_cache,
            reload_lock: Mutex::new(()),
            current_state: RwLock::new(initial_state),
            cache_valid: AtomicBool::new(false),
            page_cache,
            extract_title: config.extract_title,
            kroki_url: config.kroki_url,
            include_dirs: config.include_dirs,
            config_file: config.config_file,
            dpi: config.dpi,
        }
    }

    /// Get current site state snapshot.
    ///
    /// Returns an `Arc<SiteState>` that can be used without holding any lock.
    /// The site state is guaranteed to be internally consistent.
    ///
    /// Note: This returns the current snapshot without checking cache validity.
    /// For most use cases, prefer `reload_if_needed()` which ensures the site
    /// is up-to-date.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[must_use]
    pub(crate) fn state(&self) -> Arc<SiteState> {
        self.current_state.read().unwrap().clone()
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
        self.reload_if_needed().navigation(scope_path)
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
        self.reload_if_needed().get_navigation_scope(page_path)
    }

    /// Get page by URL path.
    ///
    /// Reloads site if needed and returns the page for a given URL path.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path without leading slash (e.g., "guide", "domain/page", "" for root)
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    #[must_use]
    pub fn get_page(&self, path: &str) -> Option<Page> {
        self.reload_if_needed().get_page(path).cloned()
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
        self.reload_if_needed().get_breadcrumbs(path)
    }

    /// Reload site state from storage if cache is invalid.
    ///
    /// Uses double-checked locking pattern:
    /// 1. Fast path: return current site state if cache valid
    /// 2. Slow path: acquire `reload_lock`, recheck, then reload
    ///
    /// # Returns
    ///
    /// `Arc<SiteState>` containing the current site state snapshot.
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    pub(crate) fn reload_if_needed(&self) -> Arc<SiteState> {
        // Fast path: cache valid
        if self.cache_valid.load(Ordering::Acquire) {
            return self.state();
        }

        // Slow path: acquire reload lock
        let _guard = self.reload_lock.lock().unwrap();

        // Double-check after acquiring lock
        if self.cache_valid.load(Ordering::Acquire) {
            return self.state();
        }

        // Try file cache first
        if let Some(site) = self.structure_cache.get() {
            let site = Arc::new(site);
            *self.current_state.write().unwrap() = site.clone();
            self.cache_valid.store(true, Ordering::Release);
            return site;
        }

        // Load from storage
        let site = self.load_from_storage();
        let site = Arc::new(site);

        // Store in file cache
        self.structure_cache.set(&site);

        // Update current state
        *self.current_state.write().unwrap() = site.clone();
        self.cache_valid.store(true, Ordering::Release);

        site
    }

    /// Invalidate cached site state and page caches.
    ///
    /// Marks cache as invalid. Next `reload_if_needed()` will reload.
    /// Current readers continue using their existing `Arc<SiteState>`.
    pub fn invalidate(&self) {
        self.cache_valid.store(false, Ordering::Release);
        self.structure_cache.invalidate();
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
        // Reload site if needed
        let state = self.reload_if_needed();

        // Look up page by URL path
        let page = state
            .get_page(path)
            .ok_or_else(|| RenderError::PageNotFound(path.to_string()))?;

        // Get breadcrumbs
        let breadcrumbs = state.get_breadcrumbs(path);

        // Check if this is a virtual page (no content file)
        if !page.has_content {
            return Ok(self.render_virtual_page(path, page, breadcrumbs));
        }

        // Get source mtime using URL path (Storage maps to file internally)
        let source_mtime = self
            .storage
            .mtime(path)
            .map_err(|_| RenderError::FileNotFound(path.to_string()))?;

        // Load metadata lazily from storage (with inheritance applied)
        let metadata = self.load_metadata(path);

        // Check page cache
        if let Some(cached) = self.page_cache.get(path, source_mtime) {
            return Ok(PageRenderResult {
                html: cached.html,
                title: cached.meta.title,
                toc: cached.meta.toc,
                warnings: Vec::new(),
                from_cache: true,
                has_content: page.has_content,
                source_mtime,
                breadcrumbs,
                metadata,
            });
        }

        // Render the page using URL path (Storage maps to file internally)
        let markdown_text = self.storage.read(path)?;
        let result = self.create_renderer(path).render_markdown(&markdown_text);

        // Store in cache
        self.page_cache.set(
            path,
            &result.html,
            result.title.as_deref(),
            source_mtime,
            &result.toc,
        );

        Ok(PageRenderResult {
            html: result.html,
            title: result.title,
            toc: result.toc,
            warnings: result.warnings,
            from_cache: false,
            has_content: page.has_content,
            source_mtime,
            breadcrumbs,
            metadata,
        })
    }

    /// Render a virtual page (directory with metadata but no index.md).
    ///
    /// Returns an h1 with the page title.
    fn render_virtual_page(
        &self,
        path: &str,
        page: &Page,
        breadcrumbs: Vec<BreadcrumbItem>,
    ) -> PageRenderResult {
        // Get mtime from metadata file
        let source_mtime = self.storage.mtime(path).unwrap_or(0.0);

        // Load metadata lazily from storage (with inheritance applied)
        let metadata = self.load_metadata(path);

        PageRenderResult {
            html: format!("<h1>{}</h1>\n", escape_html(&page.title)),
            title: Some(page.title.clone()),
            toc: Vec::new(),
            warnings: Vec::new(),
            from_cache: false,
            has_content: false,
            source_mtime,
            breadcrumbs,
            metadata,
        }
    }

    /// Invalidate page cache for a path.
    ///
    /// # Arguments
    ///
    /// * `path` - Document path without leading slash (e.g., "guide", "" for root)
    pub fn invalidate_page(&self, path: &str) {
        self.page_cache.invalidate(path);
    }

    /// Create a renderer with common configuration.
    fn create_renderer(&self, base_path: &str) -> MarkdownRenderer<HtmlBackend> {
        let directives = DirectiveProcessor::new().with_container(TabsDirective::new());

        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_gfm(true)
            .with_base_path(base_path)
            .with_directives(directives);

        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }

        if let Some(processor) = self.create_diagram_processor() {
            renderer = renderer.with_processor(processor);
        }

        renderer
    }

    /// Create a diagram processor if `kroki_url` is configured.
    fn create_diagram_processor(&self) -> Option<DiagramProcessor> {
        let url = self.kroki_url.as_ref()?;

        let mut processor = DiagramProcessor::new(url)
            .include_dirs(&self.include_dirs)
            .config_file(self.config_file.as_deref())
            .dpi(self.dpi);

        if let Some(dir) = self.page_cache.diagrams_dir() {
            processor = processor.with_cache(Arc::new(FileCache::new(dir.to_path_buf())));
        }

        Some(processor)
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
        let scan_result = match self.storage.scan() {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to scan storage");
                return builder.build();
            }
        };

        let mut documents = scan_result.documents;

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
                doc.page_type.clone(),
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

    /// Load metadata for a path (lazy loading).
    fn load_metadata(&self, path: &str) -> Option<Metadata> {
        match self.storage.meta(path) {
            Ok(meta) => meta,
            Err(e) => {
                tracing::warn!(path = %path, error = %e, "Failed to load metadata");
                None
            }
        }
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
        let config = SiteConfig::default();
        Site::new(Arc::new(storage), config)
    }

    // ========================================================================
    // Site structure tests
    // ========================================================================

    #[test]
    fn test_reload_if_needed_empty_storage_returns_empty_site() {
        let storage = MockStorage::new();
        let site = create_site_with_storage(storage);

        let state = site.reload_if_needed();

        assert!(state.get_root_pages().is_empty());
    }

    #[test]
    fn test_reload_if_needed_flat_structure_builds_site() {
        let storage = MockStorage::new()
            .with_document("guide", "User Guide")
            .with_document("api", "API Reference");

        let site = create_site_with_storage(storage);

        let state = site.reload_if_needed();

        assert_eq!(state.get_root_pages().len(), 2);
        assert!(state.get_page("guide").is_some());
        assert!(state.get_page("api").is_some());
    }

    #[test]
    fn test_reload_if_needed_root_index_adds_home_page() {
        let storage = MockStorage::new()
            .with_file("", "Welcome", "# Welcome\n\nHome page content.");

        let site = create_site_with_storage(storage);

        let state = site.reload_if_needed();

        let page = state.get_page("");
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

        let state = site.reload_if_needed();

        let domain = state.get_page("domain-a");
        assert!(domain.is_some());
        let domain = domain.unwrap();
        assert_eq!(domain.title, "Domain A");
        assert!(domain.has_content);

        // Verify child via root navigation (non-section pages expand their children)
        let nav = state.navigation("");
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].path, "domain-a");
        assert_eq!(nav.items[0].children.len(), 1);
        assert_eq!(nav.items[0].children[0].title, "Setup Guide");

        // Verify child page details
        let child = state.get_page("domain-a/guide").unwrap();
        assert!(child.has_content);
    }

    #[test]
    fn test_reload_if_needed_page_titles_from_storage() {
        let storage = MockStorage::new()
            .with_document("guide", "My Custom Title");

        let site = create_site_with_storage(storage);

        let state = site.reload_if_needed();

        let page = state.get_page("guide");
        assert!(page.is_some());
        assert_eq!(page.unwrap().title, "My Custom Title");
    }

    #[test]
    fn test_reload_if_needed_cyrillic_path() {
        let storage = MockStorage::new()
            .with_document("руководство", "Руководство");

        let site = create_site_with_storage(storage);

        let state = site.reload_if_needed();

        let page = state.get_page("руководство");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Руководство");
        assert_eq!(page.path, "руководство");
        assert!(page.has_content);
    }

    #[test]
    fn test_reload_if_needed_directory_without_index_promotes_children() {
        // MockStorage simulates child promotion by just providing the child at path
        let storage = MockStorage::new()
            .with_document("no-index/child", "Child Page");

        let site = create_site_with_storage(storage);

        let state = site.reload_if_needed();

        // Child should be at root level (promoted)
        let roots = state.get_root_pages();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "no-index/child");
        assert!(roots[0].has_content);
    }

    #[test]
    fn test_state_returns_same_arc() {
        let storage = MockStorage::new()
            .with_document("guide", "Guide");

        let site = create_site_with_storage(storage);

        // First reload to populate
        let _ = site.reload_if_needed();

        // state() should return the same Arc
        let state1 = site.state();
        let state2 = site.state();

        assert!(Arc::ptr_eq(&state1, &state2));
    }

    #[test]
    fn test_reload_if_needed_caches_result() {
        let storage = MockStorage::new()
            .with_document("guide", "Guide");

        let site = create_site_with_storage(storage);

        let state1 = site.reload_if_needed();
        let state2 = site.reload_if_needed();

        // Should return the same Arc (cached)
        assert!(Arc::ptr_eq(&state1, &state2));
    }

    #[test]
    fn test_invalidate_clears_cached_state() {
        let storage = MockStorage::new()
            .with_document("guide", "Guide");

        let site = create_site_with_storage(storage);

        // First reload
        let state1 = site.reload_if_needed();
        assert!(state1.get_page("guide").is_some());

        // Invalidate cache
        site.invalidate();

        // Second reload - should be a different Arc
        let state2 = site.reload_if_needed();
        assert!(!Arc::ptr_eq(&state1, &state2));
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let storage = MockStorage::new()
            .with_document("guide", "Guide");

        let site = Arc::new(create_site_with_storage(storage));

        // Spawn multiple threads accessing concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let site = Arc::clone(&site);
                thread::spawn(move || {
                    let state = site.reload_if_needed();
                    assert!(state.get_page("guide").is_some());
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

        let storage = MockStorage::new()
            .with_document("guide", "Guide");

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
                        let state = site.reload_if_needed();
                        // Site should always be valid
                        assert!(state.get_page("guide").is_some());
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Final state should be valid
        let state = site.reload_if_needed();
        assert!(state.get_page("guide").is_some());
    }

    #[test]
    fn test_nested_hierarchy_with_multiple_levels() {
        let storage = MockStorage::new()
            .with_file("", "Home", "# Home")
            .with_file("level1", "Level 1", "# Level 1")
            .with_file("level1/level2", "Level 2", "# Level 2")
            .with_file("level1/level2/page", "Deep Page", "# Deep Page");

        let site = create_site_with_storage(storage);

        let state = site.reload_if_needed();

        // Check root
        let root = state.get_page("").unwrap();
        assert_eq!(root.title, "Home");

        // Check level 1
        let level1 = state.get_page("level1").unwrap();
        assert_eq!(level1.title, "Level 1");

        // Check level 2
        let level2 = state.get_page("level1/level2").unwrap();
        assert_eq!(level2.title, "Level 2");

        // Check deep page
        let deep = state.get_page("level1/level2/page").unwrap();
        assert_eq!(deep.title, "Deep Page");

        // Verify nested hierarchy via root navigation (non-section pages expand children)
        let root_nav = state.navigation("");
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

        let config = SiteConfig {
            extract_title: true,
            ..Default::default()
        };
        let site = Site::new(Arc::new(storage), config);

        let result = site.render("test").unwrap();
        assert!(result.html.contains("<p>World</p>"));
        assert_eq!(result.title, Some("Hello".to_string()));
        assert!(!result.from_cache);
        assert!(result.has_content);
    }

    #[test]
    fn test_render_page_not_found() {
        let storage = MockStorage::new()
            .with_document("exists", "Exists");

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

        let config = SiteConfig {
            cache_dir: Some(cache_dir),
            version: "1.0.0".to_string(),
            extract_title: true,
            ..Default::default()
        };
        let site = Site::new(Arc::new(storage), config);

        // First render - cache miss
        let result1 = site.render("test").unwrap();
        assert!(!result1.from_cache);
        assert_eq!(result1.title, Some("Cached".to_string()));

        // Second render - cache hit
        let result2 = site.render("test").unwrap();
        assert!(result2.from_cache);
        assert_eq!(result2.title, Some("Cached".to_string()));
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
        let storage = MockStorage::new()
            .with_virtual_page_and_type("my-domain", "My Domain", "domain");

        let site = create_site_with_storage(storage);

        let state = site.reload_if_needed();

        let page = state.get_page("my-domain");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "My Domain");
        assert!(!page.has_content); // Virtual page

        // page_type is tracked via sections map
        let section = state.sections().get("my-domain");
        assert!(section.is_some());
        assert_eq!(section.unwrap().section_type, "domain");
    }

    #[test]
    fn test_real_page_with_type() {
        // Has both content and page_type
        let storage = MockStorage::new()
            .with_document_and_type("real-domain", "Meta Title", "domain");

        let site = create_site_with_storage(storage);

        let state = site.reload_if_needed();

        let page = state.get_page("real-domain");
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
        assert_eq!(result.title, Some("My Domain".to_string()));
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

        let state = site.reload_if_needed();

        // Check parent virtual
        let domains = state.get_page("domains");
        assert!(domains.is_some());
        assert!(!domains.unwrap().has_content);

        // Check child virtual
        let billing = state.get_page("domains/billing");
        assert!(billing.is_some());
        assert!(!billing.unwrap().has_content);

        // Check real page has correct parent
        let overview = state.get_page("domains/billing/overview");
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
}
