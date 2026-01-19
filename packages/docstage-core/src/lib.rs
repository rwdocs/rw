//! Python bindings for docstage-core via PyO3.

use std::path::PathBuf;

use pyo3::exceptions::{PyFileNotFoundError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use ::docstage_config::{
    CliSettings, Config, ConfigError, ConfluenceConfig, ConfluenceTestConfig, DiagramsConfig,
    DocsConfig, LiveReloadConfig, ServerConfig,
};
use ::docstage_core::{ConvertResult, HtmlConvertResult, MarkdownConverter};
use ::docstage_renderer::TocEntry;

/// Result of converting markdown to Confluence format.
#[pyclass(name = "ConvertResult")]
pub struct PyConvertResult {
    #[pyo3(get)]
    pub html: String,
    #[pyo3(get)]
    pub title: Option<String>,
    /// Filenames of rendered diagram images in output_dir.
    #[pyo3(get)]
    pub diagrams: Vec<String>,
    /// Warnings generated during conversion (e.g., unresolved includes).
    #[pyo3(get)]
    pub warnings: Vec<String>,
}

impl From<ConvertResult> for PyConvertResult {
    fn from(result: ConvertResult) -> Self {
        Self {
            html: result.html,
            title: result.title,
            diagrams: result.diagrams,
            warnings: result.warnings,
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
        let mut inner = MarkdownConverter::new()
            .gfm(gfm)
            .prepend_toc(prepend_toc)
            .extract_title(extract_title)
            .include_dirs(include_dirs.unwrap_or_default())
            .config_file(config_file);

        if let Some(dpi) = dpi {
            inner = inner.dpi(dpi);
        }

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
    ///     base_path: Optional base path for resolving relative links (e.g., "domains/billing/guide").
    ///                When provided, relative `.md` links are transformed to absolute paths (e.g., `/domains/billing/page`).
    ///
    /// Returns:
    ///     HtmlConvertResult with HTML, optional title, and table of contents
    #[pyo3(signature = (markdown_text, base_path = None))]
    pub fn convert_html(
        &self,
        markdown_text: &str,
        base_path: Option<&str>,
    ) -> PyHtmlConvertResult {
        self.inner.convert_html(markdown_text, base_path).into()
    }

    /// Convert markdown to HTML format with rendered diagrams.
    ///
    /// Produces semantic HTML5 with diagram code blocks rendered as images via Kroki.
    /// Supports PlantUML, Mermaid, GraphViz, and other Kroki-supported diagram types.
    ///
    /// Diagrams are rendered based on their format attribute:
    /// - `svg` (default): Inline SVG (supports links and interactivity)
    /// - `png`: Inline PNG as base64 data URI
    ///
    /// If diagram rendering fails, the diagram is replaced with an error message.
    /// This allows the page to still render even when Kroki is unavailable.
    ///
    /// Args:
    ///     markdown_text: Markdown source text
    ///     kroki_url: Kroki server URL (e.g., "https://kroki.io")
    ///     base_path: Optional base path for resolving relative links
    ///
    /// Returns:
    ///     HtmlConvertResult with HTML containing rendered diagrams or error messages
    #[pyo3(signature = (markdown_text, kroki_url, base_path = None))]
    pub fn convert_html_with_diagrams(
        &self,
        py: Python<'_>,
        markdown_text: &str,
        kroki_url: &str,
        base_path: Option<&str>,
    ) -> PyHtmlConvertResult {
        py.detach(|| {
            self.inner
                .convert_html_with_diagrams(markdown_text, kroki_url, base_path)
                .into()
        })
    }

    /// Convert markdown to HTML format with cached diagram rendering.
    ///
    /// Like `convert_html_with_diagrams`, but uses a file-based cache to avoid
    /// re-rendering diagrams with the same content. The cache key is computed from:
    /// - Diagram source (after preprocessing)
    /// - Kroki endpoint
    /// - Output format (svg/png)
    /// - DPI setting
    ///
    /// Cache files are stored as `{cache_dir}/{hash}.{format}` (e.g., `abc123.svg`).
    /// Rust owns the cache entirely, eliminating Python-to-Rust callbacks.
    ///
    /// Args:
    ///     markdown_text: Markdown source text
    ///     kroki_url: Kroki server URL (e.g., "https://kroki.io")
    ///     cache_dir: Directory for cached diagrams (caching disabled if None)
    ///     base_path: Optional base path for resolving relative links
    ///
    /// Returns:
    ///     HtmlConvertResult with HTML containing rendered diagrams
    #[pyo3(signature = (markdown_text, kroki_url, cache_dir = None, base_path = None))]
    pub fn convert_html_with_diagrams_cached(
        &self,
        py: Python<'_>,
        markdown_text: &str,
        kroki_url: &str,
        cache_dir: Option<PathBuf>,
        base_path: Option<&str>,
    ) -> PyHtmlConvertResult {
        py.detach(|| {
            self.inner
                .convert_html_with_diagrams_cached(
                    markdown_text,
                    kroki_url,
                    cache_dir.as_deref(),
                    base_path,
                )
                .into()
        })
    }
}

// ============================================================================
// Config bindings
// ============================================================================

/// CLI settings that override configuration file values.
#[pyclass(name = "CliSettings")]
#[derive(Clone, Default)]
pub struct PyCliSettings {
    #[pyo3(get, set)]
    pub host: Option<String>,
    #[pyo3(get, set)]
    pub port: Option<u16>,
    #[pyo3(get, set)]
    pub source_dir: Option<PathBuf>,
    #[pyo3(get, set)]
    pub cache_dir: Option<PathBuf>,
    #[pyo3(get, set)]
    pub cache_enabled: Option<bool>,
    #[pyo3(get, set)]
    pub kroki_url: Option<String>,
    #[pyo3(get, set)]
    pub live_reload_enabled: Option<bool>,
}

#[pymethods]
impl PyCliSettings {
    #[new]
    #[pyo3(signature = (
        host = None,
        port = None,
        source_dir = None,
        cache_dir = None,
        cache_enabled = None,
        kroki_url = None,
        live_reload_enabled = None
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        host: Option<String>,
        port: Option<u16>,
        source_dir: Option<PathBuf>,
        cache_dir: Option<PathBuf>,
        cache_enabled: Option<bool>,
        kroki_url: Option<String>,
        live_reload_enabled: Option<bool>,
    ) -> Self {
        Self {
            host,
            port,
            source_dir,
            cache_dir,
            cache_enabled,
            kroki_url,
            live_reload_enabled,
        }
    }
}

impl From<&PyCliSettings> for CliSettings {
    fn from(py: &PyCliSettings) -> Self {
        Self {
            host: py.host.clone(),
            port: py.port,
            source_dir: py.source_dir.clone(),
            cache_dir: py.cache_dir.clone(),
            cache_enabled: py.cache_enabled,
            kroki_url: py.kroki_url.clone(),
            live_reload_enabled: py.live_reload_enabled,
        }
    }
}

/// Server configuration.
#[pyclass(name = "ServerConfig")]
#[derive(Clone)]
pub struct PyServerConfig {
    #[pyo3(get)]
    pub host: String,
    #[pyo3(get)]
    pub port: u16,
}

impl From<&ServerConfig> for PyServerConfig {
    fn from(c: &ServerConfig) -> Self {
        Self {
            host: c.host.clone(),
            port: c.port,
        }
    }
}

/// Documentation configuration.
#[pyclass(name = "DocsConfig")]
#[derive(Clone)]
pub struct PyDocsConfig {
    #[pyo3(get)]
    pub source_dir: PathBuf,
    #[pyo3(get)]
    pub cache_dir: PathBuf,
    #[pyo3(get)]
    pub cache_enabled: bool,
}

impl From<&DocsConfig> for PyDocsConfig {
    fn from(c: &DocsConfig) -> Self {
        Self {
            source_dir: c.source_dir.clone(),
            cache_dir: c.cache_dir.clone(),
            cache_enabled: c.cache_enabled,
        }
    }
}

/// Diagram rendering configuration.
#[pyclass(name = "DiagramsConfig")]
#[derive(Clone)]
pub struct PyDiagramsConfig {
    #[pyo3(get)]
    pub kroki_url: Option<String>,
    #[pyo3(get)]
    pub include_dirs: Vec<PathBuf>,
    #[pyo3(get)]
    pub config_file: Option<String>,
    #[pyo3(get)]
    pub dpi: u32,
}

impl From<&DiagramsConfig> for PyDiagramsConfig {
    fn from(c: &DiagramsConfig) -> Self {
        Self {
            kroki_url: c.kroki_url.clone(),
            include_dirs: c.include_dirs.clone(),
            config_file: c.config_file.clone(),
            dpi: c.dpi,
        }
    }
}

/// Live reload configuration.
#[pyclass(name = "LiveReloadConfig")]
#[derive(Clone)]
pub struct PyLiveReloadConfig {
    #[pyo3(get)]
    pub enabled: bool,
    #[pyo3(get)]
    pub watch_patterns: Option<Vec<String>>,
}

impl From<&LiveReloadConfig> for PyLiveReloadConfig {
    fn from(c: &LiveReloadConfig) -> Self {
        Self {
            enabled: c.enabled,
            watch_patterns: c.watch_patterns.clone(),
        }
    }
}

/// Confluence test configuration.
#[pyclass(name = "ConfluenceTestConfig")]
#[derive(Clone)]
pub struct PyConfluenceTestConfig {
    #[pyo3(get)]
    pub space_key: String,
}

impl From<&ConfluenceTestConfig> for PyConfluenceTestConfig {
    fn from(c: &ConfluenceTestConfig) -> Self {
        Self {
            space_key: c.space_key.clone(),
        }
    }
}

/// Confluence configuration.
#[pyclass(name = "ConfluenceConfig")]
#[derive(Clone)]
pub struct PyConfluenceConfig {
    #[pyo3(get)]
    pub base_url: String,
    #[pyo3(get)]
    pub access_token: String,
    #[pyo3(get)]
    pub access_secret: String,
    #[pyo3(get)]
    pub consumer_key: String,
    #[pyo3(get)]
    pub test: Option<PyConfluenceTestConfig>,
}

impl From<&ConfluenceConfig> for PyConfluenceConfig {
    fn from(c: &ConfluenceConfig) -> Self {
        Self {
            base_url: c.base_url.clone(),
            access_token: c.access_token.clone(),
            access_secret: c.access_secret.clone(),
            consumer_key: c.consumer_key.clone(),
            test: c.test.as_ref().map(Into::into),
        }
    }
}

/// Application configuration.
#[pyclass(name = "Config")]
pub struct PyConfig {
    inner: Config,
}

#[pymethods]
impl PyConfig {
    /// Load configuration from file with optional CLI settings.
    ///
    /// If config_path is provided, loads from that file.
    /// Otherwise, searches for docstage.toml in current directory and parents.
    ///
    /// Args:
    ///     config_path: Path to configuration file (auto-discovers if None)
    ///     cli_settings: CLI settings that override config file values
    #[staticmethod]
    #[pyo3(signature = (config_path = None, cli_settings = None))]
    pub fn load(
        config_path: Option<PathBuf>,
        cli_settings: Option<&PyCliSettings>,
    ) -> PyResult<Self> {
        let rust_settings = cli_settings.map(CliSettings::from);

        Config::load(config_path.as_deref(), rust_settings.as_ref())
            .map(|inner| Self { inner })
            .map_err(|e| match e {
                ConfigError::NotFound(path) => PyFileNotFoundError::new_err(format!(
                    "Configuration file not found: {}",
                    path.display()
                )),
                ConfigError::Io(err) => PyRuntimeError::new_err(format!("IO error: {err}")),
                ConfigError::Parse(err) => {
                    PyValueError::new_err(format!("TOML parse error: {err}"))
                }
            })
    }

    /// Server configuration.
    #[getter]
    pub fn server(&self) -> PyServerConfig {
        (&self.inner.server).into()
    }

    /// Documentation configuration.
    #[getter]
    pub fn docs(&self) -> PyDocsConfig {
        (&self.inner.docs_resolved).into()
    }

    /// Diagram rendering configuration.
    #[getter]
    pub fn diagrams(&self) -> PyDiagramsConfig {
        (&self.inner.diagrams_resolved).into()
    }

    /// Live reload configuration.
    #[getter]
    pub fn live_reload(&self) -> PyLiveReloadConfig {
        (&self.inner.live_reload).into()
    }

    /// Confluence configuration (None if not configured).
    #[getter]
    pub fn confluence(&self) -> Option<PyConfluenceConfig> {
        self.inner.confluence.as_ref().map(Into::into)
    }

    /// Confluence test configuration (None if not configured).
    #[getter]
    pub fn confluence_test(&self) -> Option<PyConfluenceTestConfig> {
        self.inner.confluence_test().map(Into::into)
    }

    /// Path to the config file (None if using defaults).
    #[getter]
    pub fn config_path(&self) -> Option<PathBuf> {
        self.inner.config_path.clone()
    }
}

/// Python module definition.
#[pymodule]
#[pyo3(name = "_docstage_core")]
pub fn docstage_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Converter classes
    m.add_class::<PyConvertResult>()?;
    m.add_class::<PyHtmlConvertResult>()?;
    m.add_class::<PyMarkdownConverter>()?;
    m.add_class::<PyTocEntry>()?;

    // Config classes
    m.add_class::<PyConfig>()?;
    m.add_class::<PyCliSettings>()?;
    m.add_class::<PyServerConfig>()?;
    m.add_class::<PyDocsConfig>()?;
    m.add_class::<PyDiagramsConfig>()?;
    m.add_class::<PyLiveReloadConfig>()?;
    m.add_class::<PyConfluenceConfig>()?;
    m.add_class::<PyConfluenceTestConfig>()?;

    Ok(())
}
