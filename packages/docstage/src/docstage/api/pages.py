"""Pages API endpoint.

Handles page rendering and returns JSON responses with metadata, ToC, and HTML content.
"""

import sys
from datetime import UTC, datetime
from email.utils import formatdate
from hashlib import md5
from time import mktime

from aiohttp import web

from docstage.app_keys import navigation_key, renderer_key, verbose_key


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

    # Log warnings in verbose mode
    if request.app[verbose_key] and result.warnings:
        for warning in result.warnings:
            print(f"[WARNING] {path}: {warning}", file=sys.stderr)

    source_mtime = result.source_path.stat().st_mtime
    last_modified = datetime.fromtimestamp(source_mtime, tz=UTC)

    etag = _compute_etag(result.html)

    if_none_match = request.headers.get("If-None-Match")
    if if_none_match == etag:
        return web.Response(status=304)

    site = navigation.build_site()
    breadcrumbs = [b.to_dict() for b in site.get_breadcrumbs(path)]

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
