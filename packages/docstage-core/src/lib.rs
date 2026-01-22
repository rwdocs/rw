//! Python bindings for docstage-core via PyO3.

use std::path::PathBuf;

use pyo3::exceptions::{PyFileNotFoundError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use ::docstage_config::{
    CliSettings, Config, ConfigError, ConfluenceConfig, DiagramsConfig, DocsConfig,
    LiveReloadConfig, ServerConfig,
};
use ::docstage_confluence::ConfluenceClient;

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
    fn from(settings: &PyCliSettings) -> Self {
        Self {
            host: settings.host.clone(),
            port: settings.port,
            source_dir: settings.source_dir.clone(),
            cache_dir: settings.cache_dir.clone(),
            cache_enabled: settings.cache_enabled,
            kroki_url: settings.kroki_url.clone(),
            live_reload_enabled: settings.live_reload_enabled,
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

impl From<&PyDiagramsConfig> for DiagramsConfig {
    fn from(c: &PyDiagramsConfig) -> Self {
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
}

impl From<&ConfluenceConfig> for PyConfluenceConfig {
    fn from(c: &ConfluenceConfig) -> Self {
        Self {
            base_url: c.base_url.clone(),
            access_token: c.access_token.clone(),
            access_secret: c.access_secret.clone(),
            consumer_key: c.consumer_key.clone(),
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
                ConfigError::Validation(err) => {
                    PyValueError::new_err(format!("Configuration validation error: {err}"))
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

    /// Path to the config file (None if using defaults).
    #[getter]
    pub fn config_path(&self) -> Option<PathBuf> {
        self.inner.config_path.clone()
    }
}

// ============================================================================
// HTTP Server bindings
// ============================================================================

/// HTTP server configuration.
///
/// This is the configuration for the native Rust HTTP server.
#[pyclass(name = "HttpServerConfig")]
#[derive(Clone)]
pub struct PyHttpServerConfig {
    /// Host address to bind to.
    #[pyo3(get, set)]
    pub host: String,
    /// Port to listen on.
    #[pyo3(get, set)]
    pub port: u16,
    /// Documentation source directory.
    #[pyo3(get, set)]
    pub source_dir: PathBuf,
    /// Cache directory (`None` disables caching).
    #[pyo3(get, set)]
    pub cache_dir: Option<PathBuf>,
    /// Kroki URL for diagrams (`None` disables diagrams).
    #[pyo3(get, set)]
    pub kroki_url: Option<String>,
    /// `PlantUML` include directories.
    #[pyo3(get, set)]
    pub include_dirs: Vec<PathBuf>,
    /// `PlantUML` config file.
    #[pyo3(get, set)]
    pub config_file: Option<String>,
    /// Diagram DPI.
    #[pyo3(get, set)]
    pub dpi: u32,
    /// Enable live reload.
    #[pyo3(get, set)]
    pub live_reload_enabled: bool,
    /// Watch patterns for live reload.
    #[pyo3(get, set)]
    pub watch_patterns: Option<Vec<String>>,
    /// Enable verbose output.
    #[pyo3(get, set)]
    pub verbose: bool,
    /// Application version (for cache invalidation).
    #[pyo3(get, set)]
    pub version: String,
}

#[pymethods]
impl PyHttpServerConfig {
    /// Create configuration from a Config object.
    ///
    /// Args:
    ///     config: Application configuration
    ///     version: Application version
    ///     verbose: Enable verbose output
    #[staticmethod]
    pub fn from_config(config: &PyConfig, version: String, verbose: bool) -> Self {
        ::docstage_server::server_config_from_docstage_config(&config.inner, version, verbose)
            .into()
    }
}

impl From<::docstage_server::ServerConfig> for PyHttpServerConfig {
    fn from(c: ::docstage_server::ServerConfig) -> Self {
        let ::docstage_server::ServerConfig {
            host,
            port,
            source_dir,
            cache_dir,
            kroki_url,
            include_dirs,
            config_file,
            dpi,
            live_reload_enabled,
            watch_patterns,
            verbose,
            version,
        } = c;
        Self {
            host,
            port,
            source_dir,
            cache_dir,
            kroki_url,
            include_dirs,
            config_file,
            dpi,
            live_reload_enabled,
            watch_patterns,
            verbose,
            version,
        }
    }
}

impl From<PyHttpServerConfig> for ::docstage_server::ServerConfig {
    fn from(c: PyHttpServerConfig) -> Self {
        let PyHttpServerConfig {
            host,
            port,
            source_dir,
            cache_dir,
            kroki_url,
            include_dirs,
            config_file,
            dpi,
            live_reload_enabled,
            watch_patterns,
            verbose,
            version,
        } = c;
        Self {
            host,
            port,
            source_dir,
            cache_dir,
            kroki_url,
            include_dirs,
            config_file,
            dpi,
            live_reload_enabled,
            watch_patterns,
            verbose,
            version,
        }
    }
}

/// Run the HTTP server.
///
/// This function starts the native Rust HTTP server and blocks until it is shut down.
///
/// Args:
///     config: Server configuration
///
/// Raises:
///     RuntimeError: If the server fails to start
#[pyfunction]
#[pyo3(name = "run_http_server")]
pub fn py_run_http_server(py: Python<'_>, config: PyHttpServerConfig) -> PyResult<()> {
    py.detach(|| {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create async runtime: {e}")))?;

        runtime
            .block_on(::docstage_server::run_server(config.into()))
            .map_err(|e| PyRuntimeError::new_err(format!("Server error: {e}")))
    })
}

// ============================================================================
// Confluence Client bindings
// ============================================================================

use ::docstage_confluence::{ConfluenceError, Page as ConfluencePage};

/// Comment that could not be placed in new HTML.
#[pyclass(name = "UnmatchedComment", frozen)]
#[derive(Clone)]
pub struct PyUnmatchedComment {
    /// Comment reference ID.
    #[pyo3(get)]
    pub ref_id: String,
    /// Text content the marker was wrapping.
    #[pyo3(get)]
    pub text: String,
}

impl From<::docstage_confluence::UnmatchedComment> for PyUnmatchedComment {
    fn from(c: ::docstage_confluence::UnmatchedComment) -> Self {
        Self {
            ref_id: c.ref_id,
            text: c.text,
        }
    }
}

/// Confluence page.
#[pyclass(name = "ConfluencePage")]
#[derive(Clone)]
pub struct PyConfluencePage {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub title: String,
    #[pyo3(get)]
    pub version: u32,
    #[pyo3(get)]
    pub body: Option<String>,
}

impl From<ConfluencePage> for PyConfluencePage {
    fn from(page: ConfluencePage) -> Self {
        Self {
            id: page.id,
            title: page.title,
            version: page.version.number,
            body: page.body.and_then(|b| b.storage).map(|s| s.value),
        }
    }
}

fn confluence_error_to_py(e: ConfluenceError) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

/// Confluence REST API client with OAuth 1.0 RSA-SHA1 authentication.
#[pyclass(name = "ConfluenceClient")]
pub struct PyConfluenceClient {
    inner: ConfluenceClient,
}

#[pymethods]
impl PyConfluenceClient {
    /// Create a new Confluence client.
    ///
    /// Args:
    ///     base_url: Confluence server base URL
    ///     consumer_key: OAuth consumer key
    ///     private_key: PEM-encoded RSA private key bytes
    ///     access_token: OAuth access token
    ///     access_secret: OAuth access token secret
    #[new]
    #[pyo3(signature = (base_url, consumer_key, private_key, access_token, access_secret))]
    pub fn new(
        base_url: &str,
        consumer_key: &str,
        private_key: &[u8],
        access_token: &str,
        access_secret: &str,
    ) -> PyResult<Self> {
        let inner = ConfluenceClient::from_config(
            base_url,
            consumer_key,
            private_key,
            access_token,
            access_secret,
        )
        .map_err(confluence_error_to_py)?;
        Ok(Self { inner })
    }

    /// Get the base URL.
    #[getter]
    pub fn base_url(&self) -> &str {
        self.inner.base_url()
    }

    /// Update a Confluence page from markdown content.
    ///
    /// This method performs the entire update workflow in a single call:
    /// 1. Converts markdown to Confluence storage format
    /// 2. Fetches current page content
    /// 3. Preserves inline comments from current page
    /// 4. Uploads diagram attachments
    /// 5. Updates the page with new content
    ///
    /// Args:
    ///     page_id: Page ID to update
    ///     markdown_text: Markdown content
    ///     diagrams: Diagram rendering configuration
    ///     extract_title: Whether to extract title from first H1 heading
    ///     message: Optional version message
    ///
    /// Returns:
    ///     UpdateResult with page info, URL, and comment status
    #[pyo3(signature = (page_id, markdown_text, diagrams, extract_title = true, message = None))]
    pub fn update_page_from_markdown(
        &self,
        py: Python<'_>,
        page_id: &str,
        markdown_text: &str,
        diagrams: &PyDiagramsConfig,
        extract_title: bool,
        message: Option<&str>,
    ) -> PyResult<PyUpdateResult> {
        let config = ::docstage_core::updater::UpdateConfig {
            diagrams: diagrams.into(),
            extract_title,
        };
        py.allow_threads(|| {
            let updater = ::docstage_core::updater::PageUpdater::new(&self.inner, config);
            updater
                .update(page_id, markdown_text, message)
                .map(Into::into)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Perform a dry-run update (no changes made).
    ///
    /// Returns information about what would change without
    /// actually updating the page or uploading attachments.
    ///
    /// Args:
    ///     page_id: Page ID to check
    ///     markdown_text: Markdown content
    ///     diagrams: Diagram rendering configuration
    ///     extract_title: Whether to extract title from first H1 heading
    ///
    /// Returns:
    ///     DryRunResult with preview of changes
    #[pyo3(signature = (page_id, markdown_text, diagrams, extract_title = true))]
    pub fn dry_run_update(
        &self,
        py: Python<'_>,
        page_id: &str,
        markdown_text: &str,
        diagrams: &PyDiagramsConfig,
        extract_title: bool,
    ) -> PyResult<PyDryRunResult> {
        let config = ::docstage_core::updater::UpdateConfig {
            diagrams: diagrams.into(),
            extract_title,
        };
        py.allow_threads(|| {
            let updater = ::docstage_core::updater::PageUpdater::new(&self.inner, config);
            updater
                .dry_run(page_id, markdown_text)
                .map(Into::into)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        })
    }
}

/// Read RSA private key from PEM file.
///
/// Args:
///     path: Path to PEM file
///
/// Returns:
///     PEM-encoded key bytes
#[pyfunction]
#[pyo3(name = "read_private_key")]
pub fn py_read_private_key(path: PathBuf) -> PyResult<Vec<u8>> {
    ::docstage_confluence::oauth::read_private_key(&path).map_err(|e| {
        if let ConfluenceError::Io(ref io_err) = e {
            if io_err.kind() == std::io::ErrorKind::NotFound {
                return PyFileNotFoundError::new_err(format!(
                    "Private key file not found: {}",
                    path.display()
                ));
            }
        }
        PyRuntimeError::new_err(e.to_string())
    })
}

// ============================================================================
// Page Updater result bindings
// ============================================================================

/// Result of updating a Confluence page.
#[pyclass(name = "UpdateResult")]
pub struct PyUpdateResult {
    #[pyo3(get)]
    pub page: PyConfluencePage,
    #[pyo3(get)]
    pub url: String,
    #[pyo3(get)]
    pub comment_count: usize,
    #[pyo3(get)]
    pub unmatched_comments: Vec<PyUnmatchedComment>,
    #[pyo3(get)]
    pub attachments_uploaded: usize,
    #[pyo3(get)]
    pub warnings: Vec<String>,
}

impl From<::docstage_core::updater::UpdateResult> for PyUpdateResult {
    fn from(r: ::docstage_core::updater::UpdateResult) -> Self {
        Self {
            page: r.page.into(),
            url: r.url,
            comment_count: r.comment_count,
            unmatched_comments: r.unmatched_comments.into_iter().map(Into::into).collect(),
            attachments_uploaded: r.attachments_uploaded,
            warnings: r.warnings,
        }
    }
}

/// Result of dry-run update operation.
#[pyclass(name = "DryRunResult")]
pub struct PyDryRunResult {
    #[pyo3(get)]
    pub html: String,
    #[pyo3(get)]
    pub title: Option<String>,
    #[pyo3(get)]
    pub current_title: String,
    #[pyo3(get)]
    pub current_version: u32,
    #[pyo3(get)]
    pub unmatched_comments: Vec<PyUnmatchedComment>,
    #[pyo3(get)]
    pub attachment_count: usize,
    #[pyo3(get)]
    pub attachment_names: Vec<String>,
    #[pyo3(get)]
    pub warnings: Vec<String>,
}

impl From<::docstage_core::updater::DryRunResult> for PyDryRunResult {
    fn from(r: ::docstage_core::updater::DryRunResult) -> Self {
        Self {
            html: r.html,
            title: r.title,
            current_title: r.current_title,
            current_version: r.current_version,
            unmatched_comments: r.unmatched_comments.into_iter().map(Into::into).collect(),
            attachment_count: r.attachment_count,
            attachment_names: r.attachment_names,
            warnings: r.warnings,
        }
    }
}

// ============================================================================
// Python module definition
// ============================================================================

#[pymodule]
#[pyo3(name = "_docstage_core")]
pub fn docstage_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Config classes
    m.add_class::<PyConfig>()?;
    m.add_class::<PyCliSettings>()?;
    m.add_class::<PyServerConfig>()?;
    m.add_class::<PyDocsConfig>()?;
    m.add_class::<PyDiagramsConfig>()?;
    m.add_class::<PyLiveReloadConfig>()?;
    m.add_class::<PyConfluenceConfig>()?;

    // HTTP Server
    m.add_class::<PyHttpServerConfig>()?;
    m.add_function(wrap_pyfunction!(py_run_http_server, m)?)?;

    // Confluence Client
    m.add_class::<PyUnmatchedComment>()?;
    m.add_class::<PyConfluencePage>()?;
    m.add_class::<PyConfluenceClient>()?;
    m.add_function(wrap_pyfunction!(py_read_private_key, m)?)?;

    // Page Updater results
    m.add_class::<PyUpdateResult>()?;
    m.add_class::<PyDryRunResult>()?;

    Ok(())
}
