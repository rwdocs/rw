//! Python bindings for docstage-core via PyO3.

use std::path::PathBuf;

use ::docstage_core::{ConvertResult, DiagramInfo, MarkdownConverter};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// Rendered diagram info (file written to output_dir).
#[pyclass(name = "DiagramInfo")]
#[derive(Clone)]
pub struct PyDiagramInfo {
    #[pyo3(get)]
    pub filename: String,
    #[pyo3(get)]
    pub width: u32,
    #[pyo3(get)]
    pub height: u32,
}

impl From<DiagramInfo> for PyDiagramInfo {
    fn from(info: DiagramInfo) -> Self {
        Self {
            filename: info.filename,
            width: info.width,
            height: info.height,
        }
    }
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

impl From<ConvertResult> for PyConvertResult {
    fn from(result: ConvertResult) -> Self {
        Self {
            html: result.html,
            title: result.title,
            diagrams: result.diagrams.into_iter().map(Into::into).collect(),
        }
    }
}

/// Markdown to Confluence converter.
#[pyclass(name = "MarkdownConverter")]
pub struct PyMarkdownConverter {
    inner: MarkdownConverter,
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
        let inner = MarkdownConverter::new()
            .gfm(gfm)
            .prepend_toc(prepend_toc)
            .extract_title(extract_title)
            .include_dirs(include_dirs.unwrap_or_default())
            .config_file(config_file);

        Self { inner }
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
        py.detach(|| {
            self.inner
                .convert(markdown_text, kroki_url, &output_dir)
                .map(Into::into)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        })
    }
}

/// Python module definition.
#[pymodule]
pub fn docstage_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyConvertResult>()?;
    m.add_class::<PyMarkdownConverter>()?;
    m.add_class::<PyDiagramInfo>()?;
    Ok(())
}
