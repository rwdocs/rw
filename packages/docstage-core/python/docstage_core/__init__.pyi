"""Type stubs for docstage_core."""

from pathlib import Path

# Config classes

class ServerConfig:
    """Server configuration."""

    @property
    def host(self) -> str:
        """Server host address."""
        ...

    @property
    def port(self) -> int:
        """Server port."""
        ...

class DocsConfig:
    """Documentation configuration."""

    @property
    def source_dir(self) -> Path:
        """Source directory for markdown files."""
        ...

    @property
    def cache_dir(self) -> Path:
        """Cache directory for rendered pages."""
        ...

    @property
    def cache_enabled(self) -> bool:
        """Whether caching is enabled."""
        ...

class DiagramsConfig:
    """Diagram rendering configuration."""

    @property
    def kroki_url(self) -> str | None:
        """Kroki server URL."""
        ...

    @property
    def include_dirs(self) -> list[Path]:
        """Directories to search for PlantUML includes."""
        ...

    @property
    def config_file(self) -> str | None:
        """PlantUML config filename."""
        ...

    @property
    def dpi(self) -> int:
        """DPI for diagram rendering."""
        ...

class LiveReloadConfig:
    """Live reload configuration."""

    @property
    def enabled(self) -> bool:
        """Whether live reload is enabled."""
        ...

    @property
    def watch_patterns(self) -> list[str] | None:
        """File patterns to watch for changes."""
        ...

class ConfluenceTestConfig:
    """Confluence test configuration."""

    @property
    def space_key(self) -> str:
        """Space key for testing."""
        ...

class ConfluenceConfig:
    """Confluence configuration."""

    @property
    def base_url(self) -> str:
        """Confluence server base URL."""
        ...

    @property
    def access_token(self) -> str:
        """OAuth access token."""
        ...

    @property
    def access_secret(self) -> str:
        """OAuth access token secret."""
        ...

    @property
    def consumer_key(self) -> str:
        """OAuth consumer key."""
        ...

    @property
    def test(self) -> ConfluenceTestConfig | None:
        """Test configuration."""
        ...

class Config:
    """Application configuration."""

    @staticmethod
    def load(config_path: Path | None = None) -> Config:
        """Load configuration from file.

        If config_path is provided, loads from that file.
        Otherwise, searches for docstage.toml in current directory and parents.

        Args:
            config_path: Optional explicit path to config file

        Returns:
            Config instance with defaults for missing sections

        Raises:
            FileNotFoundError: If explicit config_path doesn't exist
            ValueError: If configuration is invalid
        """
        ...

    @property
    def server(self) -> ServerConfig:
        """Server configuration."""
        ...

    @property
    def docs(self) -> DocsConfig:
        """Documentation configuration."""
        ...

    @property
    def diagrams(self) -> DiagramsConfig:
        """Diagram rendering configuration."""
        ...

    @property
    def live_reload(self) -> LiveReloadConfig:
        """Live reload configuration."""
        ...

    @property
    def confluence(self) -> ConfluenceConfig | None:
        """Confluence configuration (None if not configured)."""
        ...

    @property
    def confluence_test(self) -> ConfluenceTestConfig | None:
        """Confluence test configuration (None if not configured)."""
        ...

    @property
    def config_path(self) -> Path | None:
        """Path to the config file (None if using defaults)."""
        ...

class DiagramInfo:
    """Rendered diagram info (file written to output_dir)."""

    @property
    def filename(self) -> str:
        """Output filename (e.g., "diagram_abc123.png")."""
        ...

    @property
    def width(self) -> int:
        """Image width in pixels."""
        ...

    @property
    def height(self) -> int:
        """Image height in pixels."""
        ...

class ConvertResult:
    """Result of converting markdown to Confluence format."""

    @property
    def html(self) -> str:
        """Confluence XHTML storage format."""
        ...

    @property
    def title(self) -> str | None:
        """Title extracted from first H1 heading (if extract_title was enabled)."""
        ...

    @property
    def diagrams(self) -> list[DiagramInfo]:
        """Rendered diagrams (empty if kroki_url/output_dir not provided)."""
        ...

class TocEntry:
    """Table of contents entry."""

    @property
    def level(self) -> int:
        """Heading level (1-6)."""
        ...

    @property
    def title(self) -> str:
        """Heading text."""
        ...

    @property
    def id(self) -> str:
        """Anchor ID for linking."""
        ...

class HtmlConvertResult:
    """Result of converting markdown to HTML format."""

    @property
    def html(self) -> str:
        """Rendered HTML content."""
        ...

    @property
    def title(self) -> str | None:
        """Title extracted from first H1 heading (if extract_title was enabled)."""
        ...

    @property
    def toc(self) -> list[TocEntry]:
        """Table of contents entries."""
        ...

    @property
    def warnings(self) -> list[str]:
        """Warnings generated during conversion (e.g., unresolved includes)."""
        ...

class PreparedDiagram:
    """A prepared diagram ready for rendering via Kroki."""

    @property
    def index(self) -> int:
        """Zero-based index of this diagram in the document."""
        ...

    @property
    def source(self) -> str:
        """Prepared source ready for Kroki (with !include resolved, config injected)."""
        ...

    @property
    def endpoint(self) -> str:
        """Kroki endpoint for this diagram type (e.g., "plantuml", "mermaid")."""
        ...

    @property
    def format(self) -> str:
        """Output format ("svg" or "png")."""
        ...

class ExtractResult:
    """Result of extracting diagrams from markdown."""

    @property
    def html(self) -> str:
        """HTML with diagram placeholders ({{DIAGRAM_0}}, {{DIAGRAM_1}}, etc.)."""
        ...

    @property
    def title(self) -> str | None:
        """Title extracted from first H1 heading (if extract_title was enabled)."""
        ...

    @property
    def toc(self) -> list[TocEntry]:
        """Table of contents entries."""
        ...

    @property
    def diagrams(self) -> list[PreparedDiagram]:
        """Prepared diagrams ready for rendering."""
        ...

    @property
    def warnings(self) -> list[str]:
        """Warnings generated during conversion."""
        ...

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
    ) -> None:
        """Initialize converter.

        Args:
            gfm: Enable GitHub Flavored Markdown (tables, strikethrough, etc.)
            prepend_toc: Whether to prepend a table of contents macro
            extract_title: Whether to extract title from first H1 and level up headers
            include_dirs: Directories to search for PlantUML includes
            config_file: PlantUML config filename to load and prepend to diagrams
            dpi: DPI for PlantUML rendering (default: 192 for retina displays)
        """
        ...

    def convert(
        self,
        markdown_text: str,
        kroki_url: str,
        output_dir: object,
    ) -> ConvertResult:
        """Convert markdown to Confluence storage format.

        PlantUML diagrams are rendered via Kroki and placeholders replaced with
        Confluence image macros.

        Args:
            markdown_text: Markdown source text
            kroki_url: Kroki server URL (e.g., "https://kroki.io")
            output_dir: Directory to write rendered PNG files (Path or str)

        Returns:
            ConvertResult with HTML, title, and rendered diagrams

        Raises:
            RuntimeError: If diagram rendering fails
        """
        ...

    def convert_html(
        self,
        markdown_text: str,
        base_path: str | None = None,
    ) -> HtmlConvertResult:
        """Convert markdown to HTML format.

        Produces semantic HTML5 with syntax highlighting and table of contents.
        Diagram code blocks are rendered as syntax-highlighted code.
        For rendered diagram images, use `convert_html_with_diagrams()`.

        Args:
            markdown_text: Markdown source text
            base_path: Optional base path for resolving relative links (e.g., "domains/billing/guide").
                       When provided, relative `.md` links are transformed to absolute `/docs/...` paths.

        Returns:
            HtmlConvertResult with HTML, title, and table of contents
        """
        ...

    def convert_html_with_diagrams(
        self,
        markdown_text: str,
        kroki_url: str,
        base_path: str | None = None,
    ) -> HtmlConvertResult:
        """Convert markdown to HTML format with rendered diagrams.

        Produces semantic HTML5 with diagram code blocks rendered as images via Kroki.
        Supports PlantUML, Mermaid, GraphViz, and other Kroki-supported diagram types.

        Diagrams are rendered based on their format attribute:
        - `svg` (default): Inline SVG (supports links and interactivity)
        - `png`: Inline PNG as base64 data URI
        - `img`: External SVG via `<img>` tag (falls back to inline SVG)

        If diagram rendering fails, the diagram is replaced with an error message.
        This allows the page to still render even when Kroki is unavailable.

        Args:
            markdown_text: Markdown source text
            kroki_url: Kroki server URL (e.g., "https://kroki.io")
            base_path: Optional base path for resolving relative links

        Returns:
            HtmlConvertResult with HTML containing rendered diagrams or error messages
        """
        ...

    def extract_html_with_diagrams(
        self,
        markdown_text: str,
        base_path: str | None = None,
    ) -> ExtractResult:
        """Extract diagrams from markdown without rendering.

        Returns HTML with `{{DIAGRAM_N}}` placeholders and prepared diagrams.
        This method is used for diagram caching - the caller should:
        1. Check the cache for each diagram by content hash
        2. Render only uncached diagrams via Kroki
        3. Replace placeholders with rendered content

        Args:
            markdown_text: Markdown source text
            base_path: Optional base path for resolving relative links

        Returns:
            ExtractResult with HTML placeholders and prepared diagrams
        """
        ...
