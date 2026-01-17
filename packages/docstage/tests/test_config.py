"""Tests for configuration loading."""

from pathlib import Path

import pytest
from docstage.config import Config


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

        with pytest.raises(ValueError, match="TOML parse error"):
            Config.load(config_file)

    def test__invalid_port_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when port is not an integer."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[server]
port = "3000"
""")

        with pytest.raises(ValueError, match="TOML parse error"):
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

        with pytest.raises(ValueError, match="TOML parse error"):
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

        with pytest.raises(ValueError, match="TOML parse error"):
            Config.load(config_file)

    def test__invalid_include_dirs_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when include_dirs is not a list."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[diagrams]
include_dirs = "not-a-list"
""")

        with pytest.raises(ValueError, match="TOML parse error"):
            Config.load(config_file)

    def test__invalid_dpi_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when dpi is not an integer."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[diagrams]
dpi = "192"
""")

        with pytest.raises(ValueError, match="TOML parse error"):
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

    def test__missing_access_secret__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when access_secret is missing."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[confluence]
base_url = "https://confluence.example.com"
access_token = "token123"
""")

        with pytest.raises(ValueError, match="TOML parse error"):
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


class TestLiveReloadConfigParsing:
    """Tests for live_reload config section parsing."""

    def test__valid_live_reload__parses_correctly(self, tmp_path: Path) -> None:
        """Parse valid live_reload section."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[live_reload]
enabled = false
watch_patterns = ["**/*.md", "**/*.toml"]
""")

        config = Config.load(config_file)

        assert config.live_reload.enabled is False
        assert config.live_reload.watch_patterns == ["**/*.md", "**/*.toml"]

    def test__live_reload_defaults__enabled_true_no_patterns(
        self, tmp_path: Path
    ) -> None:
        """Live reload defaults to enabled with no patterns."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")

        config = Config.load(config_file)

        assert config.live_reload.enabled is True
        assert config.live_reload.watch_patterns is None

    def test__invalid_enabled_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when enabled is not a boolean."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[live_reload]
enabled = "yes"
""")

        with pytest.raises(ValueError, match="TOML parse error"):
            Config.load(config_file)


class TestConfigWithOverrides:
    """Tests for Config.with_overrides method."""

    def test__no_overrides__returns_same_values(self, tmp_path: Path) -> None:
        """When no overrides are provided, values remain unchanged."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")
        original = Config.load(config_file)

        result = original.with_overrides()

        assert result.server.host == original.server.host
        assert result.server.port == original.server.port
        assert result.docs.source_dir == original.docs.source_dir
        assert result.docs.cache_dir == original.docs.cache_dir
        assert result.diagrams.kroki_url == original.diagrams.kroki_url
        assert result.live_reload.enabled == original.live_reload.enabled

    def test__override_host__changes_only_host(self, tmp_path: Path) -> None:
        """Override host changes only server.host."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")
        original = Config.load(config_file)

        result = original.with_overrides(host="0.0.0.0")

        assert result.server.host == "0.0.0.0"
        assert result.server.port == original.server.port

    def test__override_port__changes_only_port(self, tmp_path: Path) -> None:
        """Override port changes only server.port."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")
        original = Config.load(config_file)

        result = original.with_overrides(port=9000)

        assert result.server.port == 9000
        assert result.server.host == original.server.host

    def test__override_source_dir__changes_only_source_dir(
        self, tmp_path: Path
    ) -> None:
        """Override source_dir changes only docs.source_dir."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")
        original = Config.load(config_file)
        new_path = Path("/custom/docs")

        result = original.with_overrides(source_dir=new_path)

        assert result.docs.source_dir == new_path
        assert result.docs.cache_dir == original.docs.cache_dir

    def test__override_kroki_url__changes_diagrams_kroki_url(
        self, tmp_path: Path
    ) -> None:
        """Override kroki_url changes diagrams.kroki_url."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")
        original = Config.load(config_file)

        result = original.with_overrides(kroki_url="https://kroki.example.com")

        assert result.diagrams.kroki_url == "https://kroki.example.com"

    def test__override_live_reload_enabled__changes_live_reload(
        self, tmp_path: Path
    ) -> None:
        """Override live_reload_enabled changes live_reload.enabled."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")
        original = Config.load(config_file)
        assert original.live_reload.enabled is True

        result = original.with_overrides(live_reload_enabled=False)

        assert result.live_reload.enabled is False

    def test__multiple_overrides__changes_all_specified(self, tmp_path: Path) -> None:
        """Multiple overrides change all specified values."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")
        original = Config.load(config_file)

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

    def test__override_cache_enabled__changes_cache_enabled(
        self, tmp_path: Path
    ) -> None:
        """Override cache_enabled changes docs.cache_enabled."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")
        original = Config.load(config_file)
        assert original.docs.cache_enabled is True

        result = original.with_overrides(cache_enabled=False)

        assert result.docs.cache_enabled is False


class TestCacheEnabledConfigParsing:
    """Tests for docs.cache_enabled config parsing."""

    def test__cache_enabled_defaults_to_true(self, tmp_path: Path) -> None:
        """Cache enabled defaults to True when not specified."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")

        config = Config.load(config_file)

        assert config.docs.cache_enabled is True

    def test__cache_enabled_false__parses_correctly(self, tmp_path: Path) -> None:
        """Parse cache_enabled = false correctly."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[docs]
cache_enabled = false
""")

        config = Config.load(config_file)

        assert config.docs.cache_enabled is False

    def test__cache_enabled_true__parses_correctly(self, tmp_path: Path) -> None:
        """Parse cache_enabled = true correctly."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[docs]
cache_enabled = true
""")

        config = Config.load(config_file)

        assert config.docs.cache_enabled is True

    def test__invalid_cache_enabled_type__raises_error(self, tmp_path: Path) -> None:
        """Raise ValueError when cache_enabled is not a boolean."""
        config_file = tmp_path / "docstage.toml"
        config_file.write_text("""
[docs]
cache_enabled = "yes"
""")

        with pytest.raises(ValueError, match="TOML parse error"):
            Config.load(config_file)
