"""Markdown to Confluence storage format converter.

This module converts Markdown text to Confluence XHTML storage format.
Uses md2cf's ConfluenceRenderer for conversion.
"""

import logging
from pathlib import Path

import mistune
from md2cf.confluence_renderer import ConfluenceRenderer

logger = logging.getLogger(__name__)


class MarkdownConverter:
    """Convert Markdown to Confluence storage format."""

    def __init__(self) -> None:
        """Initialize the converter with Confluence renderer."""
        self.renderer = ConfluenceRenderer(use_xhtml=True)
        self.markdown = mistune.Markdown(renderer=self.renderer)

    def convert(self, markdown_text: str) -> str:
        """Convert Markdown text to Confluence storage format.

        Args:
            markdown_text: Markdown source text

        Returns:
            Confluence storage format (XHTML) string
        """
        logger.debug(f'Converting {len(markdown_text)} characters of markdown')
        confluence_body = self.markdown(markdown_text)
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
