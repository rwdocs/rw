"""High-performance markdown renderer for Docstage.

This module provides Python bindings to the Rust pulldown-cmark parser
with custom renderers for Confluence and HTML5 formats.
"""

from ._docstage_core import (
    ConvertResult,
    HtmlConvertResult,
    MarkdownConverter,
    TocEntry,
)

__all__ = [
    "ConvertResult",
    "HtmlConvertResult",
    "MarkdownConverter",
    "TocEntry",
]
