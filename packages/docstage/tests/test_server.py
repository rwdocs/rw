"""Tests for server module."""

from pathlib import Path
from typing import Any

import pytest
from aiohttp import web

from docstage.app_keys import cache_key, navigation_key, renderer_key
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
            "static_dir": None,
        }

        app = create_app(config)

        assert renderer_key in app
        assert navigation_key in app
        assert cache_key in app
        assert app[renderer_key].source_dir == source_dir
        assert app[navigation_key].source_dir == source_dir
        assert app[cache_key].cache_dir == cache_dir

    def test__with_static_dir__registers_static_routes(self, tmp_path: Path) -> None:
        """Create app with static directory configures SPA routes."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        cache_dir = tmp_path / ".cache"
        static_dir = tmp_path / "static"
        static_dir.mkdir()
        (static_dir / "index.html").write_text("<html></html>")
        (static_dir / "assets").mkdir()

        config: ServerConfig = {
            "host": "127.0.0.1",
            "port": 8080,
            "source_dir": source_dir,
            "cache_dir": cache_dir,
            "static_dir": static_dir,
        }

        app = create_app(config)

        assert app.get("static_dir") == static_dir

    def test__without_static_dir__no_static_routes(self, tmp_path: Path) -> None:
        """Create app without static directory does not configure SPA routes."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        cache_dir = tmp_path / ".cache"

        config: ServerConfig = {
            "host": "127.0.0.1",
            "port": 8080,
            "source_dir": source_dir,
            "cache_dir": cache_dir,
            "static_dir": None,
        }

        app = create_app(config)

        assert app.get("static_dir") is None


class TestSpaFallback:
    """Tests for SPA fallback route."""

    @pytest.fixture
    def static_app(self, tmp_path: Path) -> web.Application:
        """Create app with static directory for testing."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        cache_dir = tmp_path / ".cache"
        static_dir = tmp_path / "static"
        static_dir.mkdir()
        (static_dir / "index.html").write_text(
            "<!DOCTYPE html><html><body>Docstage</body></html>"
        )
        assets_dir = static_dir / "assets"
        assets_dir.mkdir()
        (assets_dir / "main.js").write_text("console.log('hello');")
        (static_dir / "favicon.png").write_bytes(b"fake-png-data")

        config: ServerConfig = {
            "host": "127.0.0.1",
            "port": 8080,
            "source_dir": source_dir,
            "cache_dir": cache_dir,
            "static_dir": static_dir,
        }

        return create_app(config)

    @pytest.mark.asyncio
    async def test__root_path__serves_index_html(
        self, aiohttp_client: Any, static_app: web.Application
    ) -> None:
        """Root path serves index.html."""
        client = await aiohttp_client(static_app)
        response = await client.get("/")

        assert response.status == 200
        text = await response.text()
        assert "Docstage" in text

    @pytest.mark.asyncio
    async def test__spa_route__serves_index_html(
        self, aiohttp_client: Any, static_app: web.Application
    ) -> None:
        """SPA routes serve index.html for client-side routing."""
        client = await aiohttp_client(static_app)
        response = await client.get("/docs/some/path")

        assert response.status == 200
        text = await response.text()
        assert "Docstage" in text

    @pytest.mark.asyncio
    async def test__assets_route__serves_static_files(
        self, aiohttp_client: Any, static_app: web.Application
    ) -> None:
        """Assets directory serves static files directly."""
        client = await aiohttp_client(static_app)
        response = await client.get("/assets/main.js")

        assert response.status == 200
        text = await response.text()
        assert "console.log" in text

    @pytest.mark.asyncio
    async def test__favicon__serves_favicon(
        self, aiohttp_client: Any, static_app: web.Application
    ) -> None:
        """Favicon route serves favicon.png."""
        client = await aiohttp_client(static_app)
        response = await client.get("/favicon.png")

        assert response.status == 200
        data = await response.read()
        assert data == b"fake-png-data"

    @pytest.mark.asyncio
    async def test__api_routes__take_precedence(
        self, aiohttp_client: Any, static_app: web.Application
    ) -> None:
        """API routes take precedence over SPA fallback."""
        client = await aiohttp_client(static_app)
        response = await client.get("/api/navigation")

        # Should return 200 from API, not index.html
        assert response.status == 200
        data = await response.json()
        assert "items" in data
