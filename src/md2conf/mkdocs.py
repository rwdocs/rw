"""MkDocs document processor for Confluence.

Processes markdown documents from MkDocs sites, handling PlantUML diagrams
with includes and converting them to Confluence format with image attachments.
"""

import logging
import re
from dataclasses import dataclass
from pathlib import Path

logger = logging.getLogger(__name__)

PLANTUML_BLOCK_PATTERN = re.compile(
    r"```plantuml\s*\n(.*?)```",
    re.DOTALL,
)

INCLUDE_PATTERN = re.compile(r"^!include\s+(.+)$", re.MULTILINE)

H1_PATTERN = re.compile(r"^#\s+(.+)$", re.MULTILINE)

HEADER_PATTERN = re.compile(r"^(#{2,6})\s+", re.MULTILINE)


def _level_up_headers(markdown: str) -> str:
    """Level up all headers by removing one # from each.

    Args:
        markdown: Markdown content

    Returns:
        Markdown with headers leveled up (## -> #, ### -> ##, etc.)
    """

    def replace_header(match: re.Match[str]) -> str:
        hashes = match.group(1)
        return hashes[1:] + " "

    return HEADER_PATTERN.sub(replace_header, markdown)


@dataclass
class DiagramInfo:
    """Information about an extracted diagram."""

    source: str
    resolved_source: str
    index: int


@dataclass
class ProcessedDocument:
    """Result of processing an MkDocs document."""

    markdown: str
    diagrams: list[DiagramInfo]
    title: str | None


DEFAULT_DPI = 192


class MkDocsProcessor:
    """Processes MkDocs documents with PlantUML diagrams."""

    def __init__(
        self,
        include_dirs: list[Path],
        config_file: str | None = None,
        dpi: int = DEFAULT_DPI,
    ):
        """Initialize processor.

        Args:
            include_dirs: List of directories to search for includes
            config_file: Optional PlantUML config file to prepend to diagrams
            dpi: DPI for PNG output (default 200, PlantUML default is 96)
        """
        self.include_dirs = include_dirs
        self.dpi = dpi
        self.config_content: str | None = None

        if config_file:
            for include_dir in include_dirs:
                config_path = include_dir / config_file
                if config_path.exists():
                    self.config_content = config_path.read_text(encoding="utf-8")
                    logger.info(f"Loaded PlantUML config from {config_path}")
                    break

    def _resolve_include(self, include_path: str) -> str | None:
        """Resolve an include path to file content.

        Args:
            include_path: Path from !include directive

        Returns:
            File content if found, None otherwise
        """
        # Skip stdlib includes (e.g., <C4/C4_Context>)
        if include_path.startswith("<") and include_path.endswith(">"):
            return None

        # Try each include directory
        for include_dir in self.include_dirs:
            full_path = include_dir / include_path
            if full_path.exists():
                logger.debug(f"Resolved include '{include_path}' to {full_path}")
                return full_path.read_text(encoding="utf-8")

        logger.warning(f"Could not resolve include: {include_path}")
        return None

    def _resolve_includes(self, source: str, depth: int = 0) -> str:
        """Recursively resolve all includes in diagram source.

        Args:
            source: PlantUML source code
            depth: Current recursion depth (to prevent infinite loops)

        Returns:
            Source with local includes resolved
        """
        if depth > 10:
            logger.warning("Include depth exceeded, stopping resolution")
            return source

        def replace_include(match: re.Match[str]) -> str:
            include_path = match.group(1).strip()
            content = self._resolve_include(include_path)
            if content is None:
                # Keep original include (stdlib or not found)
                return match.group(0)
            # Recursively resolve includes in the included content
            resolved = self._resolve_includes(content, depth + 1)
            return resolved

        return INCLUDE_PATTERN.sub(replace_include, source)

    def extract_diagrams(self, markdown: str) -> ProcessedDocument:
        """Extract PlantUML diagrams and title from markdown.

        Args:
            markdown: Markdown content

        Returns:
            ProcessedDocument with diagrams extracted, title extracted, and placeholders inserted
        """
        diagrams: list[DiagramInfo] = []
        diagram_index = 0

        def replace_diagram(match: re.Match[str]) -> str:
            nonlocal diagram_index
            source = match.group(1)
            resolved = self._resolve_includes(source)

            # Prepend DPI setting and config if available
            dpi_setting = f"skinparam dpi {self.dpi}\n"
            if self.config_content:
                resolved = dpi_setting + self.config_content + "\n" + resolved
            else:
                resolved = dpi_setting + resolved

            diagrams.append(
                DiagramInfo(
                    source=source,
                    resolved_source=resolved,
                    index=diagram_index,
                )
            )

            placeholder = f"{{{{DIAGRAM_{diagram_index}}}}}"
            diagram_index += 1
            return placeholder

        processed_markdown = PLANTUML_BLOCK_PATTERN.sub(replace_diagram, markdown)

        # Extract title from first H1 heading
        title: str | None = None
        h1_match = H1_PATTERN.search(processed_markdown)
        if h1_match:
            title = h1_match.group(1).strip()
            # Remove the H1 line from content
            processed_markdown = H1_PATTERN.sub("", processed_markdown, count=1).lstrip()
            # Level up all remaining headers (## -> #, ### -> ##, etc.)
            processed_markdown = _level_up_headers(processed_markdown)

        return ProcessedDocument(
            markdown=processed_markdown,
            diagrams=diagrams,
            title=title,
        )

    def process_file(self, file_path: Path) -> ProcessedDocument:
        """Process an MkDocs markdown file.

        Args:
            file_path: Path to markdown file

        Returns:
            ProcessedDocument with diagrams extracted

        Raises:
            FileNotFoundError: If file doesn't exist
        """
        if not file_path.exists():
            raise FileNotFoundError(f"Markdown file not found: {file_path}")

        logger.info(f"Processing MkDocs file: {file_path}")
        markdown = file_path.read_text(encoding="utf-8")
        return self.extract_diagrams(markdown)


TOC_MACRO = '<ac:structured-macro ac:name="toc" ac:schema-version="1" />'


def create_image_tag(filename: str, width: int | None = None) -> str:
    """Create Confluence image macro for an attachment.

    Args:
        filename: Attachment filename
        width: Optional width in pixels

    Returns:
        Confluence storage format image macro
    """
    if width:
        return (
            f'<ac:image ac:width="{width}">'
            f'<ri:attachment ri:filename="{filename}" />'
            f'</ac:image>'
        )
    return f'<ac:image><ri:attachment ri:filename="{filename}" /></ac:image>'
