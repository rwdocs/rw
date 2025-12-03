"""Markdown to Confluence storage format converter.

This module converts Markdown text to Confluence XHTML storage format.
Uses md2conf-core's Rust-based renderer for high-performance conversion.
"""

import logging
from pathlib import Path

from md2conf_core import ConvertResult, MarkdownConverter as CoreConverter

logger = logging.getLogger(__name__)


class MarkdownConverter:
    """Convert Markdown to Confluence storage format."""

    def __init__(self, prepend_toc: bool = False, extract_title: bool = False) -> None:
        """Initialize the converter.

        Args:
            prepend_toc: Whether to prepend a table of contents macro
            extract_title: Whether to extract title from first H1 and level up headers
        """
        self._converter = CoreConverter(prepend_toc=prepend_toc, extract_title=extract_title)

    def convert(self, markdown_text: str) -> ConvertResult:
        """Convert Markdown text to Confluence storage format.

        Args:
            markdown_text: Markdown source text

        Returns:
            ConvertResult with HTML and optional title
        """
        logger.debug(f'Converting {len(markdown_text)} characters of markdown')
        result = self._converter.convert(markdown_text)
        logger.debug(f'Converted to {len(result.html)} characters of XHTML')
        return result

    def convert_file(self, file_path: str | Path) -> ConvertResult:
        """Convert a Markdown file to Confluence storage format.

        Args:
            file_path: Path to Markdown file

        Returns:
            ConvertResult with HTML and optional title

        Raises:
            FileNotFoundError: If file doesn't exist
        """
        path = Path(file_path)
        if not path.exists():
            raise FileNotFoundError(f'Markdown file not found: {file_path}')

        logger.info(f'Converting file: {file_path}')
        markdown_text = path.read_text(encoding='utf-8')
        return self.convert(markdown_text)
