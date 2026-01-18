"""Type stubs for the compiled docstage_core extension module."""

from pathlib import Path

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
        """Initialize converter."""
        ...

    def convert(
        self,
        markdown_text: str,
        kroki_url: str,
        output_dir: object,
    ) -> ConvertResult:
        """Convert markdown to Confluence storage format."""
        ...

    def convert_html(
        self,
        markdown_text: str,
        base_path: str | None = None,
    ) -> HtmlConvertResult:
        """Convert markdown to HTML format."""
        ...

    def convert_html_with_diagrams(
        self,
        markdown_text: str,
        kroki_url: str,
        base_path: str | None = None,
    ) -> HtmlConvertResult:
        """Convert markdown to HTML format with rendered diagrams."""
        ...

    def extract_html_with_diagrams(
        self,
        markdown_text: str,
        base_path: str | None = None,
    ) -> ExtractResult:
        """Extract diagrams from markdown without rendering."""
        ...
