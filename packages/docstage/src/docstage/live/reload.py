"""WebSocket-based live reload for development mode.

Monitors source markdown files for changes and notifies connected clients
via WebSocket to trigger page reloads.
"""

import asyncio
import json
import weakref
from pathlib import Path
from typing import TYPE_CHECKING

from aiohttp import WSMsgType, web
from watchfiles import Change, awatch

if TYPE_CHECKING:
    from docstage.core.navigation import NavigationBuilder


class LiveReloadManager:
    """Manages WebSocket connections and file watching for live reload.

    Coordinates between file system watcher and connected WebSocket clients
    to provide automatic page refresh on source file changes.
    """

    def __init__(
        self,
        source_dir: Path,
        watch_patterns: list[str] | None = None,
        *,
        navigation: NavigationBuilder | None = None,
    ) -> None:
        """Initialize the live reload manager.

        Args:
            source_dir: Directory to watch for changes
            watch_patterns: Glob patterns to watch (default: ["**/*.md"])
            navigation: NavigationBuilder instance for navigation cache invalidation
        """
        self._source_dir = source_dir
        self._watch_patterns = watch_patterns or ["**/*.md"]
        self._connections: weakref.WeakSet[web.WebSocketResponse] = weakref.WeakSet()
        self._watch_task: asyncio.Task[None] | None = None
        self._navigation = navigation

    async def start(self) -> None:
        """Start the file watcher."""
        if self._watch_task is not None:
            return
        self._watch_task = asyncio.create_task(self._watch_files())

    async def stop(self) -> None:
        """Stop the file watcher and close all connections."""
        if self._watch_task is not None:
            self._watch_task.cancel()
            try:
                await self._watch_task
            except asyncio.CancelledError:
                pass
            self._watch_task = None

        for ws in list(self._connections):
            await ws.close()

    async def handle_websocket(self, request: web.Request) -> web.WebSocketResponse:
        """Handle WebSocket connection for live reload.

        Args:
            request: aiohttp request

        Returns:
            WebSocket response
        """
        ws = web.WebSocketResponse()
        await ws.prepare(request)

        self._connections.add(ws)

        try:
            async for msg in ws:
                if msg.type == WSMsgType.ERROR:
                    break
        finally:
            self._connections.discard(ws)

        return ws

    async def _watch_files(self) -> None:
        """Watch for file changes and broadcast reload events."""
        async for changes in awatch(self._source_dir):
            for change_type, path_str in changes:
                if change_type == Change.deleted:
                    continue

                path = Path(path_str)
                if not self._matches_patterns(path):
                    continue

                doc_path = self._to_doc_path(path)

                self._invalidate_caches()
                await self._broadcast_reload(doc_path)

    def _invalidate_caches(self) -> None:
        """Invalidate navigation cache.

        Note: Page cache uses mtime-based invalidation, so explicit invalidation
        is not needed - the cache will automatically return None when the source
        file's mtime changes.
        """
        if self._navigation:
            self._navigation.invalidate()

    def _matches_patterns(self, path: Path) -> bool:
        """Check if a path matches any watch pattern.

        Args:
            path: Path to check

        Returns:
            True if path matches any pattern
        """
        try:
            relative = path.relative_to(self._source_dir)
        except ValueError:
            return False

        for pattern in self._watch_patterns:
            if relative.match(pattern):
                return True
        return False

    def _to_doc_path(self, file_path: Path) -> str:
        """Convert a file system path to a documentation path.

        Args:
            file_path: Absolute file path

        Returns:
            Documentation path (e.g., "/docs/guide/setup")
        """
        relative = file_path.relative_to(self._source_dir)
        doc_path = str(relative.with_suffix(""))

        if doc_path.endswith("/index") or doc_path == "index":
            doc_path = doc_path.rsplit("/index", 1)[0] or ""

        return f"/docs/{doc_path}" if doc_path else "/docs"

    async def _broadcast_reload(self, path: str) -> None:
        """Broadcast reload event to all connected clients.

        Args:
            path: Documentation path that changed
        """
        if not self._connections:
            return

        message = json.dumps({"type": "reload", "path": path})

        for ws in list(self._connections):
            if ws.closed:
                continue
            try:
                await ws.send_str(message)
            except ConnectionResetError:
                # Client disconnected mid-send, will be cleaned up by WeakSet
                pass


def create_live_reload_routes(manager: LiveReloadManager) -> list[web.RouteDef]:
    """Create routes for live reload WebSocket.

    Args:
        manager: LiveReloadManager instance

    Returns:
        List of route definitions
    """
    return [web.get("/ws/live-reload", manager.handle_websocket)]
