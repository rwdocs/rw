//! Markdown converter with multiple output formats.
//!
//! This module provides [`MarkdownConverter`], the main entry point for converting
//! `CommonMark` documents to either Confluence XHTML or semantic HTML5.
//!
//! # Features
//!
//! - GitHub Flavored Markdown support (tables, strikethrough, task lists)
//! - Title extraction from first H1 heading
//! - Table of contents generation
//! - `PlantUML` diagram rendering via Kroki service
//! - Configurable DPI for diagram output
//!
//! # Example
//!
//! ```ignore
//! use std::path::Path;
//! use docstage_core::MarkdownConverter;
//!
//! let converter = MarkdownConverter::new()
//!     .extract_title(true)
//!     .dpi(192);
//!
//! // Convert to HTML
//! let result = converter.convert_html("# Hello\n\nWorld");
//! println!("{}", result.html);
//!
//! // Convert to Confluence with diagram rendering
//! let result = converter.convert(
//!     "# Hello\n\n```plantuml\nA -> B\n```",
//!     "https://kroki.io",
//!     Path::new("/tmp/diagrams"),
//! )?;
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use docstage_confluence::ConfluenceBackend;
use docstage_diagrams::{DiagramCache, DiagramOutput, DiagramProcessor, FileCache, NullCache};
use docstage_renderer::{HtmlBackend, MarkdownRenderer, RenderResult, TocEntry};

use crate::confluence_tags::ConfluenceTagGenerator;

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Result of converting markdown to Confluence format.
///
/// Type alias for `RenderResult` from `docstage-renderer`.
#[deprecated(since = "0.2.0", note = "use RenderResult instead")]
pub type ConvertResult = RenderResult;

/// Result of converting markdown to HTML format.
///
/// Type alias for `RenderResult` from `docstage-renderer`.
#[deprecated(since = "0.2.0", note = "use RenderResult instead")]
pub type HtmlConvertResult = RenderResult;

/// Markdown to Confluence converter configuration.
#[derive(Clone, Debug)]
pub struct MarkdownConverter {
    gfm: bool,
    prepend_toc: bool,
    extract_title: bool,
    include_dirs: Vec<PathBuf>,
    config_file: Option<String>,
    dpi: Option<u32>,
}

impl Default for MarkdownConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownConverter {
    /// Create a new converter with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gfm: true,
            prepend_toc: false,
            extract_title: false,
            include_dirs: Vec::new(),
            config_file: None,
            dpi: None,
        }
    }

    /// Enable or disable GitHub Flavored Markdown features.
    #[must_use]
    pub fn gfm(mut self, enabled: bool) -> Self {
        self.gfm = enabled;
        self
    }

    /// Enable or disable prepending a table of contents macro.
    #[must_use]
    pub fn prepend_toc(mut self, enabled: bool) -> Self {
        self.prepend_toc = enabled;
        self
    }

    /// Enable or disable extracting the first H1 as page title.
    #[must_use]
    pub fn extract_title(mut self, enabled: bool) -> Self {
        self.extract_title = enabled;
        self
    }

    /// Set directories to search for `PlantUML` includes.
    #[must_use]
    pub fn include_dirs(mut self, dirs: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        self.include_dirs = dirs.into_iter().map(Into::into).collect();
        self
    }

    /// Set `PlantUML` config file (loaded from `include_dirs` when needed).
    #[must_use]
    pub fn config_file(mut self, config_file: Option<&str>) -> Self {
        self.config_file = config_file.map(String::from);
        self
    }

    /// Set DPI for `PlantUML` diagram rendering.
    ///
    /// Default is 192 (2x for retina displays). Set to 96 for standard resolution.
    #[must_use]
    pub fn dpi(mut self, dpi: u32) -> Self {
        self.dpi = Some(dpi);
        self
    }

    /// Prepend TOC macro if enabled and there are headings.
    fn maybe_prepend_toc(&self, html: String, toc: &[TocEntry]) -> String {
        if self.prepend_toc && !toc.is_empty() {
            format!("{TOC_MACRO}{html}")
        } else {
            html
        }
    }

    /// Convert markdown to Confluence storage format with diagram rendering via Kroki.
    #[must_use]
    pub fn convert(&self, markdown_text: &str, kroki_url: &str, output_dir: &Path) -> RenderResult {
        let processor = self
            .create_diagram_processor(kroki_url)
            .output(DiagramOutput::Files {
                output_dir: output_dir.to_path_buf(),
                tag_generator: Arc::new(ConfluenceTagGenerator),
            });

        let mut renderer = self.create_confluence_renderer().with_processor(processor);
        let result = renderer.render_markdown(markdown_text);

        RenderResult {
            html: self.maybe_prepend_toc(result.html, &result.toc),
            title: result.title,
            toc: result.toc,
            warnings: result.warnings,
        }
    }

    /// Convert markdown to HTML format without diagram rendering.
    ///
    /// For rendered diagram images, use `convert_html_with_diagrams()`.
    #[must_use]
    pub fn convert_html(&self, markdown_text: &str, base_path: Option<&str>) -> RenderResult {
        self.create_html_renderer(base_path)
            .render_markdown(markdown_text)
    }

    /// Convert markdown to HTML format with diagram rendering via Kroki.
    #[must_use]
    pub fn convert_html_with_diagrams(
        &self,
        markdown_text: &str,
        kroki_url: &str,
        base_path: Option<&str>,
    ) -> RenderResult {
        self.render_html_with_diagrams(markdown_text, kroki_url, None, base_path)
    }

    /// Convert markdown to HTML format with cached diagram rendering.
    #[must_use]
    pub fn convert_html_with_diagrams_cached(
        &self,
        markdown_text: &str,
        kroki_url: &str,
        cache_dir: Option<&Path>,
        base_path: Option<&str>,
    ) -> RenderResult {
        let cache: Arc<dyn DiagramCache> = match cache_dir {
            Some(dir) => Arc::new(FileCache::new(dir.to_path_buf())),
            None => Arc::new(NullCache),
        };
        self.render_html_with_diagrams(markdown_text, kroki_url, Some(cache), base_path)
    }

    /// Create a diagram processor with common configuration.
    fn create_diagram_processor(&self, kroki_url: &str) -> DiagramProcessor {
        let mut processor = DiagramProcessor::new(kroki_url)
            .include_dirs(&self.include_dirs)
            .config_file(self.config_file.as_deref());

        if let Some(dpi) = self.dpi {
            processor = processor.dpi(dpi);
        }
        processor
    }

    /// Create an HTML renderer with common configuration.
    fn create_html_renderer(&self, base_path: Option<&str>) -> MarkdownRenderer<HtmlBackend> {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_gfm(self.gfm);
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        if let Some(path) = base_path {
            renderer = renderer.with_base_path(path);
        }
        renderer
    }

    /// Create a Confluence renderer with common configuration.
    fn create_confluence_renderer(&self) -> MarkdownRenderer<ConfluenceBackend> {
        let mut renderer = MarkdownRenderer::<ConfluenceBackend>::new().with_gfm(self.gfm);
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        renderer
    }

    /// Internal helper for HTML rendering with optional diagram caching.
    fn render_html_with_diagrams(
        &self,
        markdown_text: &str,
        kroki_url: &str,
        cache: Option<Arc<dyn DiagramCache>>,
        base_path: Option<&str>,
    ) -> RenderResult {
        let mut processor = self.create_diagram_processor(kroki_url);
        if let Some(c) = cache {
            processor = processor.with_cache(c);
        }

        self.create_html_renderer(base_path)
            .with_processor(processor)
            .render_markdown(markdown_text)
    }
}
