"""aiohttp server for Docstage.

Application factory and route registration for standalone server mode.
"""

from pathlib import Path
from typing import TypedDict

from aiohttp import web

from docstage.api.navigation import create_navigation_routes
from docstage.api.pages import create_pages_routes
from docstage.app_keys import cache_key, navigation_key, renderer_key
from docstage.core.cache import FileCache
from docstage.core.navigation import NavigationBuilder
from docstage.core.renderer import PageRenderer


class ServerConfig(TypedDict):
    """Server configuration."""

    host: str
    port: int
    source_dir: Path
    cache_dir: Path


def create_app(config: ServerConfig) -> web.Application:
    """Create aiohttp application.

    Args:
        config: Server configuration

    Returns:
        Configured aiohttp application
    """
    app = web.Application()

    cache = FileCache(config["cache_dir"])
    renderer = PageRenderer(config["source_dir"], cache)
    navigation = NavigationBuilder(config["source_dir"], cache)

    app[renderer_key] = renderer
    app[navigation_key] = navigation
    app[cache_key] = cache

    app.router.add_routes(create_pages_routes())
    app.router.add_routes(create_navigation_routes())

    return app


def run_server(config: ServerConfig) -> None:
    """Run the server.

    Args:
        config: Server configuration
    """
    app = create_app(config)
    web.run_app(app, host=config["host"], port=config["port"])
