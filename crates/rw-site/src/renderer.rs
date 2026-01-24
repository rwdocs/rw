//! Page rendering with caching.
//!
//! Provides [`PageRenderer`] for rendering markdown to HTML with file-based caching
//! and mtime-based invalidation.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rw_diagrams::{DiagramProcessor, FileCache};
use rw_renderer::{HtmlBackend, MarkdownRenderer, TocEntry};

use crate::page_cache::{FilePageCache, NullPageCache, PageCache};

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
}

/// Error returned when page rendering fails.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// Source file not found.
    #[error("Source file not found: {}", .0.display())]
    FileNotFound(PathBuf),
    /// I/O error reading source file.
    #[error("I/O error: {0}")]
    Io(#[source] std::io::Error),
}

/// Configuration for [`PageRenderer`].
#[derive(Clone, Debug)]
pub struct PageRendererConfig {
    /// Cache directory for rendered pages and metadata.
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

impl Default for PageRendererConfig {
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

/// Page renderer with file-based caching.
///
/// Uses [`MarkdownRenderer`] for rendering and [`PageCache`] for persistence.
/// Cache invalidation is based on source file mtime and build version.
///
/// # Example
///
/// ```ignore
/// use std::path::PathBuf;
/// use rw_site::{PageRenderer, PageRendererConfig};
///
/// let config = PageRendererConfig {
///     cache_dir: Some(PathBuf::from(".cache")),
///     version: "1.0.0".to_string(),
///     extract_title: true,
///     kroki_url: Some("https://kroki.io".to_string()),
///     include_dirs: vec![PathBuf::from(".")],
///     config_file: None,
///     dpi: 192,
/// };
///
/// let renderer = PageRenderer::new(config);
/// let result = renderer.render(Path::new("docs/guide.md"), "guide")?;
/// ```
pub struct PageRenderer {
    cache: Box<dyn PageCache>,
    extract_title: bool,
    kroki_url: Option<String>,
    include_dirs: Vec<PathBuf>,
    config_file: Option<String>,
    dpi: u32,
}

impl PageRenderer {
    /// Create a new page renderer with the given configuration.
    #[must_use]
    pub fn new(config: PageRendererConfig) -> Self {
        let cache: Box<dyn PageCache> = match &config.cache_dir {
            Some(dir) => Box::new(FilePageCache::new(dir.clone(), config.version.clone())),
            None => Box::new(NullPageCache),
        };

        Self {
            cache,
            extract_title: config.extract_title,
            kroki_url: config.kroki_url,
            include_dirs: config.include_dirs,
            config_file: config.config_file,
            dpi: config.dpi,
        }
    }

    /// Render a markdown page.
    ///
    /// # Arguments
    ///
    /// * `source_path` - Absolute path to markdown source file
    /// * `base_path` - URL path for resolving relative links (e.g., "domain-a/guide")
    ///
    /// # Returns
    ///
    /// `PageRenderResult` with HTML, title, and `ToC`
    ///
    /// # Errors
    ///
    /// Returns `RenderError::FileNotFound` if source file doesn't exist.
    /// Returns `RenderError::Io` if file cannot be read.
    pub fn render(
        &self,
        source_path: &Path,
        base_path: &str,
    ) -> Result<PageRenderResult, RenderError> {
        if !source_path.exists() {
            return Err(RenderError::FileNotFound(source_path.to_path_buf()));
        }

        let source_mtime = get_mtime(source_path)?;

        if let Some(cached) = self.cache.get(base_path, source_mtime) {
            return Ok(PageRenderResult {
                html: cached.html,
                title: cached.meta.title,
                toc: cached.meta.toc,
                warnings: Vec::new(),
                from_cache: true,
            });
        }

        let markdown_text = fs::read_to_string(source_path).map_err(RenderError::Io)?;

        let mut renderer = self.create_renderer(base_path);
        if let Some(processor) = self.create_diagram_processor() {
            renderer = renderer.with_processor(processor);
        }

        let result = renderer.render_markdown(&markdown_text);

        self.cache.set(
            base_path,
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
        })
    }

    /// Create a renderer with common configuration.
    fn create_renderer(&self, base_path: &str) -> MarkdownRenderer<HtmlBackend> {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_gfm(true)
            .with_base_path(base_path);

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

        if let Some(dir) = self.cache.diagrams_dir() {
            processor = processor.with_cache(Arc::new(FileCache::new(dir.to_path_buf())));
        }

        Some(processor)
    }

    /// Invalidate cache entry for a path.
    ///
    /// # Arguments
    ///
    /// * `path` - Document path to invalidate
    pub fn invalidate(&self, path: &str) {
        self.cache.invalidate(path);
    }
}

/// Get file modification time as seconds since Unix epoch.
fn get_mtime(path: &Path) -> Result<f64, RenderError> {
    path.metadata()
        .map_err(RenderError::Io)?
        .modified()
        .map_err(RenderError::Io)
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map_or(0.0, |d| d.as_secs_f64())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_temp_md(content: &str) -> (tempfile::TempDir, PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        (temp_dir, file_path)
    }

    #[test]
    fn test_render_simple_markdown() {
        let (_temp_dir, file_path) = create_temp_md("# Hello\n\nWorld");

        let config = PageRendererConfig {
            extract_title: true,
            ..Default::default()
        };
        let renderer = PageRenderer::new(config);

        let result = renderer.render(&file_path, "test").unwrap();
        assert!(result.html.contains("<p>World</p>"));
        assert_eq!(result.title, Some("Hello".to_string()));
        assert!(!result.from_cache);
    }

    #[test]
    fn test_render_file_not_found() {
        let config = PageRendererConfig::default();
        let renderer = PageRenderer::new(config);

        let result = renderer.render(Path::new("/nonexistent/file.md"), "test");
        assert!(matches!(result, Err(RenderError::FileNotFound(_))));
    }

    #[test]
    fn test_render_with_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "# Cached\n\nContent").unwrap();

        let config = PageRendererConfig {
            cache_dir: Some(cache_dir),
            version: "1.0.0".to_string(),
            extract_title: true,
            ..Default::default()
        };
        let renderer = PageRenderer::new(config);

        // First render - cache miss
        let result1 = renderer.render(&file_path, "test").unwrap();
        assert!(!result1.from_cache);
        assert_eq!(result1.title, Some("Cached".to_string()));

        // Second render - cache hit
        let result2 = renderer.render(&file_path, "test").unwrap();
        assert!(result2.from_cache);
        assert_eq!(result2.title, Some("Cached".to_string()));
        assert_eq!(result1.html, result2.html);
    }

    #[test]
    fn test_render_cache_invalidation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "# Original\n\nContent").unwrap();

        let config = PageRendererConfig {
            cache_dir: Some(cache_dir),
            version: "1.0.0".to_string(),
            extract_title: true,
            ..Default::default()
        };
        let renderer = PageRenderer::new(config);

        // Render and cache
        let result1 = renderer.render(&file_path, "test").unwrap();
        assert!(!result1.from_cache);

        // Invalidate
        renderer.invalidate("test");

        // Re-render - should be cache miss
        let result2 = renderer.render(&file_path, "test").unwrap();
        assert!(!result2.from_cache);
    }

    #[test]
    fn test_render_mtime_invalidation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "# Version1\n\nContent").unwrap();

        let config = PageRendererConfig {
            cache_dir: Some(cache_dir),
            version: "1.0.0".to_string(),
            extract_title: true,
            ..Default::default()
        };
        let renderer = PageRenderer::new(config);

        // First render
        let result1 = renderer.render(&file_path, "test").unwrap();
        assert!(!result1.from_cache);
        assert_eq!(result1.title, Some("Version1".to_string()));

        // Wait a moment and modify file
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&file_path, "# Version2\n\nUpdated").unwrap();

        // Re-render - should be cache miss due to mtime change
        let result2 = renderer.render(&file_path, "test").unwrap();
        assert!(!result2.from_cache);
        assert_eq!(result2.title, Some("Version2".to_string()));
    }

    #[test]
    fn test_render_toc_generation() {
        let (_temp_dir, file_path) = create_temp_md("# Title\n\n## Section 1\n\n## Section 2");

        let config = PageRendererConfig {
            extract_title: true,
            ..Default::default()
        };
        let renderer = PageRenderer::new(config);

        let result = renderer.render(&file_path, "test").unwrap();
        assert_eq!(result.toc.len(), 2);
        assert_eq!(result.toc[0].title, "Section 1");
        assert_eq!(result.toc[0].level, 2);
        assert_eq!(result.toc[1].title, "Section 2");
    }
}
