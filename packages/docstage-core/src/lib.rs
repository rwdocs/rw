//! Python bindings for docstage-core via PyO3.

use std::path::PathBuf;

use pyo3::exceptions::{PyFileNotFoundError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use ::docstage_config::{
    CliSettings, Config, ConfigError, ConfluenceConfig, ConfluenceTestConfig, DiagramsConfig,
    DocsConfig, LiveReloadConfig, ServerConfig,
};
use ::docstage_core::{
    build_navigation, BreadcrumbItem, ConvertResult, HtmlConvertResult, MarkdownConverter, NavItem,
    Page, PageRenderResult, PageRenderer, PageRendererConfig, RenderError, Site, SiteLoader,
    SiteLoaderConfig,
};
use ::docstage_renderer::TocEntry;

/// Result of converting markdown to Confluence format.
#[pyclass(name = "ConvertResult")]
pub struct PyConvertResult {
    #[pyo3(get)]
    pub html: String,
    #[pyo3(get)]
    pub title: Option<String>,
    /// Warnings generated during conversion (e.g., unresolved includes).
    #[pyo3(get)]
    pub warnings: Vec<String>,
}

impl From<ConvertResult> for PyConvertResult {
    fn from(result: ConvertResult) -> Self {
        Self {
            html: result.html,
            title: result.title,
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
    ) -> PyConvertResult {
        py.detach(|| {
            self.inner
                .convert(markdown_text, kroki_url, &output_dir)
                .into()
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
// PageRenderer bindings
// ============================================================================

/// Result of rendering a markdown page.
#[pyclass(name = "PageRenderResult")]
pub struct PyPageRenderResult {
    /// Rendered HTML content.
    #[pyo3(get)]
    pub html: String,
    /// Title extracted from first H1 heading (if enabled).
    #[pyo3(get)]
    pub title: Option<String>,
    /// Table of contents entries.
    #[pyo3(get)]
    pub toc: Vec<PyTocEntry>,
    /// Warnings generated during conversion (e.g., unresolved includes).
    #[pyo3(get)]
    pub warnings: Vec<String>,
    /// Whether result was served from cache.
    #[pyo3(get)]
    pub from_cache: bool,
}

impl From<PageRenderResult> for PyPageRenderResult {
    fn from(result: PageRenderResult) -> Self {
        Self {
            html: result.html,
            title: result.title,
            toc: result.toc.into_iter().map(Into::into).collect(),
            warnings: result.warnings,
            from_cache: result.from_cache,
        }
    }
}

/// Configuration for page renderer.
#[pyclass(name = "PageRendererConfig")]
#[derive(Clone)]
pub struct PyPageRendererConfig {
    /// Cache directory for rendered pages and metadata.
    #[pyo3(get, set)]
    pub cache_dir: Option<PathBuf>,
    /// Application version for cache invalidation.
    #[pyo3(get, set)]
    pub version: String,
    /// Extract title from first H1 heading.
    #[pyo3(get, set)]
    pub extract_title: bool,
    /// Kroki URL for diagram rendering (None disables diagrams).
    #[pyo3(get, set)]
    pub kroki_url: Option<String>,
    /// Directories to search for PlantUML includes.
    #[pyo3(get, set)]
    pub include_dirs: Vec<PathBuf>,
    /// PlantUML config file name.
    #[pyo3(get, set)]
    pub config_file: Option<String>,
    /// DPI for diagram rendering.
    #[pyo3(get, set)]
    pub dpi: u32,
}

#[pymethods]
impl PyPageRendererConfig {
    #[new]
    #[pyo3(signature = (
        cache_dir = None,
        version = String::new(),
        extract_title = true,
        kroki_url = None,
        include_dirs = Vec::new(),
        config_file = None,
        dpi = 192
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cache_dir: Option<PathBuf>,
        version: String,
        extract_title: bool,
        kroki_url: Option<String>,
        include_dirs: Vec<PathBuf>,
        config_file: Option<String>,
        dpi: u32,
    ) -> Self {
        Self {
            cache_dir,
            version,
            extract_title,
            kroki_url,
            include_dirs,
            config_file,
            dpi,
        }
    }
}

impl From<PyPageRendererConfig> for PageRendererConfig {
    fn from(config: PyPageRendererConfig) -> Self {
        Self {
            cache_dir: config.cache_dir,
            version: config.version,
            extract_title: config.extract_title,
            kroki_url: config.kroki_url,
            include_dirs: config.include_dirs,
            config_file: config.config_file,
            dpi: config.dpi,
        }
    }
}

/// Page renderer with file-based caching.
///
/// Uses MarkdownConverter for actual conversion and PageCache for persistence.
/// Cache invalidation is based on source file mtime and build version.
#[pyclass(name = "PageRenderer")]
pub struct PyPageRenderer {
    inner: PageRenderer,
}

#[pymethods]
impl PyPageRenderer {
    #[new]
    pub fn new(config: PyPageRendererConfig) -> Self {
        Self {
            inner: PageRenderer::new(config.into()),
        }
    }

    /// Render a markdown page.
    ///
    /// Args:
    ///     source_path: Absolute path to markdown source file
    ///     base_path: URL path for resolving relative links (e.g., "domain-a/guide")
    ///
    /// Returns:
    ///     PageRenderResult with HTML, title, ToC, and cache status
    ///
    /// Raises:
    ///     FileNotFoundError: If source markdown file doesn't exist
    ///     OSError: If file cannot be read
    pub fn render(
        &self,
        py: Python<'_>,
        source_path: PathBuf,
        base_path: &str,
    ) -> PyResult<PyPageRenderResult> {
        py.detach(|| {
            self.inner
                .render(&source_path, base_path)
                .map(Into::into)
                .map_err(|e| match e {
                    RenderError::FileNotFound(path) => PyFileNotFoundError::new_err(format!(
                        "Source file not found: {}",
                        path.display()
                    )),
                    RenderError::Io(err) => PyRuntimeError::new_err(format!("IO error: {err}")),
                })
        })
    }

    /// Invalidate cache entry for a path.
    ///
    /// Args:
    ///     path: Document path to invalidate
    pub fn invalidate(&self, path: &str) {
        self.inner.invalidate(path);
    }
}

// ============================================================================
// Site bindings
// ============================================================================

/// Document page data.
#[pyclass(name = "Page", frozen)]
pub struct PyPage {
    /// Page title.
    #[pyo3(get)]
    pub title: String,
    /// URL path (e.g., "/guide").
    #[pyo3(get)]
    pub path: String,
    /// Relative path to source file.
    #[pyo3(get)]
    pub source_path: PathBuf,
}

impl From<&Page> for PyPage {
    fn from(page: &Page) -> Self {
        Self {
            title: page.title.clone(),
            path: page.path.clone(),
            source_path: page.source_path.clone(),
        }
    }
}

/// Breadcrumb navigation item.
#[pyclass(name = "BreadcrumbItem", frozen)]
pub struct PyBreadcrumbItem {
    /// Display title.
    #[pyo3(get)]
    pub title: String,
    /// Link target path.
    #[pyo3(get)]
    pub path: String,
}

impl From<&BreadcrumbItem> for PyBreadcrumbItem {
    fn from(item: &BreadcrumbItem) -> Self {
        Self {
            title: item.title.clone(),
            path: item.path.clone(),
        }
    }
}

#[pymethods]
impl PyBreadcrumbItem {
    /// Convert to dictionary for JSON serialization.
    pub fn to_dict(&self) -> std::collections::HashMap<String, String> {
        std::collections::HashMap::from([
            ("title".to_string(), self.title.clone()),
            ("path".to_string(), self.path.clone()),
        ])
    }
}

/// Document site structure with efficient path lookups.
#[pyclass(name = "Site")]
pub struct PySite {
    inner: Site,
}

impl PySite {
    fn new(inner: Site) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PySite {
    /// Get page by URL path.
    ///
    /// Args:
    ///     path: URL path (e.g., "/guide")
    ///
    /// Returns:
    ///     Page if found, None otherwise
    pub fn get_page(&self, path: &str) -> Option<PyPage> {
        self.inner.get_page(path).map(|p| p.into())
    }

    /// Get children of a page.
    ///
    /// Args:
    ///     path: URL path of the parent page
    ///
    /// Returns:
    ///     List of child pages
    pub fn get_children(&self, path: &str) -> Vec<PyPage> {
        self.inner
            .get_children(path)
            .into_iter()
            .map(|p| p.into())
            .collect()
    }

    /// Build breadcrumbs for a given path.
    ///
    /// Args:
    ///     path: URL path (e.g., "/guide/setup")
    ///
    /// Returns:
    ///     List of breadcrumb items for ancestor navigation
    pub fn get_breadcrumbs(&self, path: &str) -> Vec<PyBreadcrumbItem> {
        self.inner
            .get_breadcrumbs(path)
            .iter()
            .map(|b| b.into())
            .collect()
    }

    /// Get root-level pages.
    pub fn get_root_pages(&self) -> Vec<PyPage> {
        self.inner
            .get_root_pages()
            .into_iter()
            .map(|p| p.into())
            .collect()
    }

    /// Resolve URL path to absolute source file path.
    ///
    /// Args:
    ///     path: URL path (e.g., "/guide")
    ///
    /// Returns:
    ///     Absolute path to source markdown file, or None if not found
    pub fn resolve_source_path(&self, path: &str) -> Option<PathBuf> {
        self.inner.resolve_source_path(path)
    }

    /// Get page by source file path.
    ///
    /// Args:
    ///     source_path: Relative path to source file (e.g., "guide.md")
    ///
    /// Returns:
    ///     Page if found, None otherwise
    pub fn get_page_by_source(&self, source_path: PathBuf) -> Option<PyPage> {
        self.inner.get_page_by_source(&source_path).map(|p| p.into())
    }

    /// Get source directory.
    #[getter]
    pub fn source_dir(&self) -> PathBuf {
        self.inner.source_dir().to_path_buf()
    }
}

/// Configuration for site loader.
#[pyclass(name = "SiteLoaderConfig")]
#[derive(Clone)]
pub struct PySiteLoaderConfig {
    /// Root directory containing markdown sources.
    #[pyo3(get, set)]
    pub source_dir: PathBuf,
    /// Cache directory for site structure (None disables caching).
    #[pyo3(get, set)]
    pub cache_dir: Option<PathBuf>,
}

#[pymethods]
impl PySiteLoaderConfig {
    #[new]
    #[pyo3(signature = (source_dir, cache_dir = None))]
    pub fn new(source_dir: PathBuf, cache_dir: Option<PathBuf>) -> Self {
        Self {
            source_dir,
            cache_dir,
        }
    }
}

impl From<PySiteLoaderConfig> for SiteLoaderConfig {
    fn from(config: PySiteLoaderConfig) -> Self {
        Self {
            source_dir: config.source_dir,
            cache_dir: config.cache_dir,
        }
    }
}

/// Loads site structure from filesystem.
#[pyclass(name = "SiteLoader")]
pub struct PySiteLoader {
    inner: SiteLoader,
}

#[pymethods]
impl PySiteLoader {
    #[new]
    pub fn new(config: PySiteLoaderConfig) -> Self {
        Self {
            inner: SiteLoader::new(config.into()),
        }
    }

    /// Load site structure from directory.
    ///
    /// Args:
    ///     use_cache: Whether to use cached data if available (default: True)
    ///
    /// Returns:
    ///     Site structure
    #[pyo3(signature = (use_cache = true))]
    pub fn load(&mut self, use_cache: bool) -> PySite {
        PySite::new(self.inner.load(use_cache).clone())
    }

    /// Invalidate cached site.
    pub fn invalidate(&mut self) {
        self.inner.invalidate();
    }

    /// Get source directory.
    #[getter]
    pub fn source_dir(&self) -> PathBuf {
        self.inner.source_dir().to_path_buf()
    }
}

/// Navigation item with children for UI tree.
#[pyclass(name = "NavItem")]
#[derive(Clone)]
pub struct PyNavItem {
    /// Display title.
    #[pyo3(get)]
    pub title: String,
    /// Link target path.
    #[pyo3(get)]
    pub path: String,
    /// Child navigation items.
    #[pyo3(get)]
    pub children: Vec<PyNavItem>,
}

impl From<NavItem> for PyNavItem {
    fn from(item: NavItem) -> Self {
        Self {
            title: item.title,
            path: item.path,
            children: item.children.into_iter().map(|c| c.into()).collect(),
        }
    }
}

#[pymethods]
impl PyNavItem {
    /// Convert to dictionary for JSON serialization.
    pub fn to_dict(&self, py: Python<'_>) -> Py<pyo3::types::PyAny> {
        let dict = pyo3::types::PyDict::new(py);
        let _ = dict.set_item("title", &self.title);
        let _ = dict.set_item("path", &self.path);
        if !self.children.is_empty() {
            let children: Vec<_> = self.children.iter().map(|c| c.to_dict(py)).collect();
            let _ = dict.set_item("children", children);
        }
        dict.into()
    }
}

/// Build navigation tree from site structure.
///
/// Args:
///     site: Site structure to build navigation from
///
/// Returns:
///     List of navigation items for UI tree
#[pyfunction]
#[pyo3(name = "build_navigation")]
pub fn py_build_navigation(site: &PySite) -> Vec<PyNavItem> {
    build_navigation(&site.inner)
        .into_iter()
        .map(|item| item.into())
        .collect()
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
// Comment Preservation bindings
// ============================================================================

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

/// Result of comment preservation operation.
#[pyclass(name = "PreserveResult")]
pub struct PyPreserveResult {
    /// HTML with preserved comment markers.
    #[pyo3(get)]
    pub html: String,
    /// Comments that could not be placed in the new HTML.
    #[pyo3(get)]
    pub unmatched_comments: Vec<PyUnmatchedComment>,
}

impl From<::docstage_confluence::PreserveResult> for PyPreserveResult {
    fn from(r: ::docstage_confluence::PreserveResult) -> Self {
        Self {
            html: r.html,
            unmatched_comments: r.unmatched_comments.into_iter().map(Into::into).collect(),
        }
    }
}

/// Preserve inline comment markers from old HTML in new HTML.
///
/// This function transfers comment markers from the old Confluence page HTML
/// to the new HTML generated from markdown conversion. It uses tree-based
/// comparison to match content and transfer markers to matching positions.
///
/// Args:
///     old_html: Current page HTML with comment markers
///     new_html: New HTML from markdown conversion
///
/// Returns:
///     PreserveResult with HTML containing preserved markers and any unmatched comments
#[pyfunction]
#[pyo3(name = "preserve_comments")]
pub fn py_preserve_comments(old_html: &str, new_html: &str) -> PyPreserveResult {
    ::docstage_confluence::preserve_comments(old_html, new_html).into()
}

// ============================================================================
// Confluence Client bindings
// ============================================================================

use ::docstage_confluence::{
    Attachment, AttachmentsResponse, Comment, CommentsResponse, ConfluenceClient, ConfluenceError,
    Extensions, InlineProperties, Page as ConfluencePage, Resolution, Version,
};

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
            body: page
                .body
                .and_then(|b| b.storage)
                .map(|s| s.value),
        }
    }
}

/// Confluence comment.
#[pyclass(name = "ConfluenceComment")]
#[derive(Clone)]
pub struct PyConfluenceComment {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub title: String,
    #[pyo3(get)]
    pub body: Option<String>,
    #[pyo3(get)]
    pub marker_ref: Option<String>,
    #[pyo3(get)]
    pub original_selection: Option<String>,
    #[pyo3(get)]
    pub status: Option<String>,
}

impl From<Comment> for PyConfluenceComment {
    fn from(comment: Comment) -> Self {
        let inline_props = comment
            .extensions
            .as_ref()
            .and_then(|e| e.inline_properties.as_ref());
        let resolution = comment
            .extensions
            .as_ref()
            .and_then(|e| e.resolution.as_ref());

        Self {
            id: comment.id,
            title: comment.title,
            body: comment.body.storage.map(|s| s.value),
            marker_ref: inline_props.map(|p| p.marker_ref.clone()),
            original_selection: inline_props.map(|p| p.original_selection.clone()),
            status: resolution.map(|r| r.status.clone()),
        }
    }
}

/// Confluence comments response.
#[pyclass(name = "ConfluenceCommentsResponse")]
#[derive(Clone)]
pub struct PyConfluenceCommentsResponse {
    #[pyo3(get)]
    pub results: Vec<PyConfluenceComment>,
    #[pyo3(get)]
    pub size: usize,
}

impl From<CommentsResponse> for PyConfluenceCommentsResponse {
    fn from(response: CommentsResponse) -> Self {
        Self {
            results: response.results.into_iter().map(Into::into).collect(),
            size: response.size,
        }
    }
}

/// Confluence attachment.
#[pyclass(name = "ConfluenceAttachment")]
#[derive(Clone)]
pub struct PyConfluenceAttachment {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub title: String,
}

impl From<Attachment> for PyConfluenceAttachment {
    fn from(attachment: Attachment) -> Self {
        Self {
            id: attachment.id,
            title: attachment.title,
        }
    }
}

/// Confluence attachments response.
#[pyclass(name = "ConfluenceAttachmentsResponse")]
#[derive(Clone)]
pub struct PyConfluenceAttachmentsResponse {
    #[pyo3(get)]
    pub results: Vec<PyConfluenceAttachment>,
    #[pyo3(get)]
    pub size: usize,
}

impl From<AttachmentsResponse> for PyConfluenceAttachmentsResponse {
    fn from(response: AttachmentsResponse) -> Self {
        Self {
            results: response.results.into_iter().map(Into::into).collect(),
            size: response.size,
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
        let inner =
            ConfluenceClient::from_config(base_url, consumer_key, private_key, access_token, access_secret)
                .map_err(confluence_error_to_py)?;
        Ok(Self { inner })
    }

    /// Get the base URL.
    #[getter]
    pub fn base_url(&self) -> &str {
        self.inner.base_url()
    }

    /// Create a new page in a space.
    ///
    /// Args:
    ///     space_key: Space key
    ///     title: Page title
    ///     body: Page body in Confluence storage format
    ///     parent_id: Optional parent page ID
    ///
    /// Returns:
    ///     Created page
    #[pyo3(signature = (space_key, title, body, parent_id=None))]
    pub fn create_page(
        &self,
        py: Python<'_>,
        space_key: &str,
        title: &str,
        body: &str,
        parent_id: Option<&str>,
    ) -> PyResult<PyConfluencePage> {
        py.allow_threads(|| {
            self.inner
                .create_page(space_key, title, body, parent_id)
                .map(Into::into)
                .map_err(confluence_error_to_py)
        })
    }

    /// Get page by ID.
    ///
    /// Args:
    ///     page_id: Page ID
    ///     expand: Optional list of fields to expand
    ///
    /// Returns:
    ///     Page information
    #[pyo3(signature = (page_id, expand=None))]
    pub fn get_page(
        &self,
        py: Python<'_>,
        page_id: &str,
        expand: Option<Vec<String>>,
    ) -> PyResult<PyConfluencePage> {
        let expand_refs: Vec<&str> = expand
            .as_ref()
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();

        py.allow_threads(|| {
            self.inner
                .get_page(page_id, &expand_refs)
                .map(Into::into)
                .map_err(confluence_error_to_py)
        })
    }

    /// Update an existing page.
    ///
    /// Args:
    ///     page_id: Page ID
    ///     title: Page title
    ///     body: Page body in Confluence storage format
    ///     version: Current version number
    ///     message: Optional version message
    ///
    /// Returns:
    ///     Updated page
    #[pyo3(signature = (page_id, title, body, version, message=None))]
    pub fn update_page(
        &self,
        py: Python<'_>,
        page_id: &str,
        title: &str,
        body: &str,
        version: u32,
        message: Option<&str>,
    ) -> PyResult<PyConfluencePage> {
        py.allow_threads(|| {
            self.inner
                .update_page(page_id, title, body, version, message)
                .map(Into::into)
                .map_err(confluence_error_to_py)
        })
    }

    /// Get web URL for page.
    ///
    /// Args:
    ///     page_id: Page ID
    ///
    /// Returns:
    ///     Web URL for the page
    pub fn get_page_url(&self, py: Python<'_>, page_id: &str) -> PyResult<String> {
        py.allow_threads(|| {
            self.inner
                .get_page_url(page_id)
                .map_err(confluence_error_to_py)
        })
    }

    /// Get all comments on a page.
    ///
    /// Args:
    ///     page_id: Page ID
    ///
    /// Returns:
    ///     Comments response
    pub fn get_comments(
        &self,
        py: Python<'_>,
        page_id: &str,
    ) -> PyResult<PyConfluenceCommentsResponse> {
        py.allow_threads(|| {
            self.inner
                .get_comments(page_id)
                .map(Into::into)
                .map_err(confluence_error_to_py)
        })
    }

    /// Get inline comments with marker refs.
    ///
    /// Args:
    ///     page_id: Page ID
    ///
    /// Returns:
    ///     Comments response with inline properties
    pub fn get_inline_comments(
        &self,
        py: Python<'_>,
        page_id: &str,
    ) -> PyResult<PyConfluenceCommentsResponse> {
        py.allow_threads(|| {
            self.inner
                .get_inline_comments(page_id)
                .map(Into::into)
                .map_err(confluence_error_to_py)
        })
    }

    /// Get footer (page-level) comments.
    ///
    /// Args:
    ///     page_id: Page ID
    ///
    /// Returns:
    ///     Comments response
    pub fn get_footer_comments(
        &self,
        py: Python<'_>,
        page_id: &str,
    ) -> PyResult<PyConfluenceCommentsResponse> {
        py.allow_threads(|| {
            self.inner
                .get_footer_comments(page_id)
                .map(Into::into)
                .map_err(confluence_error_to_py)
        })
    }

    /// Upload or update attachment.
    ///
    /// Args:
    ///     page_id: Page ID
    ///     filename: Attachment filename
    ///     data: File content bytes
    ///     content_type: MIME content type
    ///     comment: Optional comment
    ///
    /// Returns:
    ///     Uploaded attachment
    #[pyo3(signature = (page_id, filename, data, content_type, comment=None))]
    pub fn upload_attachment(
        &self,
        py: Python<'_>,
        page_id: &str,
        filename: &str,
        data: &[u8],
        content_type: &str,
        comment: Option<&str>,
    ) -> PyResult<PyConfluenceAttachment> {
        py.allow_threads(|| {
            self.inner
                .upload_attachment(page_id, filename, data, content_type, comment)
                .map(Into::into)
                .map_err(confluence_error_to_py)
        })
    }

    /// List attachments on a page.
    ///
    /// Args:
    ///     page_id: Page ID
    ///
    /// Returns:
    ///     Attachments response
    pub fn get_attachments(
        &self,
        py: Python<'_>,
        page_id: &str,
    ) -> PyResult<PyConfluenceAttachmentsResponse> {
        py.allow_threads(|| {
            self.inner
                .get_attachments(page_id)
                .map(Into::into)
                .map_err(confluence_error_to_py)
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

/// Python module definition.
#[pymodule]
#[pyo3(name = "_docstage_core")]
pub fn docstage_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Converter classes
    m.add_class::<PyConvertResult>()?;
    m.add_class::<PyHtmlConvertResult>()?;
    m.add_class::<PyMarkdownConverter>()?;
    m.add_class::<PyTocEntry>()?;

    // PageRenderer classes
    m.add_class::<PyPageRenderResult>()?;
    m.add_class::<PyPageRendererConfig>()?;
    m.add_class::<PyPageRenderer>()?;

    // Site classes
    m.add_class::<PyPage>()?;
    m.add_class::<PyBreadcrumbItem>()?;
    m.add_class::<PySite>()?;
    m.add_class::<PySiteLoaderConfig>()?;
    m.add_class::<PySiteLoader>()?;
    m.add_class::<PyNavItem>()?;
    m.add_function(wrap_pyfunction!(py_build_navigation, m)?)?;

    // Config classes
    m.add_class::<PyConfig>()?;
    m.add_class::<PyCliSettings>()?;
    m.add_class::<PyServerConfig>()?;
    m.add_class::<PyDocsConfig>()?;
    m.add_class::<PyDiagramsConfig>()?;
    m.add_class::<PyLiveReloadConfig>()?;
    m.add_class::<PyConfluenceConfig>()?;
    m.add_class::<PyConfluenceTestConfig>()?;

    // HTTP Server classes
    m.add_class::<PyHttpServerConfig>()?;
    m.add_function(wrap_pyfunction!(py_run_http_server, m)?)?;

    // Comment Preservation classes
    m.add_class::<PyUnmatchedComment>()?;
    m.add_class::<PyPreserveResult>()?;
    m.add_function(wrap_pyfunction!(py_preserve_comments, m)?)?;

    // Confluence Client classes
    m.add_class::<PyConfluencePage>()?;
    m.add_class::<PyConfluenceComment>()?;
    m.add_class::<PyConfluenceCommentsResponse>()?;
    m.add_class::<PyConfluenceAttachment>()?;
    m.add_class::<PyConfluenceAttachmentsResponse>()?;
    m.add_class::<PyConfluenceClient>()?;
    m.add_function(wrap_pyfunction!(py_read_private_key, m)?)?;

    Ok(())
}
