"""Config API endpoint."""

from aiohttp import web

from docstage.app_keys import live_reload_enabled_key


def create_config_routes() -> list[web.RouteDef]:
    return [web.get("/api/config", get_config)]


async def get_config(request: web.Request) -> web.Response:
    live_reload_enabled = request.app[live_reload_enabled_key]
    return web.json_response({"liveReloadEnabled": live_reload_enabled})
