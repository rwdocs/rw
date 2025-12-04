"""Type stubs for md2conf_core."""


class ConvertResult:
    """Result of converting markdown to Confluence format (without diagram rendering)."""

    @property
    def html(self) -> str:
        """Confluence XHTML storage format (with diagram placeholders)."""
        ...

    @property
    def title(self) -> str | None:
        """Title extracted from first H1 heading (if extract_title was enabled)."""
        ...

    @property
    def diagram_count(self) -> int:
        """Number of PlantUML diagrams found in the markdown."""
        ...


class RenderedDiagram:
    """Rendered diagram with image data."""

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

    @property
    def data(self) -> bytes:
        """PNG image data."""
        ...


class ConvertWithDiagramsResult:
    """Result of converting markdown with diagram rendering."""

    @property
    def html(self) -> str:
        """Confluence XHTML storage format (with image macros replacing placeholders)."""
        ...

    @property
    def title(self) -> str | None:
        """Title extracted from first H1 heading (if extract_title was enabled)."""
        ...

    @property
    def diagrams(self) -> list[RenderedDiagram]:
        """Rendered diagrams with image data."""
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

    def convert(self, markdown_text: str) -> ConvertResult:
        """Convert markdown to Confluence storage format.

        This method does not render diagrams. Use convert_with_diagrams() to
        convert markdown and render PlantUML diagrams via Kroki.

        Args:
            markdown_text: Markdown source text

        Returns:
            ConvertResult with HTML (containing placeholders), title, and diagram count
        """
        ...

    def convert_with_diagrams(
        self, markdown_text: str, kroki_url: str
    ) -> ConvertWithDiagramsResult:
        """Convert markdown to Confluence storage format with diagram rendering.

        This method renders PlantUML diagrams via Kroki and replaces placeholders
        with Confluence image macros.

        Args:
            markdown_text: Markdown source text
            kroki_url: Kroki server URL (e.g., "https://kroki.io")

        Returns:
            ConvertWithDiagramsResult with HTML (placeholders replaced), title,
            and rendered diagrams with image data

        Raises:
            RuntimeError: If diagram rendering fails
        """
        ...
