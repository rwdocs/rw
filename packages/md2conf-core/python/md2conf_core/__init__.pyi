"""Type stubs for md2conf_core."""

class DiagramInfo:
    """Information about an extracted PlantUML diagram."""

    @property
    def source(self) -> str:
        """Original source code from markdown."""
        ...

    @property
    def resolved_source(self) -> str:
        """Source with includes resolved and config prepended."""
        ...

    @property
    def index(self) -> int:
        """Zero-based index of this diagram."""
        ...

class ProcessedDocument:
    """Result of processing a document with PlantUML extraction."""

    @property
    def markdown(self) -> str:
        """Markdown with diagrams replaced by placeholders."""
        ...

    @property
    def diagrams(self) -> list[DiagramInfo]:
        """Extracted diagrams."""
        ...

    @property
    def title(self) -> str | None:
        """Title extracted from first H1 heading."""
        ...

class MkDocsProcessor:
    """MkDocs document processor with PlantUML support."""

    def __init__(
        self,
        include_dirs: list[str],
        config_file: str | None = None,
        dpi: int = 192,
    ) -> None:
        """Initialize processor.

        Args:
            include_dirs: List of directories to search for includes
            config_file: Optional PlantUML config file to prepend to diagrams
            dpi: DPI for PNG output (default 192)
        """
        ...

    def extract_diagrams(self, markdown: str) -> ProcessedDocument:
        """Extract PlantUML diagrams and title from markdown.

        Args:
            markdown: Markdown content

        Returns:
            ProcessedDocument with diagrams extracted and placeholders inserted
        """
        ...

class MarkdownConverter:
    """Markdown to Confluence converter."""

    def __init__(self, gfm: bool = True) -> None:
        """Initialize converter.

        Args:
            gfm: Enable GitHub Flavored Markdown (tables, strikethrough, etc.)
        """
        ...

    def convert(self, markdown_text: str) -> str:
        """Convert markdown to Confluence storage format.

        Args:
            markdown_text: Markdown source text

        Returns:
            Confluence XHTML storage format string
        """
        ...

def markdown_to_confluence(markdown: str, gfm: bool = True) -> str:
    """Convert markdown to Confluence storage format.

    Args:
        markdown: Markdown source text
        gfm: Enable GitHub Flavored Markdown (tables, strikethrough, etc.)

    Returns:
        Confluence XHTML storage format string
    """
    ...

def create_image_tag(filename: str, width: int | None = None) -> str:
    """Create Confluence image macro for an attachment.

    Args:
        filename: Attachment filename
        width: Optional width in pixels

    Returns:
        Confluence storage format image macro
    """
    ...

def toc_macro() -> str:
    """Get Confluence TOC macro.

    Returns:
        Confluence TOC macro string
    """
    ...
