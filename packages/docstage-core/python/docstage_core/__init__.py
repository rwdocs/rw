"""High-performance markdown renderer for Docstage.

This module provides Python bindings to the Rust pulldown-cmark parser
with custom renderers for Confluence and HTML5 formats.
"""

from .docstage_core import (
    ConvertResult,
    DiagramInfo,
    HtmlConvertResult,
    MarkdownConverter,
    TocEntry,
)

__all__ = [
    "ConvertResult",
    "DiagramInfo",
    "HtmlConvertResult",
    "MarkdownConverter",
    "TocEntry",
]
