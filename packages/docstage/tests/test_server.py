"""Tests for server module."""

from pathlib import Path

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
        }

        app = create_app(config)

        assert renderer_key in app
        assert navigation_key in app
        assert cache_key in app
        assert app[renderer_key].source_dir == source_dir
        assert app[navigation_key].source_dir == source_dir
        assert app[cache_key].cache_dir == cache_dir
