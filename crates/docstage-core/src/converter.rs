//! Markdown to Confluence converter.

use std::path::{Path, PathBuf};

use pulldown_cmark::{Options, Parser};

use crate::confluence::ConfluenceRenderer;
use crate::kroki::{DiagramRequest, RenderError, render_all};
use crate::plantuml::{load_config_file, prepare_diagram_source};
use crate::plantuml_filter::PlantUmlFilter;

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Create Confluence image macro for an attachment.
pub fn create_image_tag(filename: &str, width: u32) -> String {
    format!(
        r#"<ac:image ac:width="{}"><ri:attachment ri:filename="{}" /></ac:image>"#,
        width, filename
    )
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

/// Markdown to Confluence converter configuration.
#[derive(Clone, Debug)]
pub struct MarkdownConverter {
    gfm: bool,
    prepend_toc: bool,
    extract_title: bool,
    include_dirs: Vec<PathBuf>,
    config_content: Option<String>,
}

impl Default for MarkdownConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownConverter {
    /// Create a new converter with default settings.
    pub fn new() -> Self {
        Self {
            gfm: true,
            prepend_toc: false,
            extract_title: false,
            include_dirs: Vec::new(),
            config_content: None,
        }
    }

    /// Enable or disable GitHub Flavored Markdown features.
    pub fn gfm(mut self, enabled: bool) -> Self {
        self.gfm = enabled;
        self
    }

    /// Enable or disable prepending a table of contents macro.
    pub fn prepend_toc(mut self, enabled: bool) -> Self {
        self.prepend_toc = enabled;
        self
    }

    /// Enable or disable extracting the first H1 as page title.
    pub fn extract_title(mut self, enabled: bool) -> Self {
        self.extract_title = enabled;
        self
    }

    /// Set directories to search for PlantUML includes.
    pub fn include_dirs(mut self, dirs: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        self.include_dirs = dirs.into_iter().map(Into::into).collect();
        self
    }

    /// Load PlantUML config from a file.
    pub fn config_file(mut self, config_file: Option<&str>) -> Self {
        self.config_content = config_file.and_then(|cf| load_config_file(&self.include_dirs, cf));
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
    /// PlantUML diagrams are rendered via Kroki and placeholders replaced with
    /// Confluence image macros.
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
            // Resolve diagram sources
            let diagram_requests: Vec<_> = extracted_diagrams
                .into_iter()
                .map(|d| {
                    let resolved_source = prepare_diagram_source(
                        &d.source,
                        &self.include_dirs,
                        self.config_content.as_deref(),
                    );
                    DiagramRequest {
                        index: d.index,
                        source: resolved_source,
                    }
                })
                .collect();

            let server_url = kroki_url.trim_end_matches('/');
            let rendered = render_all(diagram_requests, server_url, output_dir, 4)?;

            // Replace placeholders with image tags
            let mut diagram_infos = Vec::with_capacity(rendered.len());
            for r in rendered {
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
}
