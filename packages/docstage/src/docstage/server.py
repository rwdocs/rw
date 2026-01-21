"""Native Rust HTTP server for Docstage.

This module provides the entry point for running the native Rust HTTP server
via PyO3 bindings. The server is implemented entirely in Rust (docstage-server
crate) and called from Python for backwards compatibility during the transition.

Static files are served from:
- Development: `frontend/dist` directory (relative to cwd)
- Production: Embedded in the Rust binary (when built with `embed-assets` feature)
"""

from docstage_core import HttpServerConfig, run_http_server
from docstage_core.config import Config

from docstage import __version__


def run_server(config: Config, *, verbose: bool = False) -> None:
    """Run the native Rust HTTP server.

    Args:
        config: Application configuration
        verbose: Enable verbose output (show diagram warnings)
    """
    server_config = HttpServerConfig.from_config(config, __version__, verbose)
    run_http_server(server_config)
