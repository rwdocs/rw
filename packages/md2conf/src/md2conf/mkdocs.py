"""MkDocs document processor for Confluence.

Processes markdown documents from MkDocs sites, handling PlantUML diagrams
with includes and converting them to Confluence format with image attachments.
"""

import logging
from dataclasses import dataclass
from pathlib import Path

from md2conf_core import MkDocsProcessor as CoreProcessor

logger = logging.getLogger(__name__)

DEFAULT_DPI = 192


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
            dpi: DPI for PNG output (default 192, PlantUML default is 96)
        """
        self._processor = CoreProcessor(
            [str(p) for p in include_dirs],
            config_file,
            dpi,
        )

    def extract_diagrams(self, markdown: str) -> ProcessedDocument:
        """Extract PlantUML diagrams and title from markdown.

        Args:
            markdown: Markdown content

        Returns:
            ProcessedDocument with diagrams extracted, title extracted, and placeholders inserted
        """
        result = self._processor.extract_diagrams(markdown)
        return ProcessedDocument(
            markdown=result.markdown,
            diagrams=[
                DiagramInfo(
                    source=d.source,
                    resolved_source=d.resolved_source,
                    index=d.index,
                )
                for d in result.diagrams
            ],
            title=result.title,
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
