"""Navigation API endpoints."""

from aiohttp import web
from docstage_core import build_navigation

from docstage.app_keys import site_loader_key


def create_navigation_routes() -> list[web.RouteDef]:
    return [web.get("/api/navigation", get_navigation)]


async def get_navigation(request: web.Request) -> web.Response:
    site_loader = request.app[site_loader_key]
    site = site_loader.load()
    nav_items = build_navigation(site)
    return web.json_response({"items": [item.to_dict() for item in nav_items]})
