"""Navigation API endpoints.

Provides full navigation tree and subtree endpoints.
"""

from aiohttp import web

from docstage.app_keys import navigation_key


def create_navigation_routes() -> list[web.RouteDef]:
    return [
        web.get("/api/navigation", get_navigation),
        web.get("/api/navigation/{path:.*}", get_navigation_subtree),
    ]


async def get_navigation(request: web.Request) -> web.Response:
    navigation = request.app[navigation_key]
    tree = navigation.build()
    return web.json_response(tree.to_dict())


async def get_navigation_subtree(request: web.Request) -> web.Response:
    path = request.match_info["path"]
    navigation = request.app[navigation_key]

    subtree = navigation.get_subtree(path)
    if subtree is None:
        return web.json_response(
            {"error": "Section not found", "path": path},
            status=404,
        )

    return web.json_response(subtree.to_dict())
