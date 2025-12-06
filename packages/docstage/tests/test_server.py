"""Tests for server module."""

from pathlib import Path

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

        assert "renderer" in app
        assert "navigation" in app
        assert "cache" in app
        assert app["renderer"].source_dir == source_dir
        assert app["navigation"].source_dir == source_dir
        assert app["cache"].cache_dir == cache_dir
