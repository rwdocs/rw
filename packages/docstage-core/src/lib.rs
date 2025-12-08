//! Python bindings for docstage-core via PyO3.

use std::path::PathBuf;

use ::docstage_core::{
    ConvertResult, DiagramInfo, HtmlConvertResult, MarkdownConverter, TocEntry, DEFAULT_DPI,
};
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

/// Table of contents entry.
#[pyclass(name = "TocEntry")]
#[derive(Clone)]
pub struct PyTocEntry {
    /// Heading level (1-6).
    #[pyo3(get)]
    pub level: u8,
    /// Heading text.
    #[pyo3(get)]
    pub title: String,
    /// Anchor ID for linking.
    #[pyo3(get)]
    pub id: String,
}

impl From<TocEntry> for PyTocEntry {
    fn from(entry: TocEntry) -> Self {
        Self {
            level: entry.level,
            title: entry.title,
            id: entry.id,
        }
    }
}

/// Result of converting markdown to HTML format.
#[pyclass(name = "HtmlConvertResult")]
pub struct PyHtmlConvertResult {
    /// Rendered HTML content.
    #[pyo3(get)]
    pub html: String,
    /// Title extracted from first H1 heading (if `extract_title` was enabled).
    #[pyo3(get)]
    pub title: Option<String>,
    /// Table of contents entries.
    #[pyo3(get)]
    pub toc: Vec<PyTocEntry>,
    /// Warnings generated during conversion (e.g., unresolved includes).
    #[pyo3(get)]
    pub warnings: Vec<String>,
}

impl From<HtmlConvertResult> for PyHtmlConvertResult {
    fn from(result: HtmlConvertResult) -> Self {
        Self {
            html: result.html,
            title: result.title,
            toc: result.toc.into_iter().map(Into::into).collect(),
            warnings: result.warnings,
        }
    }
}

/// Markdown converter with multiple output formats.
#[pyclass(name = "MarkdownConverter")]
pub struct PyMarkdownConverter {
    inner: MarkdownConverter,
}

#[pymethods]
impl PyMarkdownConverter {
    #[new]
    #[pyo3(signature = (gfm = true, prepend_toc = false, extract_title = false, include_dirs = None, config_file = None, dpi = None))]
    pub fn new(
        gfm: bool,
        prepend_toc: bool,
        extract_title: bool,
        include_dirs: Option<Vec<PathBuf>>,
        config_file: Option<&str>,
        dpi: Option<u32>,
    ) -> Self {
        let inner = MarkdownConverter::new()
            .gfm(gfm)
            .prepend_toc(prepend_toc)
            .extract_title(extract_title)
            .include_dirs(include_dirs.unwrap_or_default())
            .config_file(config_file)
            .dpi(dpi.unwrap_or(DEFAULT_DPI));

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

    /// Convert markdown to HTML format.
    ///
    /// Produces semantic HTML5 with syntax highlighting and table of contents.
    /// Diagram code blocks are rendered as syntax-highlighted code.
    /// For rendered diagram images, use `convert_html_with_diagrams()`.
    ///
    /// Args:
    ///     markdown_text: Markdown source text
    ///
    /// Returns:
    ///     HtmlConvertResult with HTML, optional title, and table of contents
    pub fn convert_html(&self, markdown_text: &str) -> PyHtmlConvertResult {
        self.inner.convert_html(markdown_text).into()
    }

    /// Convert markdown to HTML format with rendered diagrams.
    ///
    /// Produces semantic HTML5 with diagram code blocks rendered as images via Kroki.
    /// Supports PlantUML, Mermaid, GraphViz, and other Kroki-supported diagram types.
    ///
    /// Diagrams are rendered based on their format attribute:
    /// - `svg` (default): Inline SVG (supports links and interactivity)
    /// - `png`: Inline PNG as base64 data URI
    /// - `img`: External SVG via `<img>` tag (falls back to inline SVG)
    ///
    /// If diagram rendering fails, the diagram is replaced with an error message.
    /// This allows the page to still render even when Kroki is unavailable.
    ///
    /// Args:
    ///     markdown_text: Markdown source text
    ///     kroki_url: Kroki server URL (e.g., "https://kroki.io")
    ///
    /// Returns:
    ///     HtmlConvertResult with HTML containing rendered diagrams or error messages
    pub fn convert_html_with_diagrams(
        &self,
        py: Python<'_>,
        markdown_text: &str,
        kroki_url: &str,
    ) -> PyHtmlConvertResult {
        py.detach(|| {
            self.inner
                .convert_html_with_diagrams(markdown_text, kroki_url)
                .into()
        })
    }
}

/// Python module definition.
#[pymodule]
pub fn docstage_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyConvertResult>()?;
    m.add_class::<PyHtmlConvertResult>()?;
    m.add_class::<PyMarkdownConverter>()?;
    m.add_class::<PyDiagramInfo>()?;
    m.add_class::<PyTocEntry>()?;
    Ok(())
}
