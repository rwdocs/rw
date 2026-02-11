//! Page rendering pipeline.
//!
//! [`PageRenderer`] handles markdown-to-HTML conversion, page caching,
//! diagram processing, and metadata loading. Extracted from [`Site`](crate::Site)
//! to enable independent testing of the rendering pipeline.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use rw_cache::{Cache, CacheBucket, CacheBucketExt};
use rw_diagrams::{DiagramProcessor, MetaIncludeSource};
use rw_renderer::directive::DirectiveProcessor;
use rw_renderer::{HtmlBackend, MarkdownRenderer, TabsDirective, TocEntry, escape_html};
use rw_storage::{Metadata, Storage};
use serde::{Deserialize, Serialize};

use crate::site::{BreadcrumbItem, Page, PageRenderResult, RenderError, SiteConfig};

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
    meta_include_source: RwLock<Option<Arc<dyn MetaIncludeSource>>>,
}

impl PageRenderer {
    /// Create a new page renderer.
    pub(crate) fn new(
        storage: Arc<dyn Storage>,
        config: SiteConfig,
        cache: Arc<dyn Cache>,
    ) -> Self {
        Self {
            storage,
            page_bucket: cache.bucket("pages"),
            cache,
            extract_title: config.extract_title,
            kroki_url: config.kroki_url,
            include_dirs: config.include_dirs,
            dpi: config.dpi,
            meta_include_source: RwLock::new(None),
        }
    }

    /// Update the meta include source for diagram processing.
    ///
    /// Called by `Site` after rebuilding the `TypedPageRegistry` during reload.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    pub(crate) fn set_meta_include_source(&self, source: Arc<dyn MetaIncludeSource>) {
        *self.meta_include_source.write().unwrap() = Some(source);
    }

    /// Render a page with full pipeline: mtime, metadata, cache check, render, cache write.
    ///
    /// # Errors
    ///
    /// Returns `RenderError::FileNotFound` if source file doesn't exist.
    /// Returns `RenderError::Io` if file cannot be read.
    pub(crate) fn render_page(
        &self,
        path: &str,
        page: &Page,
        breadcrumbs: Vec<BreadcrumbItem>,
    ) -> Result<PageRenderResult, RenderError> {
        if !page.has_content {
            return Ok(self.render_virtual_page(path, page, breadcrumbs));
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
        let result = self.create_renderer(path).render_markdown(&markdown_text);

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

    fn render_virtual_page(
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

    fn create_diagram_processor(&self) -> Option<DiagramProcessor> {
        let url = self.kroki_url.as_ref()?;

        let mut processor = DiagramProcessor::new(url)
            .include_dirs(&self.include_dirs)
            .dpi(self.dpi)
            .with_cache(self.cache.bucket("diagrams"));

        if let Some(source) = self.meta_include_source.read().unwrap().clone() {
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
        let config = SiteConfig::default();
        PageRenderer::new(Arc::new(storage), config, Arc::new(NullCache))
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
        let result = renderer.render_page("test", &page, vec![]).unwrap();

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
        let result = renderer.render_page("missing", &page, vec![]);

        assert!(matches!(result, Err(RenderError::FileNotFound(_))));
    }

    #[test]
    fn test_render_virtual_page() {
        let storage = MockStorage::new().with_mtime("my-domain", 1000.0);
        let renderer = create_renderer(storage);

        let page = make_page("My Domain", "my-domain", false);
        let result = renderer.render_page("my-domain", &page, vec![]).unwrap();

        assert_eq!(result.html, "<h1>My Domain</h1>\n");
        assert_eq!(result.title, Some("My Domain".to_owned()));
        assert!(!result.has_content);
        assert!(result.toc.is_empty());
    }

    #[test]
    fn test_render_page_cache_hit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache: Arc<dyn rw_cache::Cache> =
            Arc::new(rw_cache::FileCache::new(temp_dir.path().join("cache"), "1.0.0"));

        let storage = MockStorage::new()
            .with_file("test", "Cached", "# Cached\n\nContent")
            .with_mtime("test", 1000.0);

        let config = SiteConfig::default();
        let renderer = PageRenderer::new(Arc::new(storage), config, cache);
        let page = make_page("Cached", "test", true);

        let result1 = renderer.render_page("test", &page, vec![]).unwrap();
        assert!(!result1.from_cache);

        let result2 = renderer.render_page("test", &page, vec![]).unwrap();
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
        let result = renderer.render_page("test", &page, vec![]).unwrap();

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
        let result = renderer.render_page("test", &page, vec![]).unwrap();

        assert!(result.metadata.is_none());
    }

    #[test]
    fn test_render_page_toc_generation() {
        let storage = MockStorage::new()
            .with_file("test", "Title", "# Title\n\n## Section 1\n\n## Section 2")
            .with_mtime("test", 1000.0);

        let renderer = create_renderer(storage);
        let page = make_page("Title", "test", true);
        let result = renderer.render_page("test", &page, vec![]).unwrap();

        assert_eq!(result.toc.len(), 2);
        assert_eq!(result.toc[0].title, "Section 1");
        assert_eq!(result.toc[1].title, "Section 2");
    }
}
