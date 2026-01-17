"""Tests for config API endpoint."""

from pathlib import Path

import pytest
from docstage.config import Config
from docstage.server import create_app


def _make_config(
    source_dir: Path, cache_dir: Path, live_reload_enabled: bool, config_dir: Path
) -> Config:
    """Create a Config for testing by writing a temp TOML file."""
    config_file = config_dir / "docstage.toml"
    config_file.write_text(f"""
[docs]
source_dir = "{source_dir.name}"
cache_dir = "{cache_dir.name}"

[live_reload]
enabled = {str(live_reload_enabled).lower()}
""")
    return Config.load(config_file)


class TestGetConfig:
    """Tests for GET /api/config."""

    @pytest.mark.asyncio
    async def test__live_reload_enabled__returns_true(
        self,
        tmp_path: Path,
        aiohttp_client,
    ) -> None:
        """Return liveReloadEnabled: true when live reload is enabled."""
        docs = tmp_path / "docs"
        docs.mkdir()
        config = _make_config(
            docs, tmp_path / ".cache", live_reload_enabled=True, config_dir=tmp_path
        )
        app = create_app(config)
        test_client = await aiohttp_client(app)

        response = await test_client.get("/api/config")

        assert response.status == 200
        data = await response.json()
        assert data == {"liveReloadEnabled": True}

    @pytest.mark.asyncio
    async def test__live_reload_disabled__returns_false(
        self,
        tmp_path: Path,
        aiohttp_client,
    ) -> None:
        """Return liveReloadEnabled: false when live reload is disabled."""
        docs = tmp_path / "docs"
        docs.mkdir()
        config = _make_config(
            docs, tmp_path / ".cache", live_reload_enabled=False, config_dir=tmp_path
        )
        app = create_app(config)
        test_client = await aiohttp_client(app)

        response = await test_client.get("/api/config")

        assert response.status == 200
        data = await response.json()
        assert data == {"liveReloadEnabled": False}
