"""Shared test fixtures."""

from collections.abc import Callable
from pathlib import Path

import pytest
from docstage.config import Config


@pytest.fixture
def test_config(tmp_path: Path) -> Config:
    """Create a test configuration with tmp_path directories.

    Creates source_dir and returns a Config instance suitable for testing.
    Use exist_ok=True to allow other fixtures to also create the docs dir.
    """
    source_dir = tmp_path / "docs"
    source_dir.mkdir(exist_ok=True)

    config_file = tmp_path / "docstage.toml"
    config_file.write_text("""
[docs]
source_dir = "docs"
cache_dir = ".cache"

[live_reload]
enabled = false
""")

    return Config.load(config_file)


@pytest.fixture
def make_config(tmp_path: Path) -> Callable[..., Config]:
    """Factory fixture for creating test configurations.

    Returns a function that creates Config instances with customizable options.
    Source and cache directories are created relative to tmp_path.
    """

    def _make_config(
        *,
        source_dir: Path | None = None,
        cache_dir: Path | None = None,
        live_reload_enabled: bool = False,
    ) -> Config:
        source_dir = source_dir or (tmp_path / "docs")
        cache_dir = cache_dir or (tmp_path / ".cache")
        source_dir.mkdir(exist_ok=True)

        config_file = tmp_path / "docstage.toml"
        config_file.write_text(f"""
[docs]
source_dir = "{source_dir.relative_to(tmp_path)}"
cache_dir = "{cache_dir.relative_to(tmp_path)}"

[live_reload]
enabled = {str(live_reload_enabled).lower()}
""")
        return Config.load(config_file)

    return _make_config
