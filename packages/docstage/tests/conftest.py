"""Shared test fixtures."""

from pathlib import Path

import pytest
from docstage.config import (
    Config,
    DiagramsConfig,
    DocsConfig,
    LiveReloadConfig,
    ServerConfig,
)


@pytest.fixture
def test_config(tmp_path: Path) -> Config:
    """Create a test configuration with tmp_path directories.

    Creates source_dir and returns a Config instance suitable for testing.
    Use exist_ok=True to allow other fixtures to also create the docs dir.
    """
    source_dir = tmp_path / "docs"
    source_dir.mkdir(exist_ok=True)
    cache_dir = tmp_path / ".cache"

    return Config(
        server=ServerConfig(),
        docs=DocsConfig(source_dir=source_dir, cache_dir=cache_dir),
        diagrams=DiagramsConfig(),
        live_reload=LiveReloadConfig(enabled=False),
        confluence=None,
        confluence_test=None,
    )
