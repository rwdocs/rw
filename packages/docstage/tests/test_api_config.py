"""Tests for config API endpoint."""

from collections.abc import Callable

import pytest
from docstage.config import Config
from docstage.server import create_app


class TestGetConfig:
    """Tests for GET /api/config."""

    @pytest.mark.asyncio
    @pytest.mark.parametrize("enabled", [True, False])
    async def test__live_reload_setting__returns_config_value(
        self,
        make_config: Callable[..., Config],
        aiohttp_client,
        enabled: bool,
    ) -> None:
        """Return liveReloadEnabled matching the configured value."""
        config = make_config(live_reload_enabled=enabled)
        app = create_app(config)
        test_client = await aiohttp_client(app)

        response = await test_client.get("/api/config")

        assert response.status == 200
        data = await response.json()
        assert data == {"liveReloadEnabled": enabled}
