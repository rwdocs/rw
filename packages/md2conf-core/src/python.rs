//! Python bindings via PyO3.

use std::path::PathBuf;

use pulldown_cmark::{Options, Parser};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::confluence::ConfluenceRenderer;
use crate::kroki;
use crate::plantuml;
use crate::plantuml_filter::PlantUmlFilter;

/// Rendered diagram info (file written to output_dir).
#[pyclass(name = "RenderedDiagram")]
#[derive(Clone)]
pub struct PyRenderedDiagram {
    #[pyo3(get)]
    pub filename: String,
    #[pyo3(get)]
    pub width: u32,
    #[pyo3(get)]
    pub height: u32,
}

/// Result of converting markdown to Confluence format.
#[pyclass(name = "ConvertResult")]
pub struct PyConvertResult {
    #[pyo3(get)]
    pub html: String,
    #[pyo3(get)]
    pub title: Option<String>,
    #[pyo3(get)]
    pub diagrams: Vec<PyRenderedDiagram>,
}

fn get_parser_options(gfm: bool) -> Options {
    let mut options = Options::empty();
    if gfm {
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
    }
    options
}

const TOC_MACRO: &str = r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#;

/// Create Confluence image macro for an attachment.
fn create_image_tag(filename: &str, width: u32) -> String {
    format!(
        r#"<ac:image ac:width="{}"><ri:attachment ri:filename="{}" /></ac:image>"#,
        width, filename
    )
}

/// Markdown to Confluence converter.
#[pyclass(name = "MarkdownConverter")]
pub struct PyMarkdownConverter {
    gfm: bool,
    prepend_toc: bool,
    extract_title: bool,
    include_dirs: Vec<String>,
    config_content: Option<String>,
}

#[pymethods]
impl PyMarkdownConverter {
    #[new]
    #[pyo3(signature = (gfm = true, prepend_toc = false, extract_title = false, include_dirs = None, config_file = None))]
    pub fn new(
        gfm: bool,
        prepend_toc: bool,
        extract_title: bool,
        include_dirs: Option<Vec<PathBuf>>,
        config_file: Option<&str>,
    ) -> Self {
        let dirs: Vec<String> = include_dirs
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        let config_content = config_file.and_then(|cf| plantuml::load_config_file(&dirs, cf));

        Self {
            gfm,
            prepend_toc,
            extract_title,
            include_dirs: dirs,
            config_content,
        }
    }

    /// Convert markdown to Confluence storage format.
    ///
    /// PlantUML diagrams are rendered via Kroki and placeholders replaced with
    /// Confluence image macros.
    ///
    /// Args:
    ///     markdown_text: Markdown source text
    ///     kroki_url: Kroki server URL (e.g., "https://kroki.io")
    ///     output_dir: Directory to write rendered PNG files
    ///
    /// Returns:
    ///     ConvertResult with HTML, optional title, and rendered diagrams
    pub fn convert(
        &self,
        py: Python<'_>,
        markdown_text: &str,
        kroki_url: &str,
        output_dir: PathBuf,
    ) -> PyResult<PyConvertResult> {
        let options = get_parser_options(self.gfm);
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
            let diagram_infos: Vec<_> = extracted_diagrams
                .into_iter()
                .map(|d| {
                    let resolved_source = plantuml::prepare_diagram_source(
                        &d.source,
                        &self.include_dirs,
                        self.config_content.as_deref(),
                    );
                    kroki::DiagramRequest {
                        index: d.index,
                        source: resolved_source,
                    }
                })
                .collect();

            let server_url = kroki_url.trim_end_matches('/').to_string();

            let rendered = py.detach(|| {
                kroki::render_all(diagram_infos, &server_url, &output_dir, 4)
                    .map_err(|e| PyRuntimeError::new_err(e.to_string()))
            })?;

            // Replace placeholders with image tags
            let mut py_diagrams = Vec::with_capacity(rendered.len());
            for r in rendered {
                // Display width is half the actual width (for retina displays)
                let display_width = r.width / 2;
                let image_tag = create_image_tag(&r.filename, display_width);
                let placeholder = format!("{{{{DIAGRAM_{}}}}}", r.index);
                html = html.replace(&placeholder, &image_tag);

                py_diagrams.push(PyRenderedDiagram {
                    filename: r.filename,
                    width: r.width,
                    height: r.height,
                });
            }
            py_diagrams
        };

        Ok(PyConvertResult {
            html,
            title: result.title,
            diagrams,
        })
    }
}

/// Python module definition.
#[pymodule]
pub fn md2conf_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyConvertResult>()?;
    m.add_class::<PyMarkdownConverter>()?;
    m.add_class::<PyRenderedDiagram>()?;
    Ok(())
}
