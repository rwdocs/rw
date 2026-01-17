"""Shared test fixtures."""

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
