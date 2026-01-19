"""High-performance markdown renderer for Docstage.

This module provides Python bindings to the Rust pulldown-cmark parser
with custom renderers for Confluence and HTML5 formats.
"""

from ._docstage_core import (
    ConvertResult,
    DiagramCache,
    DiagramInfo,
    ExtractResult,
    HtmlConvertResult,
    MarkdownConverter,
    PreparedDiagram,
    TocEntry,
)

__all__ = [
    "ConvertResult",
    "DiagramCache",
    "DiagramInfo",
    "ExtractResult",
    "HtmlConvertResult",
    "MarkdownConverter",
    "PreparedDiagram",
    "TocEntry",
]
