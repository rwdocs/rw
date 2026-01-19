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

use pulldown_cmark::{Options, Parser};

use docstage_confluence_renderer::ConfluenceBackend;
use docstage_diagrams::{
    DiagramCache, DiagramOutput, DiagramProcessor, FileCache, NullCache, RenderError,
};
use docstage_renderer::{HtmlBackend, MarkdownRenderer, TocEntry};

use crate::ConfluenceTagGenerator;

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Result of converting markdown to Confluence format.
#[derive(Clone, Debug)]
pub struct ConvertResult {
    pub html: String,
    pub title: Option<String>,
    /// Warnings generated during conversion (e.g., unresolved includes).
    /// Used by Python CLI in verbose mode to log diagnostic info to stderr.
    pub warnings: Vec<String>,
}

/// Result of converting markdown to HTML format.
#[derive(Clone, Debug)]
pub struct HtmlConvertResult {
    /// Rendered HTML content.
    pub html: String,
    /// Title extracted from first H1 heading (if `extract_title` was enabled).
    pub title: Option<String>,
    /// Table of contents entries.
    pub toc: Vec<TocEntry>,
    /// Warnings generated during conversion (e.g., unresolved includes).
    pub warnings: Vec<String>,
}

/// Markdown to Confluence converter configuration.
#[derive(Clone, Debug)]
pub struct MarkdownConverter {
    gfm: bool,
    prepend_toc: bool,
    extract_title: bool,
    include_dirs: Vec<PathBuf>,
    /// PlantUML config file name (loaded from include_dirs when needed).
    config_file: Option<String>,
    /// DPI for `PlantUML` diagram rendering (None = default 192).
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

    /// Set `PlantUML` config file (loaded from include_dirs when needed).
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

    fn get_parser_options(&self) -> Options {
        let mut options = Options::empty();
        if self.gfm {
            options.insert(Options::ENABLE_TABLES);
            options.insert(Options::ENABLE_STRIKETHROUGH);
            options.insert(Options::ENABLE_TASKLISTS);
        }
        options
    }

    /// Optionally prepend TOC macro to HTML content.
    ///
    /// Only prepends when `prepend_toc` is enabled AND there are headings.
    fn maybe_prepend_toc(&self, html: String, toc: &[TocEntry]) -> String {
        if self.prepend_toc && !toc.is_empty() {
            format!("{TOC_MACRO}{html}")
        } else {
            html
        }
    }

    /// Convert markdown to Confluence storage format.
    ///
    /// Diagrams are rendered via Kroki and placeholders replaced with
    /// Confluence image macros. Supports all diagram types: `PlantUML`,
    /// Mermaid, `GraphViz`, and 14+ other Kroki-supported formats.
    ///
    /// # Errors
    ///
    /// Returns `RenderError` if diagram rendering fails.
    pub fn convert(
        &self,
        markdown_text: &str,
        kroki_url: &str,
        output_dir: &Path,
    ) -> Result<ConvertResult, RenderError> {
        let options = self.get_parser_options();
        let parser = Parser::new_ext(markdown_text, options);

        // Create DiagramProcessor with file-based output
        let mut processor = DiagramProcessor::new()
            .kroki_url(kroki_url)
            .include_dirs(&self.include_dirs)
            .config_file(self.config_file.as_deref())
            .output(DiagramOutput::Files {
                output_dir: output_dir.to_path_buf(),
                tag_generator: Arc::new(ConfluenceTagGenerator),
            });

        if let Some(dpi) = self.dpi {
            processor = processor.dpi(dpi);
        }

        let mut renderer = MarkdownRenderer::<ConfluenceBackend>::new();
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        renderer = renderer.with_processor(processor);

        let result = renderer.render(parser);
        let html = renderer.finalize(result.html);
        let warnings = renderer.processor_warnings();

        let html = self.maybe_prepend_toc(html, &result.toc);

        Ok(ConvertResult {
            html,
            title: result.title,
            warnings,
        })
    }

    /// Convert markdown to HTML format.
    ///
    /// Produces semantic HTML5 with syntax highlighting and table of contents.
    /// Diagram code blocks are rendered with syntax highlighting as-is.
    /// For rendered diagram images, use `convert_html_with_diagrams()`.
    ///
    /// # Arguments
    ///
    /// * `markdown_text` - Markdown source text
    /// * `base_path` - Optional base path for resolving relative links (e.g., "domains/billing/guide").
    ///   When provided, relative `.md` links are transformed to absolute paths (e.g., `/domains/billing/page`).
    #[must_use]
    pub fn convert_html(&self, markdown_text: &str, base_path: Option<&str>) -> HtmlConvertResult {
        let options = self.get_parser_options();
        let parser = Parser::new_ext(markdown_text, options);

        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        if let Some(path) = base_path {
            renderer = renderer.with_base_path(path);
        }

        let result = renderer.render(parser);

        HtmlConvertResult {
            html: result.html,
            title: result.title,
            toc: result.toc,
            warnings: Vec::new(),
        }
    }

    /// Convert markdown to HTML format with rendered diagrams.
    ///
    /// Produces semantic HTML5 with diagram code blocks rendered as images via Kroki.
    /// Diagrams are rendered based on their format attribute:
    /// - `svg` (default): Inline SVG (supports links and interactivity)
    /// - `png`: Inline PNG as base64 data URI
    ///
    /// If diagram rendering fails, the diagram is replaced with an error message
    /// wrapped in `<figure class="diagram diagram-error">`. This allows the page
    /// to still render even when Kroki is unavailable or returns an error.
    ///
    /// # Arguments
    ///
    /// * `markdown_text` - Markdown source text
    /// * `kroki_url` - Kroki server URL (e.g., `"https://kroki.io"`)
    /// * `base_path` - Optional base path for resolving relative links
    #[must_use]
    pub fn convert_html_with_diagrams(
        &self,
        markdown_text: &str,
        kroki_url: &str,
        base_path: Option<&str>,
    ) -> HtmlConvertResult {
        self.render_html_with_diagrams(markdown_text, kroki_url, None, base_path)
    }

    /// Convert markdown to HTML format with cached diagram rendering.
    ///
    /// Like [`convert_html_with_diagrams`](Self::convert_html_with_diagrams), but uses
    /// a file-based cache to avoid re-rendering diagrams with the same content.
    ///
    /// The cache key is computed from:
    /// - Diagram source (after preprocessing)
    /// - Kroki endpoint
    /// - Output format (svg/png)
    /// - DPI setting
    ///
    /// Cache files are stored as `{cache_dir}/{hash}.{format}` (e.g., `abc123.svg`).
    ///
    /// # Arguments
    ///
    /// * `markdown_text` - Markdown source text
    /// * `kroki_url` - Kroki server URL
    /// * `cache_dir` - Directory for cached diagrams (caching disabled if None)
    /// * `base_path` - Optional base path for resolving relative links
    #[must_use]
    pub fn convert_html_with_diagrams_cached(
        &self,
        markdown_text: &str,
        kroki_url: &str,
        cache_dir: Option<&Path>,
        base_path: Option<&str>,
    ) -> HtmlConvertResult {
        let cache: Arc<dyn DiagramCache> = match cache_dir {
            Some(dir) => Arc::new(FileCache::new(dir.to_path_buf())),
            None => Arc::new(NullCache),
        };
        self.render_html_with_diagrams(markdown_text, kroki_url, Some(cache), base_path)
    }

    /// Internal helper for HTML rendering with optional diagram caching.
    fn render_html_with_diagrams(
        &self,
        markdown_text: &str,
        kroki_url: &str,
        cache: Option<Arc<dyn DiagramCache>>,
        base_path: Option<&str>,
    ) -> HtmlConvertResult {
        let options = self.get_parser_options();
        let parser = Parser::new_ext(markdown_text, options);

        let mut processor = DiagramProcessor::new()
            .kroki_url(kroki_url)
            .include_dirs(&self.include_dirs)
            .config_file(self.config_file.as_deref());

        if let Some(dpi) = self.dpi {
            processor = processor.dpi(dpi);
        }
        if let Some(c) = cache {
            processor = processor.with_cache(c);
        }

        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        if let Some(path) = base_path {
            renderer = renderer.with_base_path(path);
        }
        renderer = renderer.with_processor(processor);

        let result = renderer.render(parser);
        let html = renderer.finalize(result.html);
        let warnings = renderer.processor_warnings();

        HtmlConvertResult {
            html,
            title: result.title,
            toc: result.toc,
            warnings,
        }
    }
}
