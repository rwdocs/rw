"""Tests for server module."""

from typing import Any

import pytest
from aiohttp import web
from docstage.app_keys import cache_key, renderer_key, site_loader_key
from docstage.assets import get_static_dir
from docstage.config import Config
from docstage.server import create_app

from tests.test_assets import requires_bundled_assets


@requires_bundled_assets
class TestCreateApp:
    """Tests for create_app()."""

    def test__valid_config__returns_configured_app(self, test_config: Config) -> None:
        """Create app with valid configuration."""
        app = create_app(test_config)

        assert renderer_key in app
        assert site_loader_key in app
        assert cache_key in app
        assert app[renderer_key].source_dir == test_config.docs.source_dir
        assert app[site_loader_key].source_dir == test_config.docs.source_dir
        assert app[cache_key].cache_dir == test_config.docs.cache_dir

    def test__app__uses_bundled_static_assets(self, test_config: Config) -> None:
        """Create app uses bundled static assets."""
        app = create_app(test_config)

        assert app["static_dir"] == get_static_dir()


@requires_bundled_assets
class TestSpaFallback:
    """Tests for SPA fallback route."""

    @pytest.fixture
    def app(self, test_config: Config) -> web.Application:
        """Create app with bundled static assets for testing."""
        return create_app(test_config)

    @pytest.mark.asyncio
    async def test__root_path__serves_index_html(
        self,
        aiohttp_client: Any,
        app: web.Application,
    ) -> None:
        """Root path serves index.html."""
        client = await aiohttp_client(app)
        response = await client.get("/")

        assert response.status == 200
        assert "text/html" in response.headers["Content-Type"]

    @pytest.mark.asyncio
    async def test__spa_route__serves_index_html(
        self,
        aiohttp_client: Any,
        app: web.Application,
    ) -> None:
        """SPA routes serve index.html for client-side routing."""
        client = await aiohttp_client(app)
        response = await client.get("/docs/some/path")

        assert response.status == 200
        assert "text/html" in response.headers["Content-Type"]

    @pytest.mark.asyncio
    async def test__api_routes__take_precedence(
        self,
        aiohttp_client: Any,
        app: web.Application,
    ) -> None:
        """API routes take precedence over SPA fallback."""
        client = await aiohttp_client(app)
        response = await client.get("/api/navigation")

        assert response.status == 200
        data = await response.json()
        assert "items" in data
