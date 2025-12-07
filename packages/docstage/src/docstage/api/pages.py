"""Pages API endpoint.

Handles page rendering and returns JSON responses with metadata, ToC, and HTML content.
"""

from datetime import datetime, timezone
from email.utils import formatdate
from hashlib import md5
from time import mktime

from aiohttp import web

from docstage.app_keys import navigation_key, renderer_key
from docstage.core.navigation import NavigationBuilder


def create_pages_routes() -> list[web.RouteDef]:
    return [
        web.get("/api/pages/{path:.*}", get_page),
    ]


async def get_page(request: web.Request) -> web.Response:
    path = request.match_info["path"]
    renderer = request.app[renderer_key]
    navigation = request.app[navigation_key]

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
    # Use first 16 hex chars (64 bits) - sufficient for cache invalidation,
    # collision probability is negligible for this use case
    content_hash = md5(content.encode("utf-8"), usedforsecurity=False).hexdigest()[:16]
    return f'"{content_hash}"'


def _build_breadcrumbs(
    path: str,
    navigation: NavigationBuilder,
) -> list[dict[str, str]]:
    if not path:
        return []

    parts = path.split("/")
    breadcrumbs: list[dict[str, str]] = []
    # Navigation tree is cached by NavigationBuilder, so this is efficient
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
