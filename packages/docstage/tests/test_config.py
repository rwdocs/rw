"""Tests for configuration loading."""

from pathlib import Path
from unittest.mock import patch

import pytest
from docstage.config import (
    Config,
    ConfluenceConfig,
    DiagramsConfig,
    DocsConfig,
    ServerConfig,
)


class TestConfigLoad:
    """Tests for Config.load()."""

    def test__explicit_path__loads_config(self, tmp_path: Path) -> None:
        """Load config from explicit path."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[server]
host = "0.0.0.0"
port = 3000

[docs]
source_dir = "documentation"
cache_dir = ".docstage-cache"

[diagrams]
kroki_url = "https://kroki.io"
include_dirs = [".", "includes"]
config_file = "config.iuml"
dpi = 144

[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
access_secret = "secret456"
consumer_key = "my-consumer"

[confluence.test]
space_key = "DOCS"
""")

        config = Config.load(config_file)

        assert config.server.host == "0.0.0.0"
        assert config.server.port == 3000
        assert config.docs.source_dir == tmp_path / "documentation"
        assert config.docs.cache_dir == tmp_path / ".docstage-cache"
        assert config.diagrams.kroki_url == "https://kroki.io"
        assert config.diagrams.include_dirs == [tmp_path / ".", tmp_path / "includes"]
        assert config.diagrams.config_file == "config.iuml"
        assert config.diagrams.dpi == 144
        assert config.confluence is not None
        assert config.confluence.base_url == "https://confluence.example.com"
        assert config.confluence.access_token == "token123"
        assert config.confluence.access_secret == "secret456"
        assert config.confluence.consumer_key == "my-consumer"
        assert config.confluence_test is not None
        assert config.confluence_test.space_key == "DOCS"
        assert config.config_path == config_file

    def test__minimal_config__uses_defaults(self, tmp_path: Path) -> None:
        """Load minimal config with defaults relative to config file."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")

        config = Config.load(config_file)

        assert config.server.host == "127.0.0.1"
        assert config.server.port == 8080
        assert config.docs.source_dir == tmp_path / "docs"
        assert config.docs.cache_dir == tmp_path / ".cache"
        assert config.diagrams.kroki_url is None
        assert config.diagrams.include_dirs == []
        assert config.diagrams.config_file is None
        assert config.diagrams.dpi == 192
        assert config.confluence is None
        assert config.confluence_test is None

    def test__missing_explicit_path__raises_error(self, tmp_path: Path) -> None:
        """Raise FileNotFoundError for missing explicit config file."""
        config_file = tmp_path / "nonexistent.toml"

        with pytest.raises(FileNotFoundError, match="Configuration file not found"):
            Config.load(config_file)

    def test__no_path_no_discovery__returns_defaults(self, tmp_path: Path) -> None:
        """Return defaults when no config file found."""
        with patch.object(Config, "_discover_config", return_value=None):
            config = Config.load()

        assert config.server.host == "127.0.0.1"
        assert config.server.port == 8080
        assert config.docs.source_dir == Path("docs")
        assert config.docs.cache_dir == Path(".cache")
        assert config.confluence is None
        assert config.config_path is None


class TestConfigDiscovery:
    """Tests for config file discovery."""

    def test__config_in_current_dir__found(self, tmp_path: Path) -> None:
        """Find config in current directory."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("[server]\nport = 9000")

        with patch("pathlib.Path.cwd", return_value=tmp_path):
            discovered = Config._discover_config()

        assert discovered == config_file

    def test__config_in_parent_dir__found(self, tmp_path: Path) -> None:
        """Find config in parent directory."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("[server]\nport = 9000")
        subdir = tmp_path / "subproject" / "src"
        subdir.mkdir(parents=True)

        with patch("pathlib.Path.cwd", return_value=subdir):
            discovered = Config._discover_config()

        assert discovered == config_file

    def test__no_config__returns_none(self, tmp_path: Path) -> None:
        """Return None when no config found."""
        with patch("pathlib.Path.cwd", return_value=tmp_path):
            discovered = Config._discover_config()

        assert discovered is None


class TestServerConfigParsing:
    """Tests for server config section parsing."""

    def test__valid_server__parses_correctly(self, tmp_path: Path) -> None:
        """Parse valid server section."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[server]
host = "0.0.0.0"
port = 3000
""")

        config = Config.load(config_file)

        assert config.server.host == "0.0.0.0"
        assert config.server.port == 3000

    def test__invalid_host_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when host is not a string."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[server]
host = 12345
""")

        with pytest.raises(ValueError, match="server.host must be a string"):
            Config.load(config_file)

    def test__invalid_port_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when port is not an integer."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[server]
port = "3000"
""")

        with pytest.raises(ValueError, match="server.port must be an integer"):
            Config.load(config_file)


class TestDocsConfigParsing:
    """Tests for docs config section parsing."""

    def test__valid_docs__parses_correctly(self, tmp_path: Path) -> None:
        """Parse valid docs section."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[docs]
source_dir = "documentation"
cache_dir = ".docstage-cache"
""")

        config = Config.load(config_file)

        assert config.docs.source_dir == tmp_path / "documentation"
        assert config.docs.cache_dir == tmp_path / ".docstage-cache"

    def test__invalid_source_dir_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when source_dir is not a string."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[docs]
source_dir = 123
""")

        with pytest.raises(ValueError, match="docs.source_dir must be a string"):
            Config.load(config_file)


class TestDiagramsConfigParsing:
    """Tests for diagrams config section parsing."""

    def test__valid_diagrams__parses_correctly(self, tmp_path: Path) -> None:
        """Parse valid diagrams section."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[diagrams]
kroki_url = "https://kroki.io"
include_dirs = [".", "includes", "gen/includes"]
config_file = "config.iuml"
dpi = 144
""")

        config = Config.load(config_file)

        assert config.diagrams.kroki_url == "https://kroki.io"
        assert config.diagrams.include_dirs == [
            tmp_path / ".",
            tmp_path / "includes",
            tmp_path / "gen/includes",
        ]
        assert config.diagrams.config_file == "config.iuml"
        assert config.diagrams.dpi == 144

    def test__invalid_kroki_url_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when kroki_url is not a string."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[diagrams]
kroki_url = 12345
""")

        with pytest.raises(ValueError, match="diagrams.kroki_url must be a string"):
            Config.load(config_file)

    def test__invalid_include_dirs_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when include_dirs is not a list."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[diagrams]
include_dirs = "not-a-list"
""")

        with pytest.raises(ValueError, match="diagrams.include_dirs must be a list"):
            Config.load(config_file)

    def test__invalid_include_dirs_item__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when include_dirs item is not a string."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[diagrams]
include_dirs = [123]
""")

        with pytest.raises(
            ValueError,
            match="diagrams.include_dirs items must be strings",
        ):
            Config.load(config_file)

    def test__invalid_dpi_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when dpi is not an integer."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[diagrams]
dpi = "192"
""")

        with pytest.raises(ValueError, match="diagrams.dpi must be an integer"):
            Config.load(config_file)


class TestConfluenceConfigParsing:
    """Tests for confluence config section parsing."""

    def test__valid_confluence__parses_correctly(self, tmp_path: Path) -> None:
        """Parse valid confluence section."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
access_secret = "secret456"
consumer_key = "my-consumer"
""")

        config = Config.load(config_file)

        assert config.confluence is not None
        assert config.confluence.base_url == "https://confluence.example.com"
        assert config.confluence.access_token == "token123"
        assert config.confluence.access_secret == "secret456"
        assert config.confluence.consumer_key == "my-consumer"

    def test__confluence_default_consumer_key__uses_docstage(
        self,
        tmp_path: Path,
    ) -> None:
        """Consumer key defaults to 'docstage'."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
access_secret = "secret456"
""")

        config = Config.load(config_file)

        assert config.confluence is not None
        assert config.confluence.consumer_key == "docstage"

    def test__confluence_without_base_url__returns_none(self, tmp_path: Path) -> None:
        """Return None for confluence when base_url is missing."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[confluence]
access_token = "token123"
""")

        config = Config.load(config_file)

        assert config.confluence is None

    def test__invalid_access_token_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when access_token is not a string."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = 123
""")

        with pytest.raises(
            ValueError,
            match="confluence.access_token must be a string",
        ):
            Config.load(config_file)

    def test__missing_access_secret__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when access_secret is missing."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
""")

        with pytest.raises(
            ValueError,
            match="confluence.access_secret must be a string",
        ):
            Config.load(config_file)


class TestConfluenceTestConfigParsing:
    """Tests for confluence.test config section parsing."""

    def test__valid_confluence_test__parses_correctly(self, tmp_path: Path) -> None:
        """Parse valid confluence.test section."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
access_secret = "secret456"

[confluence.test]
space_key = "DOCS"
""")

        config = Config.load(config_file)

        assert config.confluence_test is not None
        assert config.confluence_test.space_key == "DOCS"

    def test__invalid_space_key_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when space_key is not a string."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[confluence.test]
space_key = 123
""")

        with pytest.raises(
            ValueError,
            match="confluence.test.space_key must be a string",
        ):
            Config.load(config_file)


class TestServerConfig:
    """Tests for ServerConfig dataclass."""

    def test__defaults__returns_localhost(self) -> None:
        """Default host is localhost, port is 8080."""
        config = ServerConfig()

        assert config.host == "127.0.0.1"
        assert config.port == 8080


class TestDocsConfig:
    """Tests for DocsConfig dataclass."""

    def test__defaults__returns_docs_and_cache(self) -> None:
        """Default source_dir is docs, cache_dir is .cache."""
        config = DocsConfig()

        assert config.source_dir == Path("docs")
        assert config.cache_dir == Path(".cache")


class TestDiagramsConfig:
    """Tests for DiagramsConfig dataclass."""

    def test__defaults__returns_none_and_empty(self) -> None:
        """Defaults have no kroki_url and empty include_dirs."""
        config = DiagramsConfig()

        assert config.kroki_url is None
        assert config.include_dirs == []
        assert config.config_file is None
        assert config.dpi == 192


class TestConfluenceConfig:
    """Tests for ConfluenceConfig dataclass."""

    def test__default_consumer_key__is_docstage(self) -> None:
        """Consumer key defaults to 'docstage'."""
        config = ConfluenceConfig(
            base_url="https://example.com",
            access_token="token",
            access_secret="secret",
        )

        assert config.consumer_key == "docstage"

    def test__custom_consumer_key__is_used(self) -> None:
        """Consumer key can be customized."""
        config = ConfluenceConfig(
            base_url="https://example.com",
            access_token="token",
            access_secret="secret",
            consumer_key="custom-key",
        )

        assert config.consumer_key == "custom-key"


class TestConfigWithOverrides:
    """Tests for Config.with_overrides method."""

    def test__no_overrides__returns_same_values(self) -> None:
        """When no overrides are provided, values remain unchanged."""
        original = Config._default()

        result = original.with_overrides()

        assert result.server.host == original.server.host
        assert result.server.port == original.server.port
        assert result.docs.source_dir == original.docs.source_dir
        assert result.docs.cache_dir == original.docs.cache_dir
        assert result.diagrams.kroki_url == original.diagrams.kroki_url
        assert result.live_reload.enabled == original.live_reload.enabled

    def test__override_host__changes_only_host(self) -> None:
        """Override host changes only server.host."""
        original = Config._default()

        result = original.with_overrides(host="0.0.0.0")

        assert result.server.host == "0.0.0.0"
        assert result.server.port == original.server.port

    def test__override_port__changes_only_port(self) -> None:
        """Override port changes only server.port."""
        original = Config._default()

        result = original.with_overrides(port=9000)

        assert result.server.port == 9000
        assert result.server.host == original.server.host

    def test__override_source_dir__changes_only_source_dir(self) -> None:
        """Override source_dir changes only docs.source_dir."""
        original = Config._default()
        new_path = Path("/custom/docs")

        result = original.with_overrides(source_dir=new_path)

        assert result.docs.source_dir == new_path
        assert result.docs.cache_dir == original.docs.cache_dir

    def test__override_kroki_url__changes_diagrams_kroki_url(self) -> None:
        """Override kroki_url changes diagrams.kroki_url."""
        original = Config._default()

        result = original.with_overrides(kroki_url="https://kroki.example.com")

        assert result.diagrams.kroki_url == "https://kroki.example.com"

    def test__override_live_reload_enabled__changes_live_reload(self) -> None:
        """Override live_reload_enabled changes live_reload.enabled."""
        original = Config._default()
        assert original.live_reload.enabled is True

        result = original.with_overrides(live_reload_enabled=False)

        assert result.live_reload.enabled is False

    def test__multiple_overrides__changes_all_specified(self) -> None:
        """Multiple overrides change all specified values."""
        original = Config._default()

        result = original.with_overrides(
            host="0.0.0.0",
            port=9000,
            kroki_url="https://kroki.io",
            live_reload_enabled=False,
        )

        assert result.server.host == "0.0.0.0"
        assert result.server.port == 9000
        assert result.diagrams.kroki_url == "https://kroki.io"
        assert result.live_reload.enabled is False

    def test__immutability__original_unchanged(self) -> None:
        """with_overrides does not mutate the original Config."""
        original = Config._default()
        original_host = original.server.host
        original_port = original.server.port

        _ = original.with_overrides(host="0.0.0.0", port=9000)

        assert original.server.host == original_host
        assert original.server.port == original_port
