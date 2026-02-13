//! Page rendering pipeline.
//!
//! [`PageRenderer`] handles markdown-to-HTML conversion, page caching,
//! diagram processing, and metadata loading. Extracted from [`Site`](crate::Site)
//! to enable independent testing of the rendering pipeline.

use std::path::PathBuf;
use std::sync::Arc;

use rw_cache::{Cache, CacheBucket, CacheBucketExt};
use rw_diagrams::{DiagramProcessor, MetaIncludeSource};
use rw_renderer::directive::DirectiveProcessor;
use rw_renderer::{HtmlBackend, MarkdownRenderer, TabsDirective, TocEntry, escape_html};
use rw_storage::{Metadata, Storage, StorageError, StorageErrorKind};
use serde::{Deserialize, Serialize};

/// Configuration for [`PageRenderer`].
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct PageRendererConfig {
    /// Extract title from first H1 heading.
    pub extract_title: bool,
    /// Kroki URL for diagram rendering.
    ///
    /// If `None`, diagrams are rendered as syntax-highlighted code blocks.
    pub kroki_url: Option<String>,
    /// Directories to search for `PlantUML` includes.
    pub include_dirs: Vec<PathBuf>,
    /// DPI for diagram rendering (default: 192 for retina).
    pub dpi: u32,
    /// Produce relative links instead of absolute paths.
    ///
    /// Default: `false` (absolute paths for SPA navigation).
    /// Set to `true` for static site builds (e.g., `TechDocs`).
    pub relative_links: bool,
    /// Append trailing slash to resolved link paths.
    ///
    /// Default: `false`.
    pub trailing_slash: bool,
    /// Produce CSS-only tabs instead of JS-dependent tabs.
    ///
    /// Default: `false`.
    pub static_tabs: bool,
}

impl Default for PageRendererConfig {
    fn default() -> Self {
        Self {
            extract_title: true,
            kroki_url: None,
            include_dirs: Vec::new(),
            dpi: 192,
            relative_links: false,
            trailing_slash: false,
            static_tabs: false,
        }
    }
}

/// Document page data.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// Page title (from H1 heading, filename, or metadata override).
    pub title: String,
    /// URL path without leading slash (e.g., "guide", "domain/page", "" for root).
    pub path: String,
    /// True if page has content (real page). False for virtual pages (metadata only).
    pub has_content: bool,
}

/// Breadcrumb navigation item.
#[derive(Debug, PartialEq, Eq)]
pub struct BreadcrumbItem {
    /// Display title.
    pub title: String,
    /// Link target path.
    pub path: String,
}

/// Result of rendering a markdown page.
#[derive(Debug)]
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
        match e.kind {
            StorageErrorKind::NotFound => Self::FileNotFound(
                e.path
                    .as_deref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ),
            _ => Self::Io(std::io::Error::other(e.to_string())),
        }
    }
}

/// Page rendering pipeline.
///
/// Handles markdown-to-HTML conversion with caching, diagram processing,
/// and metadata loading. Operates on individual pages without knowledge of
/// site structure or reload logic.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PageRenderer {
    storage: Arc<dyn Storage>,
    cache: Arc<dyn Cache>,
    page_bucket: Box<dyn CacheBucket>,
    extract_title: bool,
    kroki_url: Option<String>,
    include_dirs: Vec<PathBuf>,
    dpi: u32,
    relative_links: bool,
    trailing_slash: bool,
    static_tabs: bool,
}

impl PageRenderer {
    /// Create a new page renderer.
    pub(crate) fn new(
        storage: Arc<dyn Storage>,
        cache: Arc<dyn Cache>,
        config: PageRendererConfig,
    ) -> Self {
        Self {
            storage,
            page_bucket: cache.bucket("pages"),
            cache,
            extract_title: config.extract_title,
            kroki_url: config.kroki_url,
            include_dirs: config.include_dirs,
            dpi: config.dpi,
            relative_links: config.relative_links,
            trailing_slash: config.trailing_slash,
            static_tabs: config.static_tabs,
        }
    }

    /// Render a page with full pipeline: mtime, metadata, cache check, render, cache write.
    ///
    /// # Errors
    ///
    /// Returns `RenderError::FileNotFound` if source file doesn't exist.
    /// Returns `RenderError::Io` if file cannot be read.
    pub(crate) fn render(
        &self,
        path: &str,
        page: &Page,
        breadcrumbs: Vec<BreadcrumbItem>,
        meta_include_source: Option<Arc<dyn MetaIncludeSource>>,
    ) -> Result<PageRenderResult, RenderError> {
        if !page.has_content {
            return Ok(self.render_virtual(path, page, breadcrumbs));
        }

        let source_mtime = self
            .storage
            .mtime(path)
            .map_err(|_| RenderError::FileNotFound(path.to_owned()))?;

        let metadata = self.load_metadata(path);

        let etag = source_mtime.to_string();

        if let Some(cached) = self.page_bucket.get_json::<CachedPage>(path, &etag) {
            return Ok(PageRenderResult {
                html: cached.html,
                title: cached.title,
                toc: cached.toc,
                warnings: Vec::new(),
                from_cache: true,
                has_content: page.has_content,
                source_mtime,
                breadcrumbs,
                metadata,
            });
        }

        let markdown_text = self.storage.read(path)?;
        let result = self
            .create_renderer(path, meta_include_source)
            .render_markdown(&markdown_text);

        self.page_bucket.set_json(
            path,
            &etag,
            &CachedPageRef {
                html: &result.html,
                title: result.title.as_deref(),
                toc: &result.toc,
            },
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

    fn render_virtual(
        &self,
        path: &str,
        page: &Page,
        breadcrumbs: Vec<BreadcrumbItem>,
    ) -> PageRenderResult {
        let source_mtime = self.storage.mtime(path).unwrap_or(0.0);
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

    fn create_renderer(
        &self,
        base_path: &str,
        meta_include_source: Option<Arc<dyn MetaIncludeSource>>,
    ) -> MarkdownRenderer<HtmlBackend> {
        let tabs = if self.static_tabs {
            TabsDirective::new_static()
        } else {
            TabsDirective::new()
        };
        let directives = DirectiveProcessor::new().with_container(tabs);

        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_gfm(true)
            .with_base_path(format!("/{base_path}"))
            .with_relative_links(self.relative_links)
            .with_trailing_slash(self.trailing_slash)
            .with_directives(directives);

        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }

        if let Some(processor) = self.create_diagram_processor(base_path, meta_include_source) {
            renderer = renderer.with_processor(processor);
        }

        renderer
    }

    fn create_diagram_processor(
        &self,
        base_path: &str,
        meta_include_source: Option<Arc<dyn MetaIncludeSource>>,
    ) -> Option<DiagramProcessor> {
        let url = self.kroki_url.as_ref()?;

        let mut processor = DiagramProcessor::new(url)
            .include_dirs(&self.include_dirs)
            .dpi(self.dpi)
            .with_cache(self.cache.bucket("diagrams"));

        if let Some(source) = meta_include_source {
            processor = processor.with_meta_include_source(source);
        }

        if self.relative_links || self.trailing_slash {
            processor = processor.with_link_config(
                format!("/{base_path}"),
                self.relative_links,
                self.trailing_slash,
            );
        }

        Some(processor)
    }

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

/// Cached page data for deserialization (owned).
#[derive(Deserialize)]
struct CachedPage {
    html: String,
    title: Option<String>,
    toc: Vec<TocEntry>,
}

/// Borrowed view of cached page data for serialization (zero-copy).
#[derive(Serialize)]
struct CachedPageRef<'a> {
    html: &'a str,
    title: Option<&'a str>,
    toc: &'a [TocEntry],
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rw_cache::NullCache;
    use rw_storage::MockStorage;

    use super::*;

    fn create_renderer(storage: MockStorage) -> PageRenderer {
        let config = PageRendererConfig::default();
        PageRenderer::new(Arc::new(storage), Arc::new(NullCache), config)
    }

    fn make_page(title: &str, path: &str, has_content: bool) -> Page {
        Page {
            title: title.to_owned(),
            path: path.to_owned(),
            has_content,
        }
    }

    #[test]
    fn test_render_page_returns_html() {
        let storage = MockStorage::new()
            .with_file("test", "Hello", "# Hello\n\nWorld")
            .with_mtime("test", 1000.0);
        let renderer = create_renderer(storage);

        let page = make_page("Hello", "test", true);
        let result = renderer.render("test", &page, vec![], None).unwrap();

        assert!(result.html.contains("<p>World</p>"));
        assert_eq!(result.title, Some("Hello".to_owned()));
        assert!(!result.from_cache);
        assert!(result.has_content);
    }

    #[test]
    fn test_render_page_file_not_found() {
        let storage = MockStorage::new();
        let renderer = create_renderer(storage);

        let page = make_page("Missing", "missing", true);
        let result = renderer.render("missing", &page, vec![], None);

        assert!(matches!(result, Err(RenderError::FileNotFound(_))));
    }

    #[test]
    fn test_render_virtual_page() {
        let storage = MockStorage::new().with_mtime("my-domain", 1000.0);
        let renderer = create_renderer(storage);

        let page = make_page("My Domain", "my-domain", false);
        let result = renderer.render("my-domain", &page, vec![], None).unwrap();

        assert_eq!(result.html, "<h1>My Domain</h1>\n");
        assert_eq!(result.title, Some("My Domain".to_owned()));
        assert!(!result.has_content);
        assert!(result.toc.is_empty());
    }

    #[test]
    fn test_render_page_cache_hit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache: Arc<dyn rw_cache::Cache> = Arc::new(rw_cache::FileCache::new(
            temp_dir.path().join("cache"),
            "1.0.0",
        ));

        let storage = MockStorage::new()
            .with_file("test", "Cached", "# Cached\n\nContent")
            .with_mtime("test", 1000.0);

        let config = PageRendererConfig::default();
        let renderer = PageRenderer::new(Arc::new(storage), cache, config);
        let page = make_page("Cached", "test", true);

        let result1 = renderer.render("test", &page, vec![], None).unwrap();
        assert!(!result1.from_cache);

        let result2 = renderer.render("test", &page, vec![], None).unwrap();
        assert!(result2.from_cache);
        assert_eq!(result1.html, result2.html);
    }

    #[test]
    fn test_render_page_includes_metadata() {
        let metadata = Metadata {
            title: Some("Meta Title".to_owned()),
            description: Some("A description".to_owned()),
            ..Default::default()
        };
        let storage = MockStorage::new()
            .with_file("test", "Test", "# Test\n\nContent")
            .with_mtime("test", 1000.0)
            .with_metadata("test", metadata);

        let renderer = create_renderer(storage);
        let page = make_page("Test", "test", true);
        let result = renderer.render("test", &page, vec![], None).unwrap();

        let meta = result.metadata.unwrap();
        assert_eq!(meta.title, Some("Meta Title".to_owned()));
        assert_eq!(meta.description, Some("A description".to_owned()));
    }

    #[test]
    fn test_render_page_metadata_none_when_missing() {
        let storage = MockStorage::new()
            .with_file("test", "Test", "# Test")
            .with_mtime("test", 1000.0);

        let renderer = create_renderer(storage);
        let page = make_page("Test", "test", true);
        let result = renderer.render("test", &page, vec![], None).unwrap();

        assert!(result.metadata.is_none());
    }

    #[test]
    fn test_render_page_toc_generation() {
        let storage = MockStorage::new()
            .with_file("test", "Title", "# Title\n\n## Section 1\n\n## Section 2")
            .with_mtime("test", 1000.0);

        let renderer = create_renderer(storage);
        let page = make_page("Title", "test", true);
        let result = renderer.render("test", &page, vec![], None).unwrap();

        assert_eq!(result.toc.len(), 2);
        assert_eq!(result.toc[0].title, "Section 1");
        assert_eq!(result.toc[1].title, "Section 2");
    }
}
