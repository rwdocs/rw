"""Python bindings for Docstage Confluence CLI.

This module provides Python bindings for the Confluence update workflow
and HTTP server. The heavy lifting is done in Rust via PyO3.
"""

from ._docstage_core import (
    ConfluenceClient,
    ConfluencePage,
    DryRunResult,
    HttpServerConfig,
    UnmatchedComment,
    UpdateResult,
    read_private_key,
    run_http_server,
)

__all__ = [
    "ConfluenceClient",
    "ConfluencePage",
    "DryRunResult",
    "HttpServerConfig",
    "UnmatchedComment",
    "UpdateResult",
    "read_private_key",
    "run_http_server",
]
