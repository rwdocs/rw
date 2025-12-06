"""Tests for configuration loading."""

import tempfile
from pathlib import Path

import pytest

from docstage.config import Config, ConfluenceConfig, TestConfig


class TestConfigFromToml:
    """Tests for Config.from_toml()."""

    def test_loads_minimal_config(self, tmp_path: Path) -> None:
        """Load config with only required fields."""
        config_file = tmp_path / 'config.toml'
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
access_secret = "secret456"
""")

        config = Config.from_toml(config_file)

        assert config.confluence.base_url == 'https://confluence.example.com'
        assert config.confluence.access_token == 'token123'
        assert config.confluence.access_secret == 'secret456'
        assert config.confluence.consumer_key == 'adrflow'  # default
        assert config.test is None

    def test_loads_full_config(self, tmp_path: Path) -> None:
        """Load config with all fields."""
        config_file = tmp_path / 'config.toml'
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
access_secret = "secret456"
consumer_key = "my-consumer"

[test]
space_key = "DOCS"
""")

        config = Config.from_toml(config_file)

        assert config.confluence.consumer_key == 'my-consumer'
        assert config.test is not None
        assert config.test.space_key == 'DOCS'

    def test_raises_on_missing_file(self, tmp_path: Path) -> None:
        """Raise FileNotFoundError for missing config file."""
        config_file = tmp_path / 'nonexistent.toml'

        with pytest.raises(FileNotFoundError, match='Configuration file not found'):
            Config.from_toml(config_file)

    def test_raises_on_missing_confluence_section(self, tmp_path: Path) -> None:
        """Raise ValueError when confluence section is missing."""
        config_file = tmp_path / 'config.toml'
        config_file.write_text("""
[test]
space_key = "DOCS"
""")

        with pytest.raises(ValueError, match='confluence section is required'):
            Config.from_toml(config_file)

    def test_raises_on_missing_base_url(self, tmp_path: Path) -> None:
        """Raise ValueError when base_url is missing."""
        config_file = tmp_path / 'config.toml'
        config_file.write_text("""
[confluence]
access_token = "token123"
access_secret = "secret456"
""")

        with pytest.raises(ValueError, match='confluence.base_url must be a string'):
            Config.from_toml(config_file)

    def test_raises_on_missing_access_token(self, tmp_path: Path) -> None:
        """Raise ValueError when access_token is missing."""
        config_file = tmp_path / 'config.toml'
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_secret = "secret456"
""")

        with pytest.raises(ValueError, match='confluence.access_token must be a string'):
            Config.from_toml(config_file)

    def test_raises_on_missing_access_secret(self, tmp_path: Path) -> None:
        """Raise ValueError when access_secret is missing."""
        config_file = tmp_path / 'config.toml'
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
""")

        with pytest.raises(ValueError, match='confluence.access_secret must be a string'):
            Config.from_toml(config_file)

    def test_raises_on_invalid_test_section(self, tmp_path: Path) -> None:
        """Raise ValueError when test section is invalid."""
        config_file = tmp_path / 'config.toml'
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
access_secret = "secret456"

[test]
# missing space_key
""")

        with pytest.raises(ValueError, match='test.space_key must be a string'):
            Config.from_toml(config_file)


class TestConfluenceConfig:
    """Tests for ConfluenceConfig dataclass."""

    def test_default_consumer_key(self) -> None:
        """Consumer key defaults to 'adrflow'."""
        config = ConfluenceConfig(
            base_url='https://example.com',
            access_token='token',
            access_secret='secret',
        )
        assert config.consumer_key == 'adrflow'

    def test_custom_consumer_key(self) -> None:
        """Consumer key can be customized."""
        config = ConfluenceConfig(
            base_url='https://example.com',
            access_token='token',
            access_secret='secret',
            consumer_key='custom-key',
        )
        assert config.consumer_key == 'custom-key'
