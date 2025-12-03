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

__all__ = ["DiagramInfo", "MkDocsProcessor", "ProcessedDocument"]

logger = logging.getLogger(__name__)


class MkDocsProcessor:
    """Processes MkDocs documents with PlantUML diagrams."""

    def __init__(
        self,
        include_dirs: list[Path],
        config_file: str | None = None,
    ):
        """Initialize processor.

        Args:
            include_dirs: List of directories to search for includes
            config_file: Optional PlantUML config file to prepend to diagrams
        """
        self._processor = CoreProcessor(include_dirs, config_file)

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
        return self._processor.process_file(file_path)
