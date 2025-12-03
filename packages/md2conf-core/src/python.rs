//! Python bindings via PyO3.

use pulldown_cmark::{Options, Parser};
use pyo3::prelude::*;

use crate::confluence::ConfluenceRenderer;
use crate::plantuml::{DiagramInfo as RustDiagramInfo, PlantUmlExtractor, ProcessedDocument};

/// Information about an extracted PlantUML diagram.
#[pyclass(name = "DiagramInfo")]
#[derive(Clone)]
pub struct PyDiagramInfo {
    #[pyo3(get)]
    pub source: String,
    #[pyo3(get)]
    pub resolved_source: String,
    #[pyo3(get)]
    pub index: usize,
}

impl From<RustDiagramInfo> for PyDiagramInfo {
    fn from(d: RustDiagramInfo) -> Self {
        Self {
            source: d.source,
            resolved_source: d.resolved_source,
            index: d.index,
        }
    }
}

/// Result of processing a document with PlantUML extraction.
#[pyclass(name = "ProcessedDocument")]
pub struct PyProcessedDocument {
    #[pyo3(get)]
    pub markdown: String,
    #[pyo3(get)]
    pub diagrams: Vec<PyDiagramInfo>,
    #[pyo3(get)]
    pub title: Option<String>,
}

impl From<ProcessedDocument> for PyProcessedDocument {
    fn from(doc: ProcessedDocument) -> Self {
        Self {
            markdown: doc.markdown,
            diagrams: doc.diagrams.into_iter().map(Into::into).collect(),
            title: doc.title,
        }
    }
}

/// Convert markdown to Confluence storage format.
fn markdown_to_confluence(markdown: &str, gfm: bool) -> String {
    let mut options = Options::empty();
    if gfm {
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
    }

    let parser = Parser::new_ext(markdown, options);
    ConfluenceRenderer::new().render(parser)
}

/// MkDocs document processor with PlantUML support.
#[pyclass(name = "MkDocsProcessor")]
pub struct PyMkDocsProcessor {
    extractor: PlantUmlExtractor,
}

#[pymethods]
impl PyMkDocsProcessor {
    #[new]
    #[pyo3(signature = (include_dirs, config_file = None, dpi = 192))]
    pub fn new(include_dirs: Vec<String>, config_file: Option<&str>, dpi: u32) -> Self {
        Self {
            extractor: PlantUmlExtractor::new(include_dirs, config_file, dpi),
        }
    }

    /// Extract PlantUML diagrams and title from markdown.
    ///
    /// Args:
    ///     markdown: Markdown content
    ///
    /// Returns:
    ///     ProcessedDocument with diagrams extracted and placeholders inserted
    pub fn extract_diagrams(&self, markdown: &str) -> PyProcessedDocument {
        self.extractor.process(markdown).into()
    }
}

/// Markdown to Confluence converter.
#[pyclass(name = "MarkdownConverter")]
pub struct PyMarkdownConverter {
    gfm: bool,
}

#[pymethods]
impl PyMarkdownConverter {
    #[new]
    #[pyo3(signature = (gfm = true))]
    pub fn new(gfm: bool) -> Self {
        Self { gfm }
    }

    /// Convert markdown to Confluence storage format.
    ///
    /// Args:
    ///     markdown_text: Markdown source text
    ///
    /// Returns:
    ///     Confluence XHTML storage format string
    pub fn convert(&self, markdown_text: &str) -> String {
        markdown_to_confluence(markdown_text, self.gfm)
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

/// Confluence TOC macro.
#[pyfunction]
pub fn toc_macro() -> &'static str {
    r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#
}

/// Python module definition.
#[pymodule]
pub fn md2conf_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(create_image_tag, m)?)?;
    m.add_function(wrap_pyfunction!(toc_macro, m)?)?;
    m.add_class::<PyDiagramInfo>()?;
    m.add_class::<PyProcessedDocument>()?;
    m.add_class::<PyMkDocsProcessor>()?;
    m.add_class::<PyMarkdownConverter>()?;
    Ok(())
}
