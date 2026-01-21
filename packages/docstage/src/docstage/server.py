"""Native Rust HTTP server for Docstage.

This module provides the entry point for running the native Rust HTTP server
via PyO3 bindings. The server is implemented entirely in Rust (docstage-server
crate) and called from Python for backwards compatibility during the transition.
"""

from docstage_core import HttpServerConfig, run_http_server
from docstage_core.config import Config

from docstage import __version__
from docstage.assets import get_static_dir


def run_server(config: Config, *, verbose: bool = False) -> None:
    """Run the native Rust HTTP server.

    Args:
        config: Application configuration
        verbose: Enable verbose output (show diagram warnings)
    """
    static_dir = get_static_dir()

    server_config = HttpServerConfig.from_config(
        config,
        static_dir,
        __version__,
        verbose,
    )

    run_http_server(server_config)
