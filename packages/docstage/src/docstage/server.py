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
    static_dir: Path | None


async def spa_fallback(request: web.Request) -> web.FileResponse:
    """Serve index.html for SPA client-side routing.

    All non-API routes fall back to index.html to support client-side routing.
    """
    static_dir = request.app.get("static_dir")
    if static_dir is None:
        raise web.HTTPNotFound()

    index_path = static_dir / "index.html"
    if not index_path.exists():
        raise web.HTTPNotFound()

    return web.FileResponse(index_path)


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

    # API routes (must be registered first to take precedence over SPA fallback)
    app.router.add_routes(create_pages_routes())
    app.router.add_routes(create_navigation_routes())

    # Static file serving for frontend
    static_dir = config.get("static_dir")
    if static_dir is not None and static_dir.exists():
        app["static_dir"] = static_dir

        # Serve static assets from /assets directory
        assets_dir = static_dir / "assets"
        if assets_dir.exists():
            app.router.add_static("/assets", assets_dir)

        # Serve other static files (favicon, etc.) from root
        app.router.add_get("/favicon.png", _serve_favicon)

        # SPA fallback - must be last to catch all non-API routes
        app.router.add_get("/{path:.*}", spa_fallback)

    return app


async def _serve_favicon(request: web.Request) -> web.FileResponse:
    """Serve favicon from static directory."""
    static_dir = request.app.get("static_dir")
    if static_dir is None:
        raise web.HTTPNotFound()

    favicon_path = static_dir / "favicon.png"
    if not favicon_path.exists():
        raise web.HTTPNotFound()

    return web.FileResponse(favicon_path)


def run_server(config: ServerConfig) -> None:
    """Run the server.

    Args:
        config: Server configuration
    """
    app = create_app(config)
    web.run_app(app, host=config["host"], port=config["port"])
