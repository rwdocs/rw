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
    verbose: bool
    """Enable verbose output."""
    version: str
    """Application version (for cache invalidation)."""

    @staticmethod
    def from_config(
        config: Config,
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

# Comment Preservation classes

class UnmatchedComment:
    """Comment that could not be placed in new HTML."""

    ref_id: str
    """Comment reference ID."""
    text: str
    """Text content the marker was wrapping."""

class PreserveResult:
    """Result of comment preservation operation."""

    html: str
    """HTML with preserved comment markers."""
    unmatched_comments: list[UnmatchedComment]
    """Comments that could not be placed in the new HTML."""

def preserve_comments(old_html: str, new_html: str) -> PreserveResult:
    """Preserve inline comment markers from old HTML in new HTML.

    This function transfers comment markers from the old Confluence page HTML
    to the new HTML generated from markdown conversion. It uses tree-based
    comparison to match content and transfer markers to matching positions.

    Args:
        old_html: Current page HTML with comment markers
        new_html: New HTML from markdown conversion

    Returns:
        PreserveResult with HTML containing preserved markers and any unmatched comments
    """
    ...

# Confluence Client classes

class ConfluencePage:
    """Confluence page."""

    id: str
    """Page ID."""
    title: str
    """Page title."""
    version: int
    """Version number."""
    body: str | None
    """Page body in storage format (if expanded)."""

class ConfluenceComment:
    """Confluence comment."""

    id: str
    """Comment ID."""
    title: str
    """Comment title."""
    body: str | None
    """Comment body."""
    marker_ref: str | None
    """Inline comment marker reference."""
    original_selection: str | None
    """Original selected text for inline comments."""
    status: str | None
    """Resolution status ("open" or "resolved")."""

class ConfluenceCommentsResponse:
    """Response from comments API."""

    results: list[ConfluenceComment]
    """List of comments."""
    size: int
    """Total count."""

class ConfluenceAttachment:
    """Confluence attachment."""

    id: str
    """Attachment ID."""
    title: str
    """Attachment filename."""

class ConfluenceAttachmentsResponse:
    """Response from attachments API."""

    results: list[ConfluenceAttachment]
    """List of attachments."""
    size: int
    """Total count."""

class ConfluenceClient:
    """Confluence REST API client with OAuth 1.0 RSA-SHA1 authentication."""

    base_url: str
    """Confluence server base URL."""

    def __init__(
        self,
        base_url: str,
        consumer_key: str,
        private_key: bytes,
        access_token: str,
        access_secret: str,
    ) -> None:
        """Create a new Confluence client.

        Args:
            base_url: Confluence server base URL
            consumer_key: OAuth consumer key
            private_key: PEM-encoded RSA private key bytes
            access_token: OAuth access token
            access_secret: OAuth access token secret
        """
        ...

    def create_page(
        self,
        space_key: str,
        title: str,
        body: str,
        parent_id: str | None = None,
    ) -> ConfluencePage:
        """Create a new page in a space.

        Args:
            space_key: Space key
            title: Page title
            body: Page body in Confluence storage format
            parent_id: Optional parent page ID

        Returns:
            Created page
        """
        ...

    def get_page(
        self,
        page_id: str,
        expand: list[str] | None = None,
    ) -> ConfluencePage:
        """Get page by ID.

        Args:
            page_id: Page ID
            expand: Optional list of fields to expand

        Returns:
            Page information
        """
        ...

    def update_page(
        self,
        page_id: str,
        title: str,
        body: str,
        version: int,
        message: str | None = None,
    ) -> ConfluencePage:
        """Update an existing page.

        Args:
            page_id: Page ID
            title: Page title
            body: Page body in Confluence storage format
            version: Current version number
            message: Optional version message

        Returns:
            Updated page
        """
        ...

    def get_page_url(self, page_id: str) -> str:
        """Get web URL for page.

        Args:
            page_id: Page ID

        Returns:
            Web URL for the page
        """
        ...

    def get_comments(self, page_id: str) -> ConfluenceCommentsResponse:
        """Get all comments on a page.

        Args:
            page_id: Page ID

        Returns:
            Comments response
        """
        ...

    def get_inline_comments(self, page_id: str) -> ConfluenceCommentsResponse:
        """Get inline comments with marker refs.

        Args:
            page_id: Page ID

        Returns:
            Comments response with inline properties
        """
        ...

    def get_footer_comments(self, page_id: str) -> ConfluenceCommentsResponse:
        """Get footer (page-level) comments.

        Args:
            page_id: Page ID

        Returns:
            Comments response
        """
        ...

    def upload_attachment(
        self,
        page_id: str,
        filename: str,
        data: bytes,
        content_type: str,
        comment: str | None = None,
    ) -> ConfluenceAttachment:
        """Upload or update attachment.

        Args:
            page_id: Page ID
            filename: Attachment filename
            data: File content bytes
            content_type: MIME content type
            comment: Optional comment

        Returns:
            Uploaded attachment
        """
        ...

    def get_attachments(self, page_id: str) -> ConfluenceAttachmentsResponse:
        """List attachments on a page.

        Args:
            page_id: Page ID

        Returns:
            Attachments response
        """
        ...

def read_private_key(path: Path) -> bytes:
    """Read RSA private key from PEM file.

    Args:
        path: Path to PEM file

    Returns:
        PEM-encoded key bytes

    Raises:
        FileNotFoundError: If file not found
        RuntimeError: If key cannot be parsed
    """
    ...
