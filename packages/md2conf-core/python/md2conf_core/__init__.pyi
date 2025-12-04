"""Type stubs for md2conf_core."""


class RenderedDiagram:
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
    def diagrams(self) -> list[RenderedDiagram]:
        """Rendered diagrams (empty if kroki_url/output_dir not provided)."""
        ...


class MarkdownConverter:
    """Markdown to Confluence converter."""

    def __init__(
        self,
        gfm: bool = True,
        prepend_toc: bool = False,
        extract_title: bool = False,
        include_dirs: list[object] | None = None,
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

    def convert(
        self,
        markdown_text: str,
        kroki_url: str | None = None,
        output_dir: object | None = None,
    ) -> ConvertResult:
        """Convert markdown to Confluence storage format.

        If kroki_url and output_dir are provided, PlantUML diagrams will be rendered
        via Kroki and placeholders replaced with Confluence image macros.

        Args:
            markdown_text: Markdown source text
            kroki_url: Optional Kroki server URL (e.g., "https://kroki.io")
            output_dir: Optional directory to write rendered PNG files (Path or str)

        Returns:
            ConvertResult with HTML, title, and rendered diagrams (empty if no kroki_url)

        Raises:
            RuntimeError: If diagram rendering fails
        """
        ...
