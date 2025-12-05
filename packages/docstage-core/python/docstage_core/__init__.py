"""High-performance markdown renderer for Docstage.

This module provides Python bindings to the Rust pulldown-cmark parser
with a custom Confluence storage format renderer.
"""

from .docstage_core import (
    ConvertResult,
    DiagramInfo,
    MarkdownConverter,
)

__all__ = [
    "ConvertResult",
    "DiagramInfo",
    "MarkdownConverter",
]
