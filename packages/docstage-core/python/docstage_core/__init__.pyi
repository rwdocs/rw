"""Type stubs for docstage_core.

This module re-exports all types from the compiled docstage_core extension module.
"""

from .docstage_core import (
    ConvertResult as ConvertResult,
    DiagramInfo as DiagramInfo,
    ExtractResult as ExtractResult,
    HtmlConvertResult as HtmlConvertResult,
    MarkdownConverter as MarkdownConverter,
    PreparedDiagram as PreparedDiagram,
    TocEntry as TocEntry,
)

__all__ = [
    "ConvertResult",
    "DiagramInfo",
    "ExtractResult",
    "HtmlConvertResult",
    "MarkdownConverter",
    "PreparedDiagram",
    "TocEntry",
]
