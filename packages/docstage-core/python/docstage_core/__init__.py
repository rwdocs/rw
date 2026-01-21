"""High-performance markdown renderer for Docstage.

This module provides Python bindings to the Rust pulldown-cmark parser
with custom renderers for Confluence and HTML5 formats. It also provides
site structure management with efficient path lookups and navigation.
"""

from ._docstage_core import (
    BreadcrumbItem,
    ConvertResult,
    HtmlConvertResult,
    HttpServerConfig,
    MarkdownConverter,
    NavItem,
    Page,
    PageRenderer,
    PageRendererConfig,
    PageRenderResult,
    PreserveResult,
    Site,
    SiteLoader,
    SiteLoaderConfig,
    TocEntry,
    UnmatchedComment,
    build_navigation,
    preserve_comments,
    run_http_server,
)

__all__ = [
    "BreadcrumbItem",
    "ConvertResult",
    "HtmlConvertResult",
    "HttpServerConfig",
    "MarkdownConverter",
    "NavItem",
    "Page",
    "PageRenderResult",
    "PageRenderer",
    "PageRendererConfig",
    "PreserveResult",
    "Site",
    "SiteLoader",
    "SiteLoaderConfig",
    "TocEntry",
    "UnmatchedComment",
    "build_navigation",
    "preserve_comments",
    "run_http_server",
]
