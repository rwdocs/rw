"""Pages API endpoint.

Handles page rendering and returns JSON responses with metadata, ToC, and HTML content.
"""

from datetime import datetime, timezone
from email.utils import formatdate
from hashlib import md5
from time import mktime

from aiohttp import web

from docstage.core.navigation import NavigationBuilder
from docstage.core.renderer import PageRenderer


def create_pages_routes() -> list[web.RouteDef]:
    """Create route definitions for pages API.

    Returns:
        List of route definitions
    """
    return [
        web.get("/api/pages/{path:.*}", get_page),
    ]


async def get_page(request: web.Request) -> web.Response:
    """Get rendered page with metadata.

    Args:
        request: aiohttp request

    Returns:
        JSON response with page data
    """
    path = request.match_info["path"]
    renderer: PageRenderer = request.app["renderer"]
    navigation: NavigationBuilder = request.app["navigation"]

    try:
        result = renderer.render(path)
    except FileNotFoundError:
        return web.json_response(
            {"error": "Page not found", "path": path},
            status=404,
        )

    source_mtime = result.source_path.stat().st_mtime
    last_modified = datetime.fromtimestamp(source_mtime, tz=timezone.utc)

    etag = _compute_etag(result.html)

    if_none_match = request.headers.get("If-None-Match")
    if if_none_match == etag:
        return web.Response(status=304)

    breadcrumbs = _build_breadcrumbs(path, navigation)

    response_data = {
        "meta": {
            "title": result.title,
            "path": f"/{path}" if path else "/",
            "source_file": str(result.source_path),
            "last_modified": last_modified.isoformat(),
        },
        "breadcrumbs": breadcrumbs,
        "toc": [
            {"level": entry.level, "title": entry.title, "id": entry.id}
            for entry in result.toc
        ],
        "content": result.html,
    }

    return web.json_response(
        response_data,
        headers={
            "ETag": etag,
            "Last-Modified": formatdate(mktime(last_modified.timetuple()), usegmt=True),
            "Cache-Control": "private, max-age=60",
        },
    )


def _compute_etag(content: str) -> str:
    """Compute ETag from content hash.

    Args:
        content: Content to hash

    Returns:
        ETag string with quotes
    """
    content_hash = md5(content.encode("utf-8"), usedforsecurity=False).hexdigest()[:16]
    return f'"{content_hash}"'


def _build_breadcrumbs(
    path: str,
    navigation: NavigationBuilder,
) -> list[dict[str, str]]:
    """Build breadcrumb trail for a path.

    Args:
        path: Page path
        navigation: Navigation builder for title lookup

    Returns:
        List of breadcrumb items with title and path
    """
    if not path:
        return []

    parts = path.split("/")
    breadcrumbs: list[dict[str, str]] = []
    nav_tree = navigation.build()

    current_path = ""
    items = nav_tree.items

    for part in parts:
        current_path = f"{current_path}/{part}" if current_path else f"/{part}"

        title = part.replace("-", " ").replace("_", " ").title()
        for item in items:
            if item.path == current_path:
                title = item.title
                items = item.children
                break

        breadcrumbs.append({"title": title, "path": current_path})

    return breadcrumbs
