"""Navigation API endpoints.

Provides full navigation tree and subtree endpoints.
"""

from aiohttp import web

from docstage.core.navigation import NavigationBuilder


def create_navigation_routes() -> list[web.RouteDef]:
    """Create route definitions for navigation API.

    Returns:
        List of route definitions
    """
    return [
        web.get("/api/navigation", get_navigation),
        web.get("/api/navigation/{path:.*}", get_navigation_subtree),
    ]


async def get_navigation(request: web.Request) -> web.Response:
    """Get full navigation tree.

    Args:
        request: aiohttp request

    Returns:
        JSON response with navigation tree
    """
    navigation: NavigationBuilder = request.app["navigation"]
    tree = navigation.build()

    return web.json_response(tree.to_dict())


async def get_navigation_subtree(request: web.Request) -> web.Response:
    """Get navigation subtree for a specific section.

    Args:
        request: aiohttp request

    Returns:
        JSON response with navigation subtree
    """
    path = request.match_info["path"]
    navigation: NavigationBuilder = request.app["navigation"]

    subtree = navigation.get_subtree(path)
    if subtree is None:
        return web.json_response(
            {"error": "Section not found", "path": path},
            status=404,
        )

    return web.json_response(subtree.to_dict())
