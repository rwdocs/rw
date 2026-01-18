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

use pulldown_cmark::{Options, Parser};

use docstage_confluence_renderer::ConfluenceBackend;
use docstage_renderer::{HtmlBackend, MarkdownRenderer, TocEntry};

use docstage_diagrams::{
    DEFAULT_DPI, DiagramProcessor, DiagramRequest, ExtractedDiagram, RenderError, load_config_file,
    prepare_diagram_source, render_all, to_extracted_diagrams,
};

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Create Confluence image macro for an attachment.
#[must_use]
pub fn create_image_tag(filename: &str, width: u32) -> String {
    format!(r#"<ac:image ac:width="{width}"><ri:attachment ri:filename="{filename}" /></ac:image>"#)
}

/// Information about a rendered diagram.
#[derive(Clone, Debug)]
pub struct DiagramInfo {
    pub filename: String,
    pub width: u32,
    pub height: u32,
}

/// Result of converting markdown to Confluence format.
#[derive(Clone, Debug)]
pub struct ConvertResult {
    pub html: String,
    pub title: Option<String>,
    pub diagrams: Vec<DiagramInfo>,
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

/// A diagram extracted from markdown with its prepared source.
#[derive(Clone, Debug)]
pub struct PreparedDiagram {
    /// Zero-based index of this diagram in the document.
    pub index: usize,
    /// Prepared source ready for Kroki (with !include resolved, config injected).
    pub source: String,
    /// Kroki endpoint for this diagram type.
    pub endpoint: String,
    /// Output format (svg, png).
    pub format: String,
}

/// Result of extracting diagrams from markdown.
///
/// Used by both HTML and Confluence output formats. Contains placeholders
/// (`{{DIAGRAM_0}}`, `{{DIAGRAM_1}}`, etc.) that should be replaced with
/// rendered diagram content.
#[derive(Clone, Debug)]
pub struct ExtractResult {
    /// HTML/XHTML with diagram placeholders (`{{DIAGRAM_0}}`, `{{DIAGRAM_1}}`, etc.).
    pub html: String,
    /// Title extracted from first H1 heading (if `extract_title` was enabled).
    pub title: Option<String>,
    /// Table of contents entries.
    pub toc: Vec<TocEntry>,
    /// Prepared diagrams ready for rendering.
    pub diagrams: Vec<PreparedDiagram>,
    /// Warnings generated during conversion.
    pub warnings: Vec<String>,
}

/// Markdown to Confluence converter configuration.
#[derive(Clone, Debug)]
pub struct MarkdownConverter {
    gfm: bool,
    prepend_toc: bool,
    extract_title: bool,
    include_dirs: Vec<PathBuf>,
    config_content: Option<String>,
    /// DPI for `PlantUML` diagram rendering.
    dpi: u32,
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
            config_content: None,
            dpi: DEFAULT_DPI,
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

    /// Load `PlantUML` config from a file.
    #[must_use]
    pub fn config_file(mut self, config_file: Option<&str>) -> Self {
        self.config_content = config_file.and_then(|cf| load_config_file(&self.include_dirs, cf));
        self
    }

    /// Set DPI for `PlantUML` diagram rendering.
    ///
    /// Default is 192 (2x for retina displays). Set to 96 for standard resolution.
    #[must_use]
    pub fn dpi(mut self, dpi: u32) -> Self {
        self.dpi = dpi;
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

    /// Create an HTML renderer with the converter's settings.
    fn create_html_renderer(
        &self,
        base_path: Option<&str>,
        with_diagrams: bool,
    ) -> MarkdownRenderer<HtmlBackend> {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        if let Some(path) = base_path {
            renderer = renderer.with_base_path(path);
        }
        if with_diagrams {
            renderer = renderer.with_processor(DiagramProcessor::new());
        }
        renderer
    }

    /// Create a Confluence renderer with the converter's settings.
    fn create_confluence_renderer(
        &self,
        with_diagrams: bool,
    ) -> MarkdownRenderer<ConfluenceBackend> {
        let mut renderer = MarkdownRenderer::<ConfluenceBackend>::new();
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        if with_diagrams {
            renderer = renderer.with_processor(DiagramProcessor::new());
        }
        renderer
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

    /// Prepare diagram source and collect warnings.
    fn prepare_diagram_source_with_warnings(
        &self,
        diagram: &ExtractedDiagram,
        warnings: &mut Vec<String>,
    ) -> String {
        if diagram.language.needs_plantuml_preprocessing() {
            let prepare_result = prepare_diagram_source(
                &diagram.source,
                &self.include_dirs,
                self.config_content.as_deref(),
                self.dpi,
            );
            warnings.extend(prepare_result.warnings);
            prepare_result.source
        } else {
            diagram.source.clone()
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

        let mut renderer = self.create_confluence_renderer(true);
        let result = renderer.render(parser);

        let extracted_diagrams = to_extracted_diagrams(&renderer.extracted_code_blocks());
        let mut warnings = renderer.processor_warnings();

        let mut html = self.maybe_prepend_toc(result.html, &result.toc);

        // Render diagrams if any
        let diagrams = if extracted_diagrams.is_empty() {
            Vec::new()
        } else {
            // Prepare diagram sources (PlantUML needs preprocessing, others pass through)
            let diagram_requests: Vec<_> = extracted_diagrams
                .iter()
                .map(|d| {
                    let source = self.prepare_diagram_source_with_warnings(d, &mut warnings);
                    DiagramRequest::new(d.index, source, d.language)
                })
                .collect();

            let server_url = kroki_url.trim_end_matches('/');
            let rendered_diagrams = render_all(&diagram_requests, server_url, output_dir, 4)?;

            // Replace placeholders with image tags
            let mut diagram_infos = Vec::with_capacity(rendered_diagrams.len());
            for r in rendered_diagrams {
                // Display width is half the actual width (for retina displays)
                let display_width = r.width / 2;
                let image_tag = create_image_tag(&r.filename, display_width);
                let placeholder = format!("{{{{DIAGRAM_{}}}}}", r.index);
                html = html.replace(&placeholder, &image_tag);

                diagram_infos.push(DiagramInfo {
                    filename: r.filename,
                    width: r.width,
                    height: r.height,
                });
            }
            diagram_infos
        };

        Ok(ConvertResult {
            html,
            title: result.title,
            diagrams,
            warnings,
        })
    }

    /// Extract diagrams from markdown and return Confluence XHTML with placeholders.
    ///
    /// This method is used for diagram caching. It returns:
    /// - Confluence XHTML with `{{DIAGRAM_N}}` placeholders
    /// - Prepared diagrams with source ready for Kroki
    ///
    /// Supports all diagram types: `PlantUML`, Mermaid, `GraphViz`, and 14+
    /// other Kroki-supported formats.
    ///
    /// # Differences from [`Self::extract_html_with_diagrams`]
    ///
    /// | Aspect | Confluence | HTML |
    /// |--------|------------|------|
    /// | Renderer | Confluence XHTML | Semantic HTML5 |
    /// | Diagram format | Always PNG (attachments) | SVG or PNG (inline) |
    /// | `ToC` | Generates Confluence macro | For client-side rendering |
    /// | Link resolution | Not supported | Supports `base_path` |
    ///
    /// The caller is responsible for:
    /// 1. Checking the cache for each diagram by content hash
    /// 2. Rendering uncached diagrams via Kroki
    /// 3. Replacing placeholders with rendered content (e.g., image macros)
    #[must_use]
    pub fn extract_confluence_with_diagrams(&self, markdown_text: &str) -> ExtractResult {
        let options = self.get_parser_options();
        let parser = Parser::new_ext(markdown_text, options);

        let mut renderer = self.create_confluence_renderer(true);
        let result = renderer.render(parser);

        let extracted_diagrams = to_extracted_diagrams(&renderer.extracted_code_blocks());
        let mut warnings = renderer.processor_warnings();

        let diagrams: Vec<_> = extracted_diagrams
            .iter()
            .map(|d| {
                let source = self.prepare_diagram_source_with_warnings(d, &mut warnings);
                PreparedDiagram {
                    index: d.index,
                    source,
                    endpoint: d.language.kroki_endpoint().to_string(),
                    // Confluence always uses PNG format for attachments
                    format: "png".to_string(),
                }
            })
            .collect();

        let html = self.maybe_prepend_toc(result.html, &result.toc);

        ExtractResult {
            html,
            title: result.title,
            toc: result.toc,
            diagrams,
            warnings,
        }
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
        let result = self.create_html_renderer(base_path, false).render(parser);

        HtmlConvertResult {
            html: result.html,
            title: result.title,
            toc: result.toc,
            warnings: Vec::new(),
        }
    }

    /// Extract diagrams from markdown and return HTML with placeholders.
    ///
    /// This method is used for diagram caching. It returns:
    /// - HTML with `{{DIAGRAM_N}}` placeholders
    /// - Prepared diagrams with source ready for Kroki
    ///
    /// The caller is responsible for:
    /// 1. Checking the cache for each diagram by content hash
    /// 2. Rendering uncached diagrams via Kroki
    /// 3. Replacing placeholders with rendered content
    ///
    /// # Arguments
    ///
    /// * `markdown_text` - Markdown source text
    /// * `base_path` - Optional base path for resolving relative links
    #[must_use]
    pub fn extract_html_with_diagrams(
        &self,
        markdown_text: &str,
        base_path: Option<&str>,
    ) -> ExtractResult {
        let options = self.get_parser_options();
        let parser = Parser::new_ext(markdown_text, options);

        let mut renderer = self.create_html_renderer(base_path, true);
        let result = renderer.render(parser);

        let extracted_diagrams = to_extracted_diagrams(&renderer.extracted_code_blocks());
        let mut warnings = renderer.processor_warnings();

        let diagrams: Vec<_> = extracted_diagrams
            .iter()
            .map(|d| {
                let source = self.prepare_diagram_source_with_warnings(d, &mut warnings);
                PreparedDiagram {
                    index: d.index,
                    source,
                    endpoint: d.language.kroki_endpoint().to_string(),
                    format: d.format.as_str().to_string(),
                }
            })
            .collect();

        ExtractResult {
            html: result.html,
            title: result.title,
            toc: result.toc,
            diagrams,
            warnings,
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
        let options = self.get_parser_options();
        let parser = Parser::new_ext(markdown_text, options);

        let processor = DiagramProcessor::new()
            .kroki_url(kroki_url)
            .include_dirs(&self.include_dirs)
            .config_content(self.config_content.as_deref())
            .dpi(self.dpi);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_confluence_with_plantuml() {
        let markdown = "# Title\n\n```plantuml\n@startuml\nA -> B\n@enduml\n```";
        let converter = MarkdownConverter::new().extract_title(true);
        let result = converter.extract_confluence_with_diagrams(markdown);

        assert_eq!(result.title, Some("Title".to_string()));
        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert_eq!(result.diagrams.len(), 1);
        assert_eq!(result.diagrams[0].index, 0);
        assert_eq!(result.diagrams[0].endpoint, "plantuml");
        assert_eq!(result.diagrams[0].format, "png");
        assert!(result.diagrams[0].source.contains("A -> B"));
    }

    #[test]
    fn test_extract_confluence_with_mermaid() {
        let markdown = "```mermaid\ngraph TD\n  A --> B\n```";
        let converter = MarkdownConverter::new();
        let result = converter.extract_confluence_with_diagrams(markdown);

        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert_eq!(result.diagrams.len(), 1);
        assert_eq!(result.diagrams[0].endpoint, "mermaid");
        assert_eq!(result.diagrams[0].format, "png");
        assert!(result.diagrams[0].source.contains("graph TD"));
    }

    #[test]
    fn test_extract_confluence_with_graphviz() {
        let markdown = "```graphviz\ndigraph G { A -> B }\n```";
        let converter = MarkdownConverter::new();
        let result = converter.extract_confluence_with_diagrams(markdown);

        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert_eq!(result.diagrams.len(), 1);
        assert_eq!(result.diagrams[0].endpoint, "graphviz");
        assert_eq!(result.diagrams[0].format, "png");
    }

    #[test]
    fn test_extract_confluence_with_multiple_diagram_types() {
        let markdown = r"
```plantuml
@startuml
A -> B
@enduml
```

Text

```mermaid
graph TD
  C --> D
```

More text

```ditaa
+---+
| A |
+---+
```
";
        let converter = MarkdownConverter::new();
        let result = converter.extract_confluence_with_diagrams(markdown);

        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert!(result.html.contains("{{DIAGRAM_1}}"));
        assert!(result.html.contains("{{DIAGRAM_2}}"));
        assert_eq!(result.diagrams.len(), 3);
        assert_eq!(result.diagrams[0].endpoint, "plantuml");
        assert_eq!(result.diagrams[1].endpoint, "mermaid");
        assert_eq!(result.diagrams[2].endpoint, "ditaa");
        // All diagrams use PNG for Confluence
        assert!(result.diagrams.iter().all(|d| d.format == "png"));
    }

    #[test]
    fn test_extract_confluence_no_diagrams() {
        let markdown = "# Title\n\nNo diagrams here.";
        let converter = MarkdownConverter::new();
        let result = converter.extract_confluence_with_diagrams(markdown);

        assert!(result.diagrams.is_empty());
        assert!(result.html.contains("No diagrams here"));
    }

    #[test]
    fn test_extract_confluence_with_kroki_prefix() {
        // Test kroki- prefixed language names (MkDocs compatibility)
        let markdown = "```kroki-mermaid\ngraph TD\n  A --> B\n```";
        let converter = MarkdownConverter::new();
        let result = converter.extract_confluence_with_diagrams(markdown);

        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert_eq!(result.diagrams.len(), 1);
        assert_eq!(result.diagrams[0].endpoint, "mermaid");
    }
}
