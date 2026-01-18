"""Type stubs for the compiled _docstage_core extension module."""

from pathlib import Path

class DiagramInfo:
    """Rendered diagram info (file written to output_dir)."""

    filename: str
    """Output filename (e.g., "diagram_abc123.png")."""
    width: int
    """Image width in pixels."""
    height: int
    """Image height in pixels."""

class ConvertResult:
    """Result of converting markdown to Confluence format."""

    html: str
    """Confluence XHTML storage format."""
    title: str | None
    """Title extracted from first H1 heading (if extract_title was enabled)."""
    diagrams: list[DiagramInfo]
    """Rendered diagrams (empty if kroki_url/output_dir not provided)."""
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

class PreparedDiagram:
    """A prepared diagram ready for rendering via Kroki."""

    index: int
    """Zero-based index of this diagram in the document."""
    source: str
    """Prepared source ready for Kroki (with !include resolved, config injected)."""
    endpoint: str
    """Kroki endpoint for this diagram type (e.g., "plantuml", "mermaid")."""
    format: str
    """Output format ("svg" or "png")."""

class ExtractResult:
    """Result of extracting diagrams from markdown.

    Used by both HTML and Confluence output formats.
    """

    html: str
    """HTML/XHTML with diagram placeholders ({{DIAGRAM_0}}, {{DIAGRAM_1}}, etc.)."""
    title: str | None
    """Title extracted from first H1 heading (if extract_title was enabled)."""
    toc: list[TocEntry]
    """Table of contents entries."""
    diagrams: list[PreparedDiagram]
    """Prepared diagrams ready for rendering."""
    warnings: list[str]
    """Warnings generated during conversion."""

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
    def extract_html_with_diagrams(
        self,
        markdown_text: str,
        base_path: str | None = None,
    ) -> ExtractResult: ...
    def extract_confluence_with_diagrams(
        self,
        markdown_text: str,
    ) -> ExtractResult: ...

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
