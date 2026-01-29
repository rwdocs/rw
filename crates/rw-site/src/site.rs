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
//! use rw_storage::FsStorage;
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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use rw_diagrams::{DiagramProcessor, FileCache};
use rw_renderer::directive::DirectiveProcessor;
use rw_renderer::{HtmlBackend, MarkdownRenderer, TabsDirective, TocEntry};
use rw_storage::{Storage, StorageError, StorageErrorKind};

use crate::page_cache::{FilePageCache, NullPageCache, PageCache};
use crate::site_cache::{FileSiteCache, NullSiteCache, SiteCache};

// Re-import from crate root for public types, and direct module for internal
pub(crate) use crate::site_state::{BreadcrumbItem, NavItem, Page, SiteState, SiteStateBuilder};

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
    /// Source file path (relative to storage root).
    pub source_path: PathBuf,
    /// Source file modification time (Unix timestamp).
    pub source_mtime: f64,
    /// Breadcrumb navigation items.
    pub breadcrumbs: Vec<BreadcrumbItem>,
}

/// Error returned when page rendering fails.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// Source file not found.
    #[error("Source file not found: {}", .0.display())]
    FileNotFound(PathBuf),
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
            StorageErrorKind::NotFound => {
                Self::FileNotFound(e.path().map(Path::to_path_buf).unwrap_or_default())
            }
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

    /// Get navigation tree.
    ///
    /// Reloads site if needed and returns the navigation tree.
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    #[must_use]
    pub fn navigation(&self) -> Vec<NavItem> {
        self.reload_if_needed().navigation()
    }

    /// Get page by source file path.
    ///
    /// Reloads site if needed and returns the page for a given source path.
    ///
    /// # Arguments
    ///
    /// * `source_path` - Relative path to source file (e.g., "guide.md")
    ///
    /// # Panics
    ///
    /// Panics if internal locks are poisoned.
    #[must_use]
    pub fn get_page_by_source(&self, source_path: &Path) -> Option<Page> {
        self.reload_if_needed()
            .get_page_by_source(source_path)
            .cloned()
    }

    /// Get page by URL path.
    ///
    /// Reloads site if needed and returns the page for a given URL path.
    ///
    /// # Arguments
    ///
    /// * `path` - URL path (e.g., "/guide" or "guide")
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
    /// * `path` - URL path (e.g., "/guide/setup")
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
    /// * `path` - URL path (e.g., "/guide" or "guide")
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

        // Get source mtime
        let source_mtime = self
            .storage
            .mtime(&page.source_path)
            .map_err(|_| RenderError::FileNotFound(page.source_path.clone()))?;

        // Get breadcrumbs
        let breadcrumbs = state.get_breadcrumbs(path);

        // Normalize path for cache key (strip leading slash)
        let cache_key = path.trim_start_matches('/');

        // Check page cache
        if let Some(cached) = self.page_cache.get(cache_key, source_mtime) {
            return Ok(PageRenderResult {
                html: cached.html,
                title: cached.meta.title,
                toc: cached.meta.toc,
                warnings: Vec::new(),
                from_cache: true,
                source_path: page.source_path.clone(),
                source_mtime,
                breadcrumbs,
            });
        }

        // Render the page
        let result = self.render_page(&page.source_path, cache_key)?;

        // Store in cache
        self.page_cache.set(
            cache_key,
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
            source_path: page.source_path.clone(),
            source_mtime,
            breadcrumbs,
        })
    }

    /// Invalidate page cache for a path.
    ///
    /// # Arguments
    ///
    /// * `path` - Document path to invalidate
    pub fn invalidate_page(&self, path: &str) {
        let cache_key = path.trim_start_matches('/');
        self.page_cache.invalidate(cache_key);
    }

    /// Render a page from source path.
    fn render_page(
        &self,
        source_path: &Path,
        base_path: &str,
    ) -> Result<RenderResult, RenderError> {
        let markdown_text = self.storage.read(source_path)?;

        let mut renderer = self.create_renderer(base_path);
        if let Some(processor) = self.create_diagram_processor() {
            renderer = renderer.with_processor(processor);
        }

        let result = renderer.render_markdown(&markdown_text);

        Ok(RenderResult {
            html: result.html,
            title: result.title,
            toc: result.toc,
            warnings: result.warnings,
        })
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
    /// Uses `storage.scan()` to get documents, then builds hierarchy based on
    /// path conventions.
    fn load_from_storage(&self) -> SiteState {
        let mut builder = SiteStateBuilder::new();

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

/// Internal render result (before adding metadata).
struct RenderResult {
    html: String,
    title: Option<String>,
    toc: Vec<TocEntry>,
    warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    // Ensure Site is Send + Sync for use with Arc
    static_assertions::assert_impl_all!(super::Site: Send, Sync);

    use std::fs;
    use std::sync::Arc;

    use rw_storage::FsStorage;

    use super::*;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn create_site(source_dir: PathBuf) -> Site {
        let storage = Arc::new(FsStorage::new(source_dir));
        let config = SiteConfig::default();
        Site::new(storage, config)
    }

    // ========================================================================
    // Site structure tests
    // ========================================================================

    #[test]
    fn test_reload_if_needed_missing_dir_returns_empty_site() {
        let temp_dir = create_test_dir();
        let site = create_site(temp_dir.path().join("nonexistent"));

        let state = site.reload_if_needed();

        assert!(state.get_root_pages().is_empty());
    }

    #[test]
    fn test_reload_if_needed_empty_dir_returns_empty_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        assert!(state.get_root_pages().is_empty());
    }

    #[test]
    fn test_reload_if_needed_flat_structure_builds_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# User Guide\n\nContent.").unwrap();
        fs::write(source_dir.join("api.md"), "# API Reference\n\nDocs.").unwrap();

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        assert_eq!(state.get_root_pages().len(), 2);
        assert!(state.get_page("/guide").is_some());
        assert!(state.get_page("/api").is_some());
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

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        let page = state.get_page("/");
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "Welcome");
        assert_eq!(page.path, "/");
        assert_eq!(page.source_path, PathBuf::from("index.md"));
    }

    #[test]
    fn test_reload_if_needed_nested_structure_builds_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        let domain_dir = source_dir.join("domain-a");
        fs::create_dir_all(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain A\n\nOverview.").unwrap();
        fs::write(domain_dir.join("guide.md"), "# Setup Guide\n\nSteps.").unwrap();

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        let domain = state.get_page("/domain-a");
        assert!(domain.is_some());
        let domain = domain.unwrap();
        assert_eq!(domain.title, "Domain A");
        assert_eq!(domain.source_path, PathBuf::from("domain-a/index.md"));

        let children = state.get_children("/domain-a");
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

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        let page = state.get_page("/guide");
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

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        let page = state.get_page("/setup-guide");
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

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        let page = state.get_page("/руководство");
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

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        assert!(state.get_page("/.hidden").is_none());
        assert!(state.get_page("/visible").is_some());
    }

    #[test]
    fn test_reload_if_needed_skips_underscore_files() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("_partial.md"), "# Partial").unwrap();
        fs::write(source_dir.join("main.md"), "# Main").unwrap();

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        assert!(state.get_page("/_partial").is_none());
        assert!(state.get_page("/main").is_some());
    }

    #[test]
    fn test_reload_if_needed_directory_without_index_promotes_children() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        let no_index_dir = source_dir.join("no-index");
        fs::create_dir_all(&no_index_dir).unwrap();
        fs::write(no_index_dir.join("child.md"), "# Child Page").unwrap();

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        // Child should be at root level (promoted)
        let roots = state.get_root_pages();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/no-index/child");
        assert_eq!(roots[0].source_path, PathBuf::from("no-index/child.md"));
    }

    #[test]
    fn test_state_returns_same_arc() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let site = create_site(source_dir);

        // First reload to populate
        let _ = site.reload_if_needed();

        // state() should return the same Arc
        let state1 = site.state();
        let state2 = site.state();

        assert!(Arc::ptr_eq(&state1, &state2));
    }

    #[test]
    fn test_reload_if_needed_caches_result() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let site = create_site(source_dir);

        let state1 = site.reload_if_needed();
        let state2 = site.reload_if_needed();

        // Should return the same Arc (cached)
        assert!(Arc::ptr_eq(&state1, &state2));
    }

    #[test]
    fn test_invalidate_clears_cached_site() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let site = create_site(source_dir.clone());

        // First reload - should NOT have /new
        let state1 = site.reload_if_needed();
        assert!(state1.get_page("/new").is_none());

        // Add new file and invalidate
        fs::write(source_dir.join("new.md"), "# New").unwrap();
        site.invalidate();

        // Second reload - should have /new now
        let state2 = site.reload_if_needed();
        assert!(state2.get_page("/new").is_some());

        // Should be a different Arc (reloaded)
        assert!(!Arc::ptr_eq(&state1, &state2));
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Guide").unwrap();

        let site = Arc::new(create_site(source_dir));

        // Spawn multiple threads accessing concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let site = Arc::clone(&site);
                thread::spawn(move || {
                    let state = site.reload_if_needed();
                    assert!(state.get_page("/guide").is_some());
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

        let site = Arc::new(create_site(source_dir));

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
                        assert!(state.get_page("/guide").is_some());
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Final state should be valid
        let state = site.reload_if_needed();
        assert!(state.get_page("/guide").is_some());
    }

    #[test]
    fn test_mtime_cache_reuses_titles() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Original Title").unwrap();

        let site = create_site(source_dir);

        // First load
        let state1 = site.reload_if_needed();
        assert_eq!(state1.get_page("/guide").unwrap().title, "Original Title");

        // Invalidate and reload without changing file - should use cached title
        site.invalidate();
        let state2 = site.reload_if_needed();
        assert_eq!(state2.get_page("/guide").unwrap().title, "Original Title");
    }

    #[test]
    fn test_mtime_cache_detects_changes() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("guide.md"), "# Original Title").unwrap();

        let site = create_site(source_dir.clone());

        // First load
        let state1 = site.reload_if_needed();
        assert_eq!(state1.get_page("/guide").unwrap().title, "Original Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify file
        fs::write(source_dir.join("guide.md"), "# Updated Title").unwrap();
        site.invalidate();

        // Reload should see new title
        let state2 = site.reload_if_needed();
        assert_eq!(state2.get_page("/guide").unwrap().title, "Updated Title");
    }

    #[test]
    fn test_source_path_to_url() {
        assert_eq!(Site::source_path_to_url(Path::new("index.md")), "/");
        assert_eq!(Site::source_path_to_url(Path::new("guide.md")), "/guide");
        assert_eq!(
            Site::source_path_to_url(Path::new("domain/index.md")),
            "/domain"
        );
        assert_eq!(
            Site::source_path_to_url(Path::new("domain/setup.md")),
            "/domain/setup"
        );
        assert_eq!(Site::source_path_to_url(Path::new("a/b/c.md")), "/a/b/c");
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

        let site = create_site(source_dir);

        let state = site.reload_if_needed();

        // Check root
        let root = state.get_page("/").unwrap();
        assert_eq!(root.title, "Home");

        // Check level 1
        let level1 = state.get_page("/level1").unwrap();
        assert_eq!(level1.title, "Level 1");

        // Check level 1 is child of root
        let root_children = state.get_children("/");
        assert!(root_children.iter().any(|c| c.path == "/level1"));

        // Check level 2
        let level2 = state.get_page("/level1/level2").unwrap();
        assert_eq!(level2.title, "Level 2");

        // Check level 2 is child of level 1
        let level1_children = state.get_children("/level1");
        assert!(level1_children.iter().any(|c| c.path == "/level1/level2"));

        // Check deep page
        let deep = state.get_page("/level1/level2/page").unwrap();
        assert_eq!(deep.title, "Deep Page");

        // Check deep page is child of level 2
        let level2_children = state.get_children("/level1/level2");
        assert!(
            level2_children
                .iter()
                .any(|c| c.path == "/level1/level2/page")
        );
    }

    // ========================================================================
    // Rendering tests
    // ========================================================================

    #[test]
    fn test_render_simple_markdown() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("test.md"), "# Hello\n\nWorld").unwrap();

        let storage = Arc::new(FsStorage::new(source_dir));
        let config = SiteConfig {
            extract_title: true,
            ..Default::default()
        };
        let site = Site::new(storage, config);

        let result = site.render("/test").unwrap();
        assert!(result.html.contains("<p>World</p>"));
        assert_eq!(result.title, Some("Hello".to_string()));
        assert!(!result.from_cache);
        assert_eq!(result.source_path, PathBuf::from("test.md"));
    }

    #[test]
    fn test_render_page_not_found() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(source_dir.join("exists.md"), "# Exists").unwrap();

        let site = create_site(source_dir);

        let result = site.render("/nonexistent");
        assert!(matches!(result, Err(RenderError::PageNotFound(_))));
    }

    #[test]
    fn test_render_with_cache() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("test.md"), "# Cached\n\nContent").unwrap();

        let storage = Arc::new(FsStorage::new(source_dir));
        let config = SiteConfig {
            cache_dir: Some(cache_dir),
            version: "1.0.0".to_string(),
            extract_title: true,
            ..Default::default()
        };
        let site = Site::new(storage, config);

        // First render - cache miss
        let result1 = site.render("/test").unwrap();
        assert!(!result1.from_cache);
        assert_eq!(result1.title, Some("Cached".to_string()));

        // Second render - cache hit
        let result2 = site.render("/test").unwrap();
        assert!(result2.from_cache);
        assert_eq!(result2.title, Some("Cached".to_string()));
        assert_eq!(result1.html, result2.html);
    }

    #[test]
    fn test_render_includes_breadcrumbs() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        let domain_dir = source_dir.join("domain");
        fs::create_dir_all(&domain_dir).unwrap();
        fs::write(source_dir.join("index.md"), "# Home").unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("page.md"), "# Page").unwrap();

        let site = create_site(source_dir);

        let result = site.render("/domain/page").unwrap();

        assert_eq!(result.breadcrumbs.len(), 2);
        assert_eq!(result.breadcrumbs[0].title, "Home");
        assert_eq!(result.breadcrumbs[0].path, "/");
        assert_eq!(result.breadcrumbs[1].title, "Domain");
        assert_eq!(result.breadcrumbs[1].path, "/domain");
    }

    #[test]
    fn test_render_toc_generation() {
        let temp_dir = create_test_dir();
        let source_dir = temp_dir.path().join("docs");
        fs::create_dir(&source_dir).unwrap();
        fs::write(
            source_dir.join("test.md"),
            "# Title\n\n## Section 1\n\n## Section 2",
        )
        .unwrap();

        let site = create_site(source_dir);

        let result = site.render("/test").unwrap();
        assert_eq!(result.toc.len(), 2);
        assert_eq!(result.toc[0].title, "Section 1");
        assert_eq!(result.toc[0].level, 2);
        assert_eq!(result.toc[1].title, "Section 2");
    }
}
