//! Python bindings via PyO3.

use std::path::PathBuf;

use pulldown_cmark::{Options, Parser};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::confluence::ConfluenceRenderer;
use crate::kroki;
use crate::plantuml;
use crate::plantuml_filter::PlantUmlFilter;

/// Diagram info with resolved source ready for rendering.
#[pyclass(name = "DiagramInfo")]
#[derive(Clone)]
pub struct PyDiagramInfo {
    /// Resolved source (includes resolved, DPI and config prepended)
    #[pyo3(get)]
    pub source: String,
    /// Zero-based index of this diagram
    #[pyo3(get)]
    pub index: usize,
}

/// Result of converting markdown to Confluence format.
#[pyclass(name = "ConvertResult")]
pub struct PyConvertResult {
    #[pyo3(get)]
    pub html: String,
    #[pyo3(get)]
    pub title: Option<String>,
    #[pyo3(get)]
    pub diagrams: Vec<PyDiagramInfo>,
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
    /// Args:
    ///     markdown_text: Markdown source text
    ///
    /// Returns:
    ///     ConvertResult with HTML, optional title, and extracted diagrams (resolved)
    pub fn convert(&self, markdown_text: &str) -> PyConvertResult {
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

        // Get extracted diagrams and resolve their sources
        let diagrams = filter
            .into_diagrams()
            .into_iter()
            .map(|d| {
                let resolved_source = plantuml::prepare_diagram_source(
                    &d.source,
                    &self.include_dirs,
                    self.config_content.as_deref(),
                );
                PyDiagramInfo {
                    source: resolved_source,
                    index: d.index,
                }
            })
            .collect();

        let html = if self.prepend_toc {
            format!("{}{}", TOC_MACRO, result.html)
        } else {
            result.html
        };

        PyConvertResult {
            html,
            title: result.title,
            diagrams,
        }
    }
}

/// Create Confluence image macro for an attachment.
#[pyfunction]
#[pyo3(signature = (filename, width = None))]
pub fn create_image_tag(filename: &str, width: Option<u32>) -> String {
    match width {
        Some(w) => format!(
            r#"<ac:image ac:width="{}"><ri:attachment ri:filename="{}" /></ac:image>"#,
            w, filename
        ),
        None => format!(
            r#"<ac:image><ri:attachment ri:filename="{}" /></ac:image>"#,
            filename
        ),
    }
}

/// Result of rendering a single diagram.
#[pyclass(name = "RenderedDiagram")]
#[derive(Clone)]
pub struct PyRenderedDiagram {
    #[pyo3(get)]
    pub index: usize,
    #[pyo3(get)]
    pub filename: String,
    #[pyo3(get)]
    pub width: u32,
    #[pyo3(get)]
    pub height: u32,
}

/// Render diagrams in parallel using Kroki service.
///
/// Args:
///     diagrams: List of DiagramInfo objects from convert()
///     server_url: Kroki server URL (e.g., "https://kroki.io")
///     output_dir: Directory to save rendered PNG files
///     pool_size: Number of parallel threads (default: 4)
///
/// Returns:
///     List of RenderedDiagram with index, filename, width, height
#[pyfunction]
#[pyo3(signature = (diagrams, server_url, output_dir, pool_size = 4))]
pub fn render_diagrams(
    py: Python<'_>,
    diagrams: Vec<PyDiagramInfo>,
    server_url: &str,
    output_dir: PathBuf,
    pool_size: usize,
) -> PyResult<Vec<PyRenderedDiagram>> {
    let requests: Vec<kroki::DiagramRequest> = diagrams
        .into_iter()
        .map(|d| kroki::DiagramRequest {
            index: d.index,
            source: d.source,
        })
        .collect();

    let server_url = server_url.trim_end_matches('/').to_string();

    py.detach(|| {
        kroki::render_all(requests, &server_url, &output_dir, pool_size)
            .map(|results| {
                results
                    .into_iter()
                    .map(|r| PyRenderedDiagram {
                        index: r.index,
                        filename: r.filename,
                        width: r.width,
                        height: r.height,
                    })
                    .collect()
            })
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    })
}

/// Python module definition.
#[pymodule]
pub fn md2conf_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(create_image_tag, m)?)?;
    m.add_function(wrap_pyfunction!(render_diagrams, m)?)?;
    m.add_class::<PyDiagramInfo>()?;
    m.add_class::<PyConvertResult>()?;
    m.add_class::<PyMarkdownConverter>()?;
    m.add_class::<PyRenderedDiagram>()?;
    Ok(())
}
