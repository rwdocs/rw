"""Type stubs for the compiled _docstage_core extension module."""

from pathlib import Path

class ConvertResult:
    """Result of converting markdown to Confluence format."""

    html: str
    """Confluence XHTML storage format."""
    title: str | None
    """Title extracted from first H1 heading (if extract_title was enabled)."""
    warnings: list[str]
    """Warnings generated during conversion (e.g., unresolved includes)."""

class TocEntry:
    """Table of contents entry."""

    level: int
    """Heading level (1-6)."""
    title: str
    """Heading text."""
    id: str
    """Anchor ID for linking."""

class HtmlConvertResult:
    """Result of converting markdown to HTML format."""

    html: str
    """Rendered HTML content."""
    title: str | None
    """Title extracted from first H1 heading (if extract_title was enabled)."""
    toc: list[TocEntry]
    """Table of contents entries."""
    warnings: list[str]
    """Warnings generated during conversion (e.g., unresolved includes)."""

class MarkdownConverter:
    """Markdown converter with multiple output formats."""

    def __init__(
        self,
        gfm: bool = True,
        prepend_toc: bool = False,
        extract_title: bool = False,
        include_dirs: list[Path] | None = None,
        config_file: str | None = None,
        dpi: int | None = None,
    ) -> None: ...
    def convert(
        self,
        markdown_text: str,
        kroki_url: str,
        output_dir: object,
    ) -> ConvertResult: ...
    def convert_html(
        self,
        markdown_text: str,
        base_path: str | None = None,
    ) -> HtmlConvertResult: ...
    def convert_html_with_diagrams(
        self,
        markdown_text: str,
        kroki_url: str,
        base_path: str | None = None,
    ) -> HtmlConvertResult: ...
    def convert_html_with_diagrams_cached(
        self,
        markdown_text: str,
        kroki_url: str,
        cache_dir: Path | None = None,
        base_path: str | None = None,
    ) -> HtmlConvertResult: ...

# PageRenderer classes

class PageRenderResult:
    """Result of rendering a markdown page."""

    html: str
    """Rendered HTML content."""
    title: str | None
    """Title extracted from first H1 heading (if enabled)."""
    toc: list[TocEntry]
    """Table of contents entries."""
    warnings: list[str]
    """Warnings generated during conversion (e.g., unresolved includes)."""
    from_cache: bool
    """Whether result was served from cache."""

class PageRendererConfig:
    """Configuration for page renderer."""

    cache_dir: Path | None
    """Cache directory for rendered pages and metadata."""
    version: str
    """Application version for cache invalidation."""
    extract_title: bool
    """Extract title from first H1 heading."""
    kroki_url: str | None
    """Kroki URL for diagram rendering (None disables diagrams)."""
    include_dirs: list[Path]
    """Directories to search for PlantUML includes."""
    config_file: str | None
    """PlantUML config file name."""
    dpi: int
    """DPI for diagram rendering."""

    def __init__(
        self,
        cache_dir: Path | None = None,
        version: str = "",
        extract_title: bool = True,
        kroki_url: str | None = None,
        include_dirs: list[Path] | None = None,
        config_file: str | None = None,
        dpi: int = 192,
    ) -> None: ...

class PageRenderer:
    """Page renderer with file-based caching.

    Uses MarkdownConverter for actual conversion and PageCache for persistence.
    Cache invalidation is based on source file mtime and build version.
    """

    def __init__(self, config: PageRendererConfig) -> None: ...
    def render(self, source_path: Path, base_path: str) -> PageRenderResult:
        """Render a markdown page.

        Args:
            source_path: Absolute path to markdown source file
            base_path: URL path for resolving relative links (e.g., "domain-a/guide")

        Returns:
            PageRenderResult with HTML, title, ToC, and cache status

        Raises:
            FileNotFoundError: If source markdown file doesn't exist
            OSError: If file cannot be read
        """
        ...

    def invalidate(self, path: str) -> None:
        """Invalidate cache entry for a path.

        Args:
            path: Document path to invalidate
        """
        ...

# Config classes

class CliSettings:
    """CLI settings that override configuration file values."""

    host: str | None
    port: int | None
    source_dir: Path | None
    cache_dir: Path | None
    cache_enabled: bool | None
    kroki_url: str | None
    live_reload_enabled: bool | None

    def __init__(
        self,
        host: str | None = None,
        port: int | None = None,
        source_dir: Path | None = None,
        cache_dir: Path | None = None,
        cache_enabled: bool | None = None,
        kroki_url: str | None = None,
        live_reload_enabled: bool | None = None,
    ) -> None: ...

class ServerConfig:
    """Server configuration."""

    host: str
    port: int

class DocsConfig:
    """Documentation configuration."""

    source_dir: Path
    cache_dir: Path
    cache_enabled: bool

class DiagramsConfig:
    """Diagram rendering configuration."""

    kroki_url: str | None
    include_dirs: list[Path]
    config_file: str | None
    dpi: int

class LiveReloadConfig:
    """Live reload configuration."""

    enabled: bool
    watch_patterns: list[str] | None

class ConfluenceTestConfig:
    """Confluence test configuration."""

    space_key: str

class ConfluenceConfig:
    """Confluence configuration."""

    base_url: str
    access_token: str
    access_secret: str
    consumer_key: str
    test: ConfluenceTestConfig | None

# Site classes

class Page:
    """Document page data."""

    title: str
    """Page title."""
    path: str
    """URL path (e.g., "/guide")."""
    source_path: Path
    """Relative path to source file."""

class BreadcrumbItem:
    """Breadcrumb navigation item."""

    title: str
    """Display title."""
    path: str
    """Link target path."""

    def to_dict(self) -> dict[str, str]:
        """Convert to dictionary for JSON serialization."""
        ...

class Site:
    """Document site structure with efficient path lookups."""

    source_dir: Path
    """Root directory containing markdown sources."""

    def get_page(self, path: str) -> Page | None:
        """Get page by URL path."""
        ...

    def get_children(self, path: str) -> list[Page]:
        """Get children of a page."""
        ...

    def get_breadcrumbs(self, path: str) -> list[BreadcrumbItem]:
        """Build breadcrumbs for a given path."""
        ...

    def get_root_pages(self) -> list[Page]:
        """Get root-level pages."""
        ...

    def resolve_source_path(self, path: str) -> Path | None:
        """Resolve URL path to absolute source file path."""
        ...

    def get_page_by_source(self, source_path: Path) -> Page | None:
        """Get page by source file path."""
        ...

class SiteLoaderConfig:
    """Configuration for site loader."""

    source_dir: Path
    """Root directory containing markdown sources."""
    cache_dir: Path | None
    """Cache directory for site structure (None disables caching)."""

    def __init__(
        self,
        source_dir: Path,
        cache_dir: Path | None = None,
    ) -> None: ...

class SiteLoader:
    """Loads site structure from filesystem."""

    source_dir: Path
    """Root directory containing markdown sources."""

    def __init__(self, config: SiteLoaderConfig) -> None: ...
    def load(self, use_cache: bool = True) -> Site:
        """Load site structure from directory."""
        ...

    def invalidate(self) -> None:
        """Invalidate cached site."""
        ...

class NavItem:
    """Navigation item with children for UI tree."""

    title: str
    """Display title."""
    path: str
    """Link target path."""
    children: list[NavItem]
    """Child navigation items."""

    def to_dict(self) -> dict[str, object]:
        """Convert to dictionary for JSON serialization."""
        ...

def build_navigation(site: Site) -> list[NavItem]:
    """Build navigation tree from site structure."""
    ...

# Config classes

class Config:
    """Application configuration."""

    server: ServerConfig
    docs: DocsConfig
    diagrams: DiagramsConfig
    live_reload: LiveReloadConfig
    confluence: ConfluenceConfig | None
    confluence_test: ConfluenceTestConfig | None
    config_path: Path | None

    @staticmethod
    def load(
        config_path: Path | None = None,
        cli_settings: CliSettings | None = None,
    ) -> Config: ...

# HTTP Server classes

class HttpServerConfig:
    """HTTP server configuration for the native Rust server."""

    host: str
    """Host address to bind to."""
    port: int
    """Port to listen on."""
    source_dir: Path
    """Documentation source directory."""
    cache_dir: Path | None
    """Cache directory (None disables caching)."""
    kroki_url: str | None
    """Kroki URL for diagrams (None disables diagrams)."""
    include_dirs: list[Path]
    """PlantUML include directories."""
    config_file: str | None
    """PlantUML config file."""
    dpi: int
    """Diagram DPI."""
    live_reload_enabled: bool
    """Enable live reload."""
    watch_patterns: list[str] | None
    """Watch patterns for live reload."""
    static_dir: Path
    """Static files directory."""
    verbose: bool
    """Enable verbose output."""
    version: str
    """Application version (for cache invalidation)."""

    @staticmethod
    def from_config(
        config: Config,
        static_dir: Path,
        version: str,
        verbose: bool,
    ) -> HttpServerConfig:
        """Create configuration from a Config object."""
        ...

def run_http_server(config: HttpServerConfig) -> None:
    """Run the HTTP server.

    This function starts the native Rust HTTP server and blocks until it is shut down.

    Args:
        config: Server configuration

    Raises:
        RuntimeError: If the server fails to start
    """
    ...
