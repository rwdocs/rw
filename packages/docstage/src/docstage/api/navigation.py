"""Navigation API endpoints.

Provides full navigation tree and subtree endpoints.
"""

from aiohttp import web

from docstage.app_keys import site_loader_key
from docstage.core.navigation import build_navigation


def create_navigation_routes() -> list[web.RouteDef]:
    return [
        web.get("/api/navigation", get_navigation),
        web.get("/api/navigation/{path:.*}", get_navigation_subtree),
    ]


async def get_navigation(request: web.Request) -> web.Response:
    site_loader = request.app[site_loader_key]
    site = site_loader.load()
    nav_items = build_navigation(site)
    return web.json_response({"items": [item.to_dict() for item in nav_items]})


async def get_navigation_subtree(request: web.Request) -> web.Response:
    path = request.match_info["path"]
    site_loader = request.app[site_loader_key]
    site = site_loader.load()

    normalized = path if path.startswith("/") else f"/{path}"
    page = site.get_page(normalized)
    if page is None:
        return web.json_response(
            {"error": "Section not found", "path": path},
            status=404,
        )

    subtree = build_navigation(site, normalized)
    return web.json_response({"items": [item.to_dict() for item in subtree]})
