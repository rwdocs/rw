"""aiohttp server for Docstage.

Application factory and route registration for standalone server mode.
"""

from pathlib import Path
from typing import TypedDict

from aiohttp import web

from docstage.api.navigation import create_navigation_routes
from docstage.api.pages import create_pages_routes
from docstage.app_keys import cache_key, navigation_key, renderer_key, verbose_key
from docstage.assets import get_static_dir
from docstage.core.cache import FileCache
from docstage.core.navigation import NavigationBuilder
from docstage.core.renderer import PageRenderer


class ServerConfig(TypedDict, total=False):
    """Server configuration."""

    host: str
    port: int
    source_dir: Path
    cache_dir: Path
    kroki_url: str | None
    include_dirs: list[Path]
    config_file: str | None
    dpi: int
    verbose: bool
    live_reload: bool
    watch_patterns: list[str] | None


async def spa_fallback(request: web.Request) -> web.FileResponse:
    """Serve index.html for SPA client-side routing.

    All non-API routes fall back to index.html to support client-side routing.
    """
    static_dir: Path = request.app["static_dir"]
    index_path = static_dir / "index.html"
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
    kroki_url = config.get("kroki_url")
    include_dirs = config.get("include_dirs", [])
    config_file = config.get("config_file")
    dpi = config.get("dpi", 192)

    renderer = PageRenderer(
        config["source_dir"],
        cache,
        kroki_url=kroki_url,
        include_dirs=include_dirs,
        config_file=config_file,
        dpi=dpi,
    )
    navigation = NavigationBuilder(config["source_dir"], cache)

    app[renderer_key] = renderer
    app[navigation_key] = navigation
    app[cache_key] = cache
    app[verbose_key] = config.get("verbose", False)

    # API routes (must be registered first to take precedence over SPA fallback)
    app.router.add_routes(create_pages_routes())
    app.router.add_routes(create_navigation_routes())

    # Live reload WebSocket endpoint
    if config.get("live_reload", False):
        from docstage.live import LiveReloadManager
        from docstage.live.reload import create_live_reload_routes

        manager = LiveReloadManager(
            config["source_dir"],
            watch_patterns=config.get("watch_patterns"),
        )
        app["live_reload_manager"] = manager
        app.router.add_routes(create_live_reload_routes(manager))
        app.on_startup.append(_start_live_reload)
        app.on_cleanup.append(_stop_live_reload)

    # Static file serving for frontend (bundled assets)
    static_dir = get_static_dir()
    app["static_dir"] = static_dir

    assets_dir = static_dir / "assets"
    if assets_dir.exists():
        app.router.add_static("/assets", assets_dir)

    app.router.add_get("/favicon.png", _serve_favicon)

    # SPA fallback - must be last to catch all non-API routes
    app.router.add_get("/{path:.*}", spa_fallback)

    return app


async def _start_live_reload(app: web.Application) -> None:
    """Start live reload on application startup."""
    from docstage.live import LiveReloadManager

    manager: LiveReloadManager = app["live_reload_manager"]
    await manager.start()


async def _stop_live_reload(app: web.Application) -> None:
    """Stop live reload on application cleanup."""
    from docstage.live import LiveReloadManager

    manager: LiveReloadManager = app["live_reload_manager"]
    await manager.stop()


async def _serve_favicon(request: web.Request) -> web.FileResponse:
    """Serve favicon from static directory."""
    static_dir: Path = request.app["static_dir"]
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
