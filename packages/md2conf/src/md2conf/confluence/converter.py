"""Markdown to Confluence storage format converter.

This module converts Markdown text to Confluence XHTML storage format.
Uses md2conf-core's Rust-based renderer for high-performance conversion.
"""

import logging
from pathlib import Path

from md2conf_core import MarkdownConverter as CoreConverter

logger = logging.getLogger(__name__)


class MarkdownConverter:
    """Convert Markdown to Confluence storage format."""

    def __init__(self) -> None:
        """Initialize the converter."""
        self._converter = CoreConverter()

    def convert(self, markdown_text: str) -> str:
        """Convert Markdown text to Confluence storage format.

        Args:
            markdown_text: Markdown source text

        Returns:
            Confluence storage format (XHTML) string
        """
        logger.debug(f'Converting {len(markdown_text)} characters of markdown')
        confluence_body = self._converter.convert(markdown_text)
        logger.debug(f'Converted to {len(confluence_body)} characters of XHTML')
        return confluence_body

    def convert_file(self, file_path: str | Path) -> str:
        """Convert a Markdown file to Confluence storage format.

        Args:
            file_path: Path to Markdown file

        Returns:
            Confluence storage format (XHTML) string

        Raises:
            FileNotFoundError: If file doesn't exist
        """
        path = Path(file_path)
        if not path.exists():
            raise FileNotFoundError(f'Markdown file not found: {file_path}')

        logger.info(f'Converting file: {file_path}')
        markdown_text = path.read_text(encoding='utf-8')
        return self.convert(markdown_text)
