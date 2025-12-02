"""High-performance markdown to Confluence converter.

This module provides Python bindings to the Rust pulldown-cmark parser
with a custom Confluence storage format renderer.
"""

from .md2conf_core import (
    DiagramInfo,
    MarkdownConverter,
    MkDocsProcessor,
    ProcessedDocument,
    create_image_tag,
    markdown_to_confluence,
    toc_macro,
)

__all__ = [
    "DiagramInfo",
    "MarkdownConverter",
    "MkDocsProcessor",
    "ProcessedDocument",
    "create_image_tag",
    "markdown_to_confluence",
    "toc_macro",
]
