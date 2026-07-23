//! Markdown to Confluence page renderer.
//!
//! This module provides [`PageRenderer`] for converting `CommonMark` documents
//! to Confluence XHTML storage format.
//!
//! # Features
//!
//! - GitHub Flavored Markdown support (tables, strikethrough, task lists)
//! - Title extraction from first H1 heading
//! - Table of contents macro prepending
//! - Diagram rendering via Kroki service
//! - Configurable DPI for diagram output
//!
//! # Usage
//!
//! Create a `PageRenderer` with builder methods (`prepend_toc`, `extract_title`),
//! then call `render(markdown, kroki_url, diagram_dir)` to produce Confluence XHTML.

use rw_kroki::{DiagramOutput, DiagramProcessor};
use rw_renderer::directive::DirectiveProcessor;
use rw_renderer::{MarkdownRenderer, Pipeline, RenderResult, TocEntry};
use std::path::{Path, PathBuf};

use crate::backend::ConfluenceBackend;
use crate::tags::confluence_tag_generator;

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Renders markdown to Confluence XHTML storage format.
///
/// Note: This is distinct from `rw_site::PageRenderer` which renders
/// markdown to HTML for the web server. Both are "page renderers" but for
/// different output formats.
#[derive(Debug)]
pub(crate) struct PageRenderer {
    prepend_toc: bool,
    extract_title: bool,
    include_dirs: Vec<PathBuf>,
}

impl Default for PageRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl PageRenderer {
    /// Create a new renderer with default settings.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            prepend_toc: false,
            extract_title: false,
            include_dirs: Vec::new(),
        }
    }

    /// Enable or disable prepending a table of contents macro.
    #[must_use]
    pub(crate) fn prepend_toc(mut self, enabled: bool) -> Self {
        self.prepend_toc = enabled;
        self
    }

    /// Enable or disable extracting the first H1 as page title.
    #[must_use]
    pub(crate) fn extract_title(mut self, enabled: bool) -> Self {
        self.extract_title = enabled;
        self
    }

    /// Set directories to search for `PlantUML` includes.
    #[must_use]
    pub(crate) fn include_dirs(
        mut self,
        dirs: impl IntoIterator<Item = impl Into<PathBuf>>,
    ) -> Self {
        self.include_dirs = dirs.into_iter().map(Into::into).collect();
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
    pub(crate) fn render(
        &self,
        markdown_text: &str,
        kroki_url: Option<&str>,
        output_dir: Option<&Path>,
    ) -> RenderResult {
        let renderer = self.create_renderer();
        let pipeline = self.create_pipeline(kroki_url, output_dir);

        let result = renderer.render(markdown_text, pipeline);

        RenderResult {
            html: self.maybe_prepend_toc(result.html, &result.toc),
            title: result.title,
            toc: result.toc,
            warnings: result.warnings,
            has_transient_error: result.has_transient_error,
            section_refs: result.section_refs,
        }
    }

    /// Create a diagram processor with common configuration.
    fn create_diagram_processor(&self, kroki_url: &str) -> DiagramProcessor {
        DiagramProcessor::new(kroki_url).include_dirs(&self.include_dirs)
    }

    /// Build the settings-only renderer.
    fn create_renderer(&self) -> MarkdownRenderer<ConfluenceBackend> {
        let mut renderer = MarkdownRenderer::<ConfluenceBackend>::new();
        if self.extract_title {
            renderer = renderer.with_title_extraction();
        }
        renderer
    }

    /// Build the per-render pipeline.
    fn create_pipeline(
        &self,
        kroki_url: Option<&str>,
        output_dir: Option<&std::path::Path>,
    ) -> Pipeline {
        // Register an (empty) processor so directive syntax is tokenized: the
        // built-in `:status` badge needs tokenization on, and the renderer gates
        // that on a processor being present. No inline/leaf/container handlers are
        // needed — status is handled by the walker, not a registered directive.
        let mut pipeline = Pipeline::new().with_directives(DirectiveProcessor::new());
        if let (Some(url), Some(dir)) = (kroki_url, output_dir) {
            let processor = self
                .create_diagram_processor(url)
                .output(DiagramOutput::Files {
                    output_dir: dir.to_path_buf(),
                    tag_generator: confluence_tag_generator(),
                });
            pipeline = pipeline.with_processor(processor);
        }
        pipeline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_directive_renders_confluence_macro() {
        let renderer = PageRenderer::new();
        let result = renderer.render(":status[On Track]{color=green}", None, None);
        assert!(
            result.html.contains(r#"ac:name="status""#),
            "got: {}",
            result.html
        );
        assert!(
            result
                .html
                .contains(r#"<ac:parameter ac:name="colour">Green</ac:parameter>"#),
            "got: {}",
            result.html
        );
        assert!(
            result
                .html
                .contains(r#"<ac:parameter ac:name="title">On Track</ac:parameter>"#),
            "got: {}",
            result.html
        );
    }

    #[test]
    fn test_status_directive_unknown_color_is_grey() {
        let renderer = PageRenderer::new();
        let result = renderer.render(":status[X]{color=mauve}", None, None);
        assert!(
            result
                .html
                .contains(r#"<ac:parameter ac:name="colour">Grey</ac:parameter>"#),
            "got: {}",
            result.html
        );
    }
}
