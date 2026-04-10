//! Page rendering pipeline.
//!
//! Contains the internal [`PageRenderer`] that handles markdown-to-HTML
//! conversion, page caching, diagram processing, and metadata loading.
//! Also defines the public result and configuration types used by
//! [`Site`](crate::Site).

use std::path::PathBuf;
use std::sync::Arc;

use rw_cache::{Cache, CacheBucket, CacheBucketExt};
use rw_diagrams::{DiagramProcessor, MetaIncludeSource, SearchDiagramProcessor};
use rw_renderer::directive::DirectiveProcessor;
use rw_renderer::{
    HtmlBackend, MarkdownRenderer, RenderBackend, SearchDocumentBackend, TabsDirective, TocEntry,
    escape_html,
};
use rw_sections::{Section, Sections};

use crate::site::{SiteSnapshot, SiteTitleResolver};
use rw_storage::{Metadata, Storage, StorageError, StorageErrorKind};
use serde::{Deserialize, Serialize};

/// Per-render dependencies from the current site snapshot.
///
/// Bundles the shared state that changes on each site reload, keeping
/// [`PageRenderer`] decoupled from site-level types.
#[derive(Default)]
pub(crate) struct RenderContext {
    pub(crate) sections: Arc<Sections>,
    pub(crate) meta_include_source: Option<Arc<dyn MetaIncludeSource>>,
    pub(crate) snapshot: Option<Arc<SiteSnapshot>>,
}

/// Controls how [`Site`](crate::Site) renders markdown pages.
///
/// # Examples
///
/// ```
/// use rw_site::PageRendererConfig;
///
/// // Default: title extraction on, no diagram rendering
/// let config = PageRendererConfig::default();
/// assert!(config.extract_title);
/// assert!(config.kroki_url.is_none());
/// ```
///
/// ```
/// use rw_site::PageRendererConfig;
///
/// // Enable diagram rendering via a Kroki instance
/// let config = PageRendererConfig {
///     kroki_url: Some("https://kroki.io".to_owned()),
///     dpi: 144,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct PageRendererConfig {
    /// When `true`, the first `# H1` heading is extracted from the rendered
    /// HTML and returned separately in [`PageRenderResult::title`].
    pub extract_title: bool,
    /// Base URL of a [Kroki](https://kroki.io) instance for rendering diagrams.
    ///
    /// When `None`, fenced code blocks for diagram languages (`PlantUML`,
    /// Mermaid, etc.) are rendered as syntax-highlighted code instead of images.
    pub kroki_url: Option<String>,
    /// Directories to search when resolving `PlantUML` `!include` directives.
    /// Defaults to empty (no include resolution).
    pub include_dirs: Vec<PathBuf>,
    /// DPI for rendered diagram images. Defaults to `192` (retina).
    pub dpi: u32,
}

impl Default for PageRendererConfig {
    fn default() -> Self {
        Self {
            extract_title: true,
            kroki_url: None,
            include_dirs: Vec::new(),
            dpi: 192,
        }
    }
}

/// A single document in the site hierarchy.
///
/// Every entry in the navigation tree corresponds to a `Page`. Pages with
/// markdown content have [`has_content`](Self::has_content) set to `true`;
/// [virtual pages](crate#virtual-pages) (directories without `index.md`)
/// have it set to `false`.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// Display title, resolved from (in priority order): metadata `title`
    /// field, first `# H1` heading, or filename.
    pub title: String,
    /// URL path without leading slash (e.g., `"guide"`, `"domain/billing"`,
    /// `""` for the site root).
    pub path: String,
    /// Whether this page has markdown content. `false` for virtual pages
    /// that exist only as navigation containers.
    pub has_content: bool,
    /// Optional description from the page's metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Source directory name for content originating outside `source_dir`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Ordered list of child page slugs for navigation ordering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<String>>,
}

/// One segment of the breadcrumb trail leading to a page.
///
/// Breadcrumbs always start with a "Home" entry (path `""`) and include
/// each ancestor page up to (but not including) the current page.
///
/// # Examples
///
/// A page at `"domain/billing/overview"` produces breadcrumbs:
///
/// ```text
/// Home → Domain → Billing
/// ```
///
/// where "Home" has `path ""`, "Domain" has `path "domain"`, and
/// "Billing" has `path "domain/billing"`.
#[derive(Debug, PartialEq, Eq)]
pub struct BreadcrumbItem {
    /// Display title for this breadcrumb segment.
    pub title: String,
    /// URL path without leading slash. Empty string for the site root.
    pub path: String,
    /// Present when this breadcrumb's path is a [section](crate#sections-and-scoped-navigation) root.
    pub section: Option<Section>,
}

/// Output of rendering a single page via [`Site::render`](crate::Site::render).
///
/// Contains everything the frontend needs to display a page: rendered HTML,
/// extracted title, table of contents for the sidebar, breadcrumb trail,
/// and optional YAML metadata.
#[derive(Debug)]
pub struct PageRenderResult {
    /// Rendered HTML body content.
    pub html: String,
    /// Title extracted from the first `# H1` heading, or `None` if title
    /// extraction is disabled or the page has no H1.
    pub title: Option<String>,
    /// Headings found in the page, used to build a "table of contents" sidebar.
    pub toc: Vec<TocEntry>,
    /// Non-fatal issues encountered during rendering (e.g., unresolved
    /// `!include` directives). Intended for logging or developer tooling,
    /// not for display to end users.
    pub warnings: Vec<String>,
    /// `true` when the HTML was served from cache rather than re-rendered.
    pub from_cache: bool,
    /// `false` for [virtual pages](crate#virtual-pages) that have no markdown source.
    pub has_content: bool,
    /// Source file modification time as a Unix timestamp (seconds since
    /// epoch). Stored as `f64` for sub-second precision and JavaScript
    /// interoperability.
    pub source_mtime: f64,
    /// Ancestor trail from "Home" to the parent of this page.
    /// See [`BreadcrumbItem`] for the structure.
    pub breadcrumbs: Vec<BreadcrumbItem>,
    /// Page metadata from YAML frontmatter or sidecar `meta.yaml` file,
    /// if present.
    pub metadata: Option<Metadata>,
}

/// Plain text representation of a page for search indexing.
///
/// Produced by [`Site::render_search_document()`](crate::Site::render_search_document).
/// Contains whitespace-separated tokens suitable for full-text search engines.
#[derive(Debug, Clone)]
pub struct SearchDocument {
    /// Page title (from metadata or first H1 heading).
    pub title: String,
    /// Plain text content with whitespace-separated tokens.
    pub text: String,
}

/// Reasons why [`Site::render`](crate::Site::render) can fail.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// The page exists in the site structure but its markdown source file
    /// is missing from storage.
    #[error("Content not found: {0}")]
    FileNotFound(String),
    /// No page with this URL path exists in the site structure. The path
    /// may be misspelled or the site may need a reload.
    #[error("Page not found: {0}")]
    PageNotFound(String),
    /// I/O error while reading the markdown source file.
    #[error("I/O error: {0}")]
    Io(#[source] std::io::Error),
    /// The storage backend itself failed (e.g., S3 connectivity issues,
    /// permission errors).
    #[error("Storage error: {0}")]
    Storage(#[source] StorageError),
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
            _ => Self::Storage(e),
        }
    }
}

/// Apply section references to breadcrumb items.
pub(crate) fn apply_breadcrumb_sections(breadcrumbs: &mut [BreadcrumbItem], sections: &Sections) {
    for crumb in breadcrumbs.iter_mut() {
        if let Some(sr) = sections.get(&crumb.path) {
            crumb.section = Some(sr.clone());
        }
    }
}

/// Page rendering pipeline.
///
/// Handles markdown-to-HTML conversion with caching, diagram processing,
/// and metadata loading. Operates on individual pages without knowledge of
/// site structure or reload logic.
pub(crate) struct PageRenderer {
    storage: Arc<dyn Storage>,
    cache: Arc<dyn Cache>,
    page_bucket: Box<dyn CacheBucket>,
    extract_title: bool,
    kroki_url: Option<String>,
    include_dirs: Vec<PathBuf>,
    dpi: u32,
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
        }
    }

    /// Clear all cached rendered pages.
    ///
    /// Called on site reload to prevent serving stale content when the
    /// underlying storage has changed (e.g., new S3 bundle published).
    pub(crate) fn clear_pages(&self) {
        self.page_bucket.clear();
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
        ctx: &RenderContext,
    ) -> Result<PageRenderResult, RenderError> {
        let mut result = if page.has_content {
            self.render_content(path, page, breadcrumbs, ctx)?
        } else {
            self.render_virtual(path, page, breadcrumbs)
        };

        apply_breadcrumb_sections(&mut result.breadcrumbs, &ctx.sections);

        Ok(result)
    }

    fn render_content(
        &self,
        path: &str,
        page: &Page,
        breadcrumbs: Vec<BreadcrumbItem>,
        ctx: &RenderContext,
    ) -> Result<PageRenderResult, RenderError> {
        let source_mtime = self.storage.mtime(path).map_err(RenderError::from)?;

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
            .create_renderer(path, page.origin.as_deref(), ctx)
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

    pub(crate) fn render_search_document(
        &self,
        path: &str,
        page: &Page,
        ctx: &RenderContext,
    ) -> Result<Option<SearchDocument>, RenderError> {
        if !page.has_content {
            return Ok(None);
        }

        let markdown_text = self.storage.read(path)?;
        let metadata = self.load_metadata(path);

        let mut search_processor = SearchDiagramProcessor::new(self.include_dirs.clone());
        if let Some(source) = &ctx.meta_include_source {
            search_processor = search_processor.with_meta_include_source(Arc::clone(source));
        }

        let mut renderer = Self::configure_renderer(
            MarkdownRenderer::<SearchDocumentBackend>::new().with_title_extraction(),
            ctx,
        )
        .with_processor(search_processor);

        let result = renderer.render_markdown(&markdown_text);

        let title = metadata
            .as_ref()
            .and_then(|m| m.title.clone())
            .or(result.title)
            .unwrap_or_else(|| page.title.clone());

        Ok(Some(SearchDocument {
            title,
            text: result.html,
        }))
    }

    fn create_renderer(
        &self,
        base_path: &str,
        origin: Option<&str>,
        ctx: &RenderContext,
    ) -> MarkdownRenderer<HtmlBackend> {
        let mut renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_base_path(format!("/{base_path}"));

        if let Some(origin) = origin {
            renderer = renderer.with_origin(origin);
        }

        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }

        let renderer = Self::configure_renderer(renderer, ctx);

        if let Some(processor) = self.create_diagram_processor(ctx.meta_include_source.clone()) {
            renderer.with_processor(processor.with_sections(Arc::clone(&ctx.sections)))
        } else {
            renderer
        }
    }

    /// Apply common renderer configuration: GFM, directives, sections, wikilinks.
    fn configure_renderer<B: RenderBackend>(
        renderer: MarkdownRenderer<B>,
        ctx: &RenderContext,
    ) -> MarkdownRenderer<B> {
        let directives = DirectiveProcessor::new().with_container(TabsDirective::new());

        let mut renderer = renderer
            .with_gfm(true)
            .with_directives(directives)
            .with_sections(Arc::clone(&ctx.sections));

        if let Some(snapshot) = &ctx.snapshot {
            renderer = renderer
                .with_wikilinks(true)
                .with_title_resolver(SiteTitleResolver {
                    snapshot: Arc::clone(snapshot),
                });
        }

        renderer
    }

    fn create_diagram_processor(
        &self,
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
            description: None,
            origin: None,
            pages: None,
        }
    }

    #[test]
    fn test_render_page_returns_html() {
        let storage = MockStorage::new()
            .with_file("test", "Hello", "# Hello\n\nWorld")
            .with_mtime("test", 1000.0);
        let renderer = create_renderer(storage);

        let page = make_page("Hello", "test", true);
        let result = renderer
            .render("test", &page, vec![], &RenderContext::default())
            .unwrap();

        assert!(result.html.contains("<p>World</p>"));
        assert_eq!(result.title, Some("Hello".to_owned()));
        assert!(!result.from_cache);
        assert!(result.has_content);
    }

    #[test]
    fn test_render_readme_with_origin_resolves_links_correctly() {
        let storage = MockStorage::new()
            .with_file("", "Home", "# Home\n\n[Guide](docs/guide.md)")
            .with_mtime("", 1000.0);
        let renderer = create_renderer(storage);

        let mut page = make_page("Home", "", true);
        page.origin = Some("docs".to_owned());
        let result = renderer
            .render("", &page, vec![], &RenderContext::default())
            .unwrap();

        assert!(
            result.html.contains(r#"href="/guide""#),
            "Expected href=\"/guide\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_render_page_file_not_found() {
        let storage = MockStorage::new();
        let renderer = create_renderer(storage);

        let page = make_page("Missing", "missing", true);
        let result = renderer.render("missing", &page, vec![], &RenderContext::default());

        assert!(matches!(result, Err(RenderError::FileNotFound(_))));
    }

    #[test]
    fn test_render_virtual_page() {
        let storage = MockStorage::new().with_mtime("my-domain", 1000.0);
        let renderer = create_renderer(storage);

        let page = make_page("My Domain", "my-domain", false);
        let result = renderer
            .render("my-domain", &page, vec![], &RenderContext::default())
            .unwrap();

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

        let result1 = renderer
            .render("test", &page, vec![], &RenderContext::default())
            .unwrap();
        assert!(!result1.from_cache);

        let result2 = renderer
            .render("test", &page, vec![], &RenderContext::default())
            .unwrap();
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
        let result = renderer
            .render("test", &page, vec![], &RenderContext::default())
            .unwrap();

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
        let result = renderer
            .render("test", &page, vec![], &RenderContext::default())
            .unwrap();

        assert!(result.metadata.is_none());
    }

    #[test]
    fn test_render_page_toc_generation() {
        let storage = MockStorage::new()
            .with_file("test", "Title", "# Title\n\n## Section 1\n\n## Section 2")
            .with_mtime("test", 1000.0);

        let renderer = create_renderer(storage);
        let page = make_page("Title", "test", true);
        let result = renderer
            .render("test", &page, vec![], &RenderContext::default())
            .unwrap();

        assert_eq!(result.toc.len(), 2);
        assert_eq!(result.toc[0].title, "Section 1");
        assert_eq!(result.toc[1].title, "Section 2");
    }

    #[test]
    fn test_render_search_document() {
        let storage = MockStorage::new()
            .with_file("test", "Hello", "# Hello\n\nWorld **bold** and `code`.")
            .with_mtime("test", 1000.0);
        let renderer = create_renderer(storage);

        let page = make_page("Hello", "test", true);
        let result = renderer
            .render_search_document("test", &page, &RenderContext::default())
            .unwrap();

        let doc = result.unwrap();
        assert_eq!(doc.title, "Hello");
        assert!(doc.text.contains("World"));
        assert!(doc.text.contains("bold"));
        assert!(doc.text.contains("code"));
        assert!(!doc.text.contains('<'));
    }

    #[test]
    fn test_render_search_document_virtual_page_returns_none() {
        let storage = MockStorage::new().with_mtime("virtual", 1000.0);
        let renderer = create_renderer(storage);

        let page = make_page("Virtual", "virtual", false);
        let result = renderer
            .render_search_document("virtual", &page, &RenderContext::default())
            .unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_render_search_document_uses_metadata_title() {
        let metadata = Metadata {
            title: Some("Meta Title".to_owned()),
            ..Default::default()
        };
        let storage = MockStorage::new()
            .with_file("test", "H1 Title", "# H1 Title\n\nContent")
            .with_mtime("test", 1000.0)
            .with_metadata("test", metadata);

        let renderer = create_renderer(storage);
        let page = make_page("H1 Title", "test", true);
        let result = renderer
            .render_search_document("test", &page, &RenderContext::default())
            .unwrap()
            .unwrap();

        assert_eq!(result.title, "Meta Title");
    }

    #[test]
    fn test_render_search_document_file_not_found() {
        let storage = MockStorage::new();
        let renderer = create_renderer(storage);

        let page = make_page("Missing", "missing", true);
        let result = renderer.render_search_document("missing", &page, &RenderContext::default());

        assert!(matches!(result, Err(RenderError::FileNotFound(_))));
    }

    #[test]
    fn test_render_search_document_falls_back_to_page_title() {
        let storage = MockStorage::new()
            .with_file("test", "Test", "Some text without a heading")
            .with_mtime("test", 1000.0);
        let renderer = create_renderer(storage);

        let page = make_page("Fallback Title", "test", true);
        let result = renderer
            .render_search_document("test", &page, &RenderContext::default())
            .unwrap()
            .unwrap();

        assert_eq!(result.title, "Fallback Title");
    }
}
