"""Type stubs for the compiled _docstage_core extension module."""

from pathlib import Path

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

class ConfluenceConfig:
    """Confluence configuration."""

    base_url: str
    access_token: str
    access_secret: str
    consumer_key: str

class Config:
    """Application configuration."""

    server: ServerConfig
    docs: DocsConfig
    diagrams: DiagramsConfig
    live_reload: LiveReloadConfig
    confluence: ConfluenceConfig | None
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

# Confluence Client classes

class UnmatchedComment:
    """Comment that could not be placed in new HTML."""

    ref_id: str
    """Comment reference ID."""
    text: str
    """Text content the marker was wrapping."""

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

    def update_page_from_markdown(
        self,
        page_id: str,
        markdown_text: str,
        diagrams: DiagramsConfig,
        extract_title: bool = True,
        message: str | None = None,
    ) -> UpdateResult:
        """Update a Confluence page from markdown content.

        This method performs the entire update workflow in a single call:
        1. Converts markdown to Confluence storage format
        2. Fetches current page content
        3. Preserves inline comments from current page
        4. Uploads diagram attachments
        5. Updates the page with new content

        Args:
            page_id: Page ID to update
            markdown_text: Markdown content
            diagrams: Diagram rendering configuration
            extract_title: Whether to extract title from first H1 heading
            message: Optional version message

        Returns:
            UpdateResult with page info, URL, and comment status
        """
        ...

    def dry_run_update(
        self,
        page_id: str,
        markdown_text: str,
        diagrams: DiagramsConfig,
        extract_title: bool = True,
    ) -> DryRunResult:
        """Perform a dry-run update (no changes made).

        Returns information about what would change without
        actually updating the page or uploading attachments.

        Args:
            page_id: Page ID to check
            markdown_text: Markdown content
            diagrams: Diagram rendering configuration
            extract_title: Whether to extract title from first H1 heading

        Returns:
            DryRunResult with preview of changes
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

# Page Updater result classes

class UpdateResult:
    """Result of updating a Confluence page."""

    page: ConfluencePage
    """Updated page information."""
    url: str
    """URL to view the updated page."""
    comment_count: int
    """Total comment count after update."""
    unmatched_comments: list[UnmatchedComment]
    """Comments that could not be preserved."""
    attachments_uploaded: int
    """Number of attachments uploaded."""
    warnings: list[str]
    """Warnings from markdown conversion."""

class DryRunResult:
    """Result of dry-run update operation."""

    html: str
    """Converted HTML with preserved comments."""
    title: str | None
    """Extracted title (if any)."""
    current_title: str
    """Current page title."""
    current_version: int
    """Current page version."""
    unmatched_comments: list[UnmatchedComment]
    """Comments that would be lost."""
    attachment_count: int
    """Number of attachments that would be uploaded."""
    attachment_names: list[str]
    """Attachment filenames."""
    warnings: list[str]
    """Warnings from markdown conversion."""
