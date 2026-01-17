"""High-performance markdown renderer for Docstage.

This module provides Python bindings to the Rust pulldown-cmark parser
with custom renderers for Confluence and HTML5 formats.
"""

from .docstage_core import (
    Config,
    ConfluenceConfig,
    ConfluenceTestConfig,
    ConvertResult,
    DiagramInfo,
    DiagramsConfig,
    DocsConfig,
    HtmlConvertResult,
    LiveReloadConfig,
    MarkdownConverter,
    ServerConfig,
    TocEntry,
)

__all__ = [
    "Config",
    "ConfluenceConfig",
    "ConfluenceTestConfig",
    "ConvertResult",
    "DiagramInfo",
    "DiagramsConfig",
    "DocsConfig",
    "HtmlConvertResult",
    "LiveReloadConfig",
    "MarkdownConverter",
    "ServerConfig",
    "TocEntry",
]
