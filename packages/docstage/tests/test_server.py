"""Tests for server module."""

from pathlib import Path
from typing import Any

import pytest
from aiohttp import web

from docstage.app_keys import cache_key, navigation_key, renderer_key
from docstage.assets import get_static_dir
from docstage.server import ServerConfig, create_app


class TestCreateApp:
    """Tests for create_app()."""

    def test__valid_config__returns_configured_app(self, tmp_path: Path) -> None:
        """Create app with valid configuration."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        cache_dir = tmp_path / ".cache"

        config: ServerConfig = {
            "host": "127.0.0.1",
            "port": 8080,
            "source_dir": source_dir,
            "cache_dir": cache_dir,
        }

        app = create_app(config)

        assert renderer_key in app
        assert navigation_key in app
        assert cache_key in app
        assert app[renderer_key].source_dir == source_dir
        assert app[navigation_key].source_dir == source_dir
        assert app[cache_key].cache_dir == cache_dir

    def test__app__uses_bundled_static_assets(self, tmp_path: Path) -> None:
        """Create app uses bundled static assets."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        cache_dir = tmp_path / ".cache"

        config: ServerConfig = {
            "host": "127.0.0.1",
            "port": 8080,
            "source_dir": source_dir,
            "cache_dir": cache_dir,
        }

        app = create_app(config)

        assert app["static_dir"] == get_static_dir()


class TestSpaFallback:
    """Tests for SPA fallback route."""

    @pytest.fixture
    def app(self, tmp_path: Path) -> web.Application:
        """Create app with bundled static assets for testing."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        cache_dir = tmp_path / ".cache"

        config: ServerConfig = {
            "host": "127.0.0.1",
            "port": 8080,
            "source_dir": source_dir,
            "cache_dir": cache_dir,
        }

        return create_app(config)

    @pytest.mark.asyncio
    async def test__root_path__serves_index_html(
        self, aiohttp_client: Any, app: web.Application
    ) -> None:
        """Root path serves index.html."""
        client = await aiohttp_client(app)
        response = await client.get("/")

        assert response.status == 200
        assert "text/html" in response.headers["Content-Type"]

    @pytest.mark.asyncio
    async def test__spa_route__serves_index_html(
        self, aiohttp_client: Any, app: web.Application
    ) -> None:
        """SPA routes serve index.html for client-side routing."""
        client = await aiohttp_client(app)
        response = await client.get("/docs/some/path")

        assert response.status == 200
        assert "text/html" in response.headers["Content-Type"]

    @pytest.mark.asyncio
    async def test__api_routes__take_precedence(
        self, aiohttp_client: Any, app: web.Application
    ) -> None:
        """API routes take precedence over SPA fallback."""
        client = await aiohttp_client(app)
        response = await client.get("/api/navigation")

        assert response.status == 200
        data = await response.json()
        assert "items" in data
