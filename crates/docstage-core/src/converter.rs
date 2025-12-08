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

use crate::confluence::ConfluenceRenderer;
use crate::diagram_filter::{DiagramFilter, DiagramFormat};
use crate::html::{HtmlRenderer, TocEntry, escape_html};
use crate::kroki::{
    DiagramError, DiagramRequest, RenderError, render_all, render_all_png_data_uri_partial,
    render_all_svg_partial,
};
use crate::plantuml::{DEFAULT_DPI, load_config_file, prepare_diagram_source};
use crate::plantuml_filter::PlantUmlFilter;

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

    /// Convert markdown to Confluence storage format.
    ///
    /// `PlantUML` diagrams are rendered via Kroki and placeholders replaced with
    /// Confluence image macros.
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

        // Filter plantuml code blocks, replacing them with placeholders
        let mut filter = PlantUmlFilter::new(parser);

        // Render to Confluence format
        let renderer = if self.extract_title {
            ConfluenceRenderer::new().with_title_extraction()
        } else {
            ConfluenceRenderer::new()
        };

        let result = renderer.render_with_title(&mut filter);

        // Get extracted diagrams
        let extracted_diagrams = filter.into_diagrams();

        let mut html = if self.prepend_toc {
            format!("{}{}", TOC_MACRO, result.html)
        } else {
            result.html
        };

        // Render diagrams if any
        let diagrams = if extracted_diagrams.is_empty() {
            Vec::new()
        } else {
            // Resolve diagram sources (PlantUML filter only extracts PlantUML)
            // Note: warnings are ignored for Confluence output (used for legacy support)
            let diagram_requests: Vec<_> = extracted_diagrams
                .into_iter()
                .map(|d| {
                    let prepare_result = prepare_diagram_source(
                        &d.source,
                        &self.include_dirs,
                        self.config_content.as_deref(),
                        self.dpi,
                    );
                    DiagramRequest::plantuml(d.index, prepare_result.source)
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
        })
    }

    /// Convert markdown to HTML format.
    ///
    /// Produces semantic HTML5 with syntax highlighting and table of contents.
    /// Diagram code blocks are rendered with syntax highlighting as-is.
    /// For rendered diagram images, use `convert_html_with_diagrams()`.
    #[must_use]
    pub fn convert_html(&self, markdown_text: &str) -> HtmlConvertResult {
        let options = self.get_parser_options();
        let parser = Parser::new_ext(markdown_text, options);

        let renderer = if self.extract_title {
            HtmlRenderer::new().with_title_extraction()
        } else {
            HtmlRenderer::new()
        };

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
    /// - `img`: External SVG via `<img>` tag (not yet implemented, falls back to inline SVG)
    ///
    /// If diagram rendering fails, the diagram is replaced with an error message
    /// wrapped in `<figure class="diagram diagram-error">`. This allows the page
    /// to still render even when Kroki is unavailable or returns an error.
    ///
    /// # Arguments
    ///
    /// * `markdown_text` - Markdown source text
    /// * `kroki_url` - Kroki server URL (e.g., `"https://kroki.io"`)
    #[must_use]
    pub fn convert_html_with_diagrams(
        &self,
        markdown_text: &str,
        kroki_url: &str,
    ) -> HtmlConvertResult {
        let options = self.get_parser_options();
        let parser = Parser::new_ext(markdown_text, options);

        // Filter diagram code blocks, replacing them with placeholders
        let mut filter = DiagramFilter::new(parser);

        // Render to HTML format
        let renderer = if self.extract_title {
            HtmlRenderer::new().with_title_extraction()
        } else {
            HtmlRenderer::new()
        };

        let result = renderer.render(&mut filter);
        let (extracted_diagrams, filter_warnings) = filter.into_parts();

        let mut html = result.html;
        let mut warnings = filter_warnings;

        if !extracted_diagrams.is_empty() {
            // Group diagrams by format
            let mut svg_diagrams = Vec::new();
            let mut png_diagrams = Vec::new();

            for d in &extracted_diagrams {
                let source = if d.language.needs_plantuml_preprocessing() {
                    let prepare_result = prepare_diagram_source(
                        &d.source,
                        &self.include_dirs,
                        self.config_content.as_deref(),
                        self.dpi,
                    );
                    warnings.extend(prepare_result.warnings);
                    prepare_result.source
                } else {
                    d.source.clone()
                };

                let request = DiagramRequest::new(d.index, source, d.language);

                match d.format {
                    DiagramFormat::Svg => svg_diagrams.push((d.index, request)),
                    DiagramFormat::Img => {
                        warnings.push(format!(
                            "diagram {}: format=img is not yet implemented, falling back to inline SVG",
                            d.index
                        ));
                        svg_diagrams.push((d.index, request));
                    }
                    DiagramFormat::Png => png_diagrams.push((d.index, request)),
                }
            }

            replace_svg_diagrams(&mut html, &svg_diagrams, kroki_url);
            replace_png_diagrams(&mut html, &png_diagrams, kroki_url);
        }

        HtmlConvertResult {
            html,
            title: result.title,
            toc: result.toc,
            warnings,
        }
    }
}

/// Replace diagram placeholders with rendered SVG content.
///
/// Attempts to render all diagrams via Kroki. On success, replaces placeholders
/// with `<figure class="diagram">` containing the SVG. On failure, replaces
/// with an error message in `<figure class="diagram diagram-error">`.
fn replace_svg_diagrams(html: &mut String, diagrams: &[(usize, DiagramRequest)], kroki_url: &str) {
    if diagrams.is_empty() {
        return;
    }

    let requests: Vec<_> = diagrams.iter().map(|(_, r)| r.clone()).collect();
    match render_all_svg_partial(&requests, kroki_url, 4) {
        Ok(result) => {
            for r in result.rendered {
                replace_placeholder_with_svg(html, r.index, r.svg.trim());
            }
            for e in result.errors {
                replace_placeholder_with_error(html, e);
            }
        }
        Err(e) => {
            let error_msg = e.to_string();
            for (idx, _) in diagrams {
                replace_placeholder_with_error_msg(html, *idx, &error_msg);
            }
        }
    }
}

/// Replace diagram placeholders with rendered PNG content as data URIs.
///
/// Attempts to render all diagrams via Kroki. On success, replaces placeholders
/// with `<figure class="diagram">` containing an `<img>` tag with base64 data URI.
/// On failure, replaces with an error message in `<figure class="diagram diagram-error">`.
fn replace_png_diagrams(html: &mut String, diagrams: &[(usize, DiagramRequest)], kroki_url: &str) {
    if diagrams.is_empty() {
        return;
    }

    let requests: Vec<_> = diagrams.iter().map(|(_, r)| r.clone()).collect();
    match render_all_png_data_uri_partial(&requests, kroki_url, 4) {
        Ok(result) => {
            for r in result.rendered {
                replace_placeholder_with_png(html, r.index, &r.data_uri);
            }
            for e in result.errors {
                replace_placeholder_with_error(html, e);
            }
        }
        Err(e) => {
            let error_msg = e.to_string();
            for (idx, _) in diagrams {
                replace_placeholder_with_error_msg(html, *idx, &error_msg);
            }
        }
    }
}

fn replace_placeholder_with_svg(html: &mut String, index: usize, svg: &str) {
    let placeholder = format!("{{{{DIAGRAM_{}}}}}", index);
    let figure = format!(r#"<figure class="diagram">{}</figure>"#, svg);
    *html = html.replace(&placeholder, &figure);
}

fn replace_placeholder_with_png(html: &mut String, index: usize, data_uri: &str) {
    let placeholder = format!("{{{{DIAGRAM_{}}}}}", index);
    let figure = format!(
        r#"<figure class="diagram"><img src="{}" alt="diagram"></figure>"#,
        data_uri
    );
    *html = html.replace(&placeholder, &figure);
}

fn replace_placeholder_with_error(html: &mut String, error: DiagramError) {
    replace_placeholder_with_error_msg(html, error.index, &error.to_string());
}

fn replace_placeholder_with_error_msg(html: &mut String, index: usize, error_msg: &str) {
    let placeholder = format!("{{{{DIAGRAM_{}}}}}", index);
    let error_figure = format!(
        r#"<figure class="diagram diagram-error"><pre>Diagram rendering failed: {}</pre></figure>"#,
        escape_html(error_msg)
    );
    *html = html.replace(&placeholder, &error_figure);
}
