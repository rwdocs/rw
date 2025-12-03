"""MkDocs document processor for Confluence.

Processes markdown documents from MkDocs sites, handling PlantUML diagrams
with includes and converting them to Confluence format with image attachments.
"""

import logging
from pathlib import Path

from md2conf_core import (
    DiagramInfo,
    MkDocsProcessor as CoreProcessor,
    ProcessedDocument,
)

__all__ = ["DiagramInfo", "MkDocsProcessor", "ProcessedDocument", "DEFAULT_DPI"]

logger = logging.getLogger(__name__)

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
            dpi: DPI for PNG output (default 192, PlantUML default is 96)
        """
        self._processor = CoreProcessor(
            [str(p) for p in include_dirs],
            config_file,
            dpi,
        )

    def process_file(self, file_path: Path) -> ProcessedDocument:
        """Process an MkDocs markdown file.

        Args:
            file_path: Path to markdown file

        Returns:
            ProcessedDocument with diagrams extracted

        Raises:
            IOError: If file cannot be read
        """
        logger.info(f"Processing MkDocs file: {file_path}")
        return self._processor.process_file(str(file_path))
