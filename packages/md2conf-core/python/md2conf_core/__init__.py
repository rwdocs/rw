"""High-performance markdown to Confluence converter.

This module provides Python bindings to the Rust pulldown-cmark parser
with a custom Confluence storage format renderer.
"""

from .md2conf_core import (
    ConvertResult,
    DiagramInfo,
    MarkdownConverter,
    create_image_tag,
    prepare_diagram_source,
)

__all__ = [
    "ConvertResult",
    "DiagramInfo",
    "MarkdownConverter",
    "create_image_tag",
    "prepare_diagram_source",
]
