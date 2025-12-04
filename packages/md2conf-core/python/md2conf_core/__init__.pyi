"""Type stubs for md2conf_core."""

from pathlib import Path


class DiagramInfo:
    """Information about an extracted PlantUML diagram."""

    @property
    def source(self) -> str:
        """Resolved source (includes resolved, DPI and config prepended)."""
        ...

    @property
    def index(self) -> int:
        """Zero-based index of this diagram."""
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
        """PlantUML diagrams extracted from code blocks (with resolved sources)."""
        ...


class MarkdownConverter:
    """Markdown to Confluence converter."""

    def __init__(
        self,
        gfm: bool = True,
        prepend_toc: bool = False,
        extract_title: bool = False,
        include_dirs: list[Path] | None = None,
        config_file: str | None = None,
    ) -> None:
        """Initialize converter.

        Args:
            gfm: Enable GitHub Flavored Markdown (tables, strikethrough, etc.)
            prepend_toc: Whether to prepend a table of contents macro
            extract_title: Whether to extract title from first H1 and level up headers
            include_dirs: Directories to search for PlantUML includes
            config_file: PlantUML config filename to load and prepend to diagrams
        """
        ...

    def convert(self, markdown_text: str) -> ConvertResult:
        """Convert markdown to Confluence storage format.

        Args:
            markdown_text: Markdown source text

        Returns:
            ConvertResult with HTML, title, and diagrams (with resolved sources)
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


class RenderedDiagram:
    """Result of rendering a single diagram."""

    @property
    def index(self) -> int:
        """Zero-based index of the diagram."""
        ...

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


def render_diagrams(
    diagrams: list[DiagramInfo],
    server_url: str,
    output_dir: Path,
    pool_size: int = 4,
) -> list[RenderedDiagram]:
    """Render diagrams in parallel using Kroki service.

    Args:
        diagrams: List of DiagramInfo objects from convert()
        server_url: Kroki server URL (e.g., "https://kroki.io")
        output_dir: Directory to save rendered PNG files
        pool_size: Number of parallel threads (default: 4)

    Returns:
        List of RenderedDiagram with index, filename, width, height

    Raises:
        RuntimeError: If rendering fails
    """
    ...
