//! Markdown to Confluence page renderer.
//!
//! This module provides [`PageRenderer`] for converting `CommonMark` documents
//! to Confluence XHTML storage format.
//!
//! Note: This is distinct from [`docstage_site::PageRenderer`] which renders
//! markdown to HTML for the web server. Both are "page renderers" but for
//! different output formats.
//!
//! # Features
//!
//! - GitHub Flavored Markdown support (tables, strikethrough, task lists)
//! - Title extraction from first H1 heading
//! - Table of contents macro prepending
//! - Diagram rendering via Kroki service
//! - Configurable DPI for diagram output
//!
//! # Example
//!
//! ```ignore
//! use std::path::Path;
//! use docstage_confluence::PageRenderer;
//!
//! let renderer = PageRenderer::new()
//!     .prepend_toc(true)
//!     .extract_title(true)
//!     .dpi(192);
//!
//! let result = renderer.render(
//!     "# Hello\n\n```plantuml\nA -> B\n```",
//!     Some("https://kroki.io"),
//!     Some(Path::new("/tmp/diagrams")),
//! );
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use docstage_diagrams::{DiagramOutput, DiagramProcessor};
use docstage_renderer::{MarkdownRenderer, RenderResult, TocEntry};

use crate::backend::ConfluenceBackend;
use crate::tags::ConfluenceTagGenerator;

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Renders markdown to Confluence XHTML storage format.
///
/// Note: This is distinct from `docstage_site::PageRenderer` which renders
/// markdown to HTML for the web server. Both are "page renderers" but for
/// different output formats.
#[derive(Clone, Debug)]
pub struct PageRenderer {
    gfm: bool,
    prepend_toc: bool,
    extract_title: bool,
    include_dirs: Vec<PathBuf>,
    config_file: Option<String>,
    dpi: Option<u32>,
}

impl Default for PageRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl PageRenderer {
    /// Create a new renderer with default settings.
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

    /// Render markdown to Confluence storage format with optional diagram rendering via Kroki.
    ///
    /// When `kroki_url` and `output_dir` are provided, diagrams are rendered via the Kroki
    /// service and written to the output directory. When `None`, diagram blocks are rendered
    /// as syntax-highlighted code.
    #[must_use]
    pub fn render(
        &self,
        markdown_text: &str,
        kroki_url: Option<&str>,
        output_dir: Option<&Path>,
    ) -> RenderResult {
        let mut renderer = self.create_renderer();

        if let (Some(url), Some(dir)) = (kroki_url, output_dir) {
            let processor = self
                .create_diagram_processor(url)
                .output(DiagramOutput::Files {
                    output_dir: dir.to_path_buf(),
                    tag_generator: Arc::new(ConfluenceTagGenerator),
                });
            renderer = renderer.with_processor(processor);
        }

        let result = renderer.render_markdown(markdown_text);

        RenderResult {
            html: self.maybe_prepend_toc(result.html, &result.toc),
            title: result.title,
            toc: result.toc,
            warnings: result.warnings,
        }
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

    /// Create a Confluence renderer with common configuration.
    fn create_renderer(&self) -> MarkdownRenderer<ConfluenceBackend> {
        let mut renderer = MarkdownRenderer::<ConfluenceBackend>::new().with_gfm(self.gfm);
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        renderer
    }
}
