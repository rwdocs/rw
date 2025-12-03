//! Python bindings via PyO3.

use std::path::PathBuf;

use pulldown_cmark::{Options, Parser};
use pyo3::prelude::*;

use crate::confluence::ConfluenceRenderer;
use crate::plantuml;

/// Diagram info from renderer (raw source, no include resolution).
#[pyclass(name = "DiagramInfo")]
#[derive(Clone)]
pub struct PyDiagramInfo {
    #[pyo3(get)]
    pub source: String,
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
}

#[pymethods]
impl PyMarkdownConverter {
    #[new]
    #[pyo3(signature = (gfm = true, prepend_toc = false, extract_title = false))]
    pub fn new(gfm: bool, prepend_toc: bool, extract_title: bool) -> Self {
        Self {
            gfm,
            prepend_toc,
            extract_title,
        }
    }

    /// Convert markdown to Confluence storage format.
    ///
    /// Args:
    ///     markdown_text: Markdown source text
    ///
    /// Returns:
    ///     ConvertResult with HTML, optional title, and extracted diagrams
    pub fn convert(&self, markdown_text: &str) -> PyConvertResult {
        let options = get_parser_options(self.gfm);
        let parser = Parser::new_ext(markdown_text, options);

        let renderer = if self.extract_title {
            ConfluenceRenderer::new().with_title_extraction()
        } else {
            ConfluenceRenderer::new()
        };

        let result = renderer.render_with_title(parser);

        let html = if self.prepend_toc {
            format!("{}{}", TOC_MACRO, result.html)
        } else {
            result.html
        };

        let diagrams = result
            .diagrams
            .into_iter()
            .map(|d| PyDiagramInfo {
                source: d.source,
                index: d.index,
            })
            .collect();

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

/// Prepare PlantUML diagram source for rendering.
///
/// Resolves !include directives, prepends DPI and optional config.
///
/// Args:
///     source: Raw diagram source from markdown
///     include_dirs: List of directories to search for includes
///     config_file: Optional config filename to load and prepend
///
/// Returns:
///     Prepared diagram source ready for Kroki rendering
#[pyfunction]
#[pyo3(signature = (source, include_dirs, config_file = None))]
pub fn prepare_diagram_source(
    source: &str,
    include_dirs: Vec<PathBuf>,
    config_file: Option<&str>,
) -> String {
    let dirs: Vec<String> = include_dirs
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    let config_content = config_file.and_then(|cf| plantuml::load_config_file(&dirs, cf));

    plantuml::prepare_diagram_source(source, &dirs, config_content.as_deref())
}

/// Python module definition.
#[pymodule]
pub fn md2conf_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(create_image_tag, m)?)?;
    m.add_function(wrap_pyfunction!(prepare_diagram_source, m)?)?;
    m.add_class::<PyDiagramInfo>()?;
    m.add_class::<PyConvertResult>()?;
    m.add_class::<PyMarkdownConverter>()?;
    Ok(())
}
