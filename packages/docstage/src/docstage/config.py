"""Configuration management for Docstage.

Supports TOML configuration format with auto-discovery.
"""

import tomllib
from dataclasses import dataclass, field, replace
from pathlib import Path

CONFIG_FILENAME = "docstage.toml"


@dataclass
class ServerConfig:
    """Server configuration."""

    host: str = "127.0.0.1"
    port: int = 8080


@dataclass
class DocsConfig:
    """Documentation configuration."""

    source_dir: Path = field(default_factory=lambda: Path("docs"))
    cache_dir: Path = field(default_factory=lambda: Path(".cache"))


@dataclass
class DiagramsConfig:
    """Diagram rendering configuration."""

    kroki_url: str | None = None
    include_dirs: list[Path] = field(default_factory=list)
    config_file: str | None = None
    dpi: int = 192


@dataclass
class ConfluenceConfig:
    """Confluence configuration."""

    base_url: str
    access_token: str
    access_secret: str
    consumer_key: str = "docstage"


@dataclass
class LiveReloadConfig:
    """Live reload configuration."""

    enabled: bool = True
    watch_patterns: list[str] | None = None


@dataclass
class ConfluenceTestConfig:
    """Confluence test configuration."""

    space_key: str


@dataclass
class Config:
    """Application configuration."""

    server: ServerConfig
    docs: DocsConfig
    diagrams: DiagramsConfig
    live_reload: LiveReloadConfig
    confluence: ConfluenceConfig | None
    confluence_test: ConfluenceTestConfig | None
    config_path: Path | None = None

    @classmethod
    def load(cls, config_path: Path | None = None) -> Config:
        """Load configuration from file.

        If config_path is provided, loads from that file.
        Otherwise, searches for docstage.toml in current directory and parents.

        Args:
            config_path: Optional explicit path to config file

        Returns:
            Config instance with defaults for missing sections

        Raises:
            FileNotFoundError: If explicit config_path doesn't exist
            ValueError: If configuration is invalid
        """
        if config_path is not None:
            if not config_path.exists():
                raise FileNotFoundError(f"Configuration file not found: {config_path}")
            return cls._load_from_file(config_path)

        discovered_path = cls._discover_config()
        if discovered_path is None:
            return cls._default()

        return cls._load_from_file(discovered_path)

    @classmethod
    def _discover_config(cls) -> Path | None:
        """Search for config file in current directory and parents.

        Returns:
            Path to config file or None if not found
        """
        current = Path.cwd()
        while True:
            candidate = current / CONFIG_FILENAME
            if candidate.exists():
                return candidate
            parent = current.parent
            if parent == current:
                return None
            current = parent

    @classmethod
    def _default(cls) -> Config:
        """Create config with all defaults.

        Returns:
            Config instance with default values
        """
        return cls(
            server=ServerConfig(),
            docs=DocsConfig(),
            diagrams=DiagramsConfig(),
            live_reload=LiveReloadConfig(),
            confluence=None,
            confluence_test=None,
        )

    @classmethod
    def _load_from_file(cls, path: Path) -> Config:
        """Load configuration from a specific file.

        Args:
            path: Path to TOML configuration file

        Returns:
            Config instance

        Raises:
            ValueError: If configuration is invalid
        """
        with path.open("rb") as f:
            data = tomllib.load(f)

        if not isinstance(data, dict):
            raise ValueError("Configuration must be a dictionary")

        config_dir = path.parent

        server = cls._parse_server(data.get("server"))
        docs = cls._parse_docs(data.get("docs"), config_dir)
        diagrams = cls._parse_diagrams(data.get("diagrams"), config_dir)
        live_reload = cls._parse_live_reload(data.get("live_reload"))
        confluence = cls._parse_confluence(data.get("confluence"))
        confluence_test = cls._parse_confluence_test(
            data.get("confluence", {}).get("test"),
        )

        return cls(
            server=server,
            docs=docs,
            diagrams=diagrams,
            live_reload=live_reload,
            confluence=confluence,
            confluence_test=confluence_test,
            config_path=path,
        )

    @classmethod
    def _parse_server(cls, data: object) -> ServerConfig:
        """Parse server configuration section.

        Args:
            data: Raw server section data

        Returns:
            ServerConfig instance
        """
        if data is None:
            return ServerConfig()

        if not isinstance(data, dict):
            raise ValueError("server section must be a dictionary")

        host = data.get("host", "127.0.0.1")
        if not isinstance(host, str):
            raise ValueError("server.host must be a string")

        port = data.get("port", 8080)
        if not isinstance(port, int):
            raise ValueError("server.port must be an integer")

        return ServerConfig(host=host, port=port)

    @classmethod
    def _parse_docs(cls, data: object, config_dir: Path) -> DocsConfig:
        """Parse docs configuration section.

        Args:
            data: Raw docs section data
            config_dir: Directory containing config file (for relative paths)

        Returns:
            DocsConfig instance
        """
        if data is None:
            return DocsConfig(
                source_dir=config_dir / "docs",
                cache_dir=config_dir / ".cache",
            )

        if not isinstance(data, dict):
            raise ValueError("docs section must be a dictionary")

        source_dir = data.get("source_dir", "docs")
        if not isinstance(source_dir, str):
            raise ValueError("docs.source_dir must be a string")
        source_path = config_dir / source_dir

        cache_dir = data.get("cache_dir", ".cache")
        if not isinstance(cache_dir, str):
            raise ValueError("docs.cache_dir must be a string")
        cache_path = config_dir / cache_dir

        return DocsConfig(source_dir=source_path, cache_dir=cache_path)

    @classmethod
    def _parse_diagrams(cls, data: object, config_dir: Path) -> DiagramsConfig:
        """Parse diagrams configuration section.

        Args:
            data: Raw diagrams section data
            config_dir: Directory containing config file (for relative paths)

        Returns:
            DiagramsConfig instance
        """
        if data is None:
            return DiagramsConfig()

        if not isinstance(data, dict):
            raise ValueError("diagrams section must be a dictionary")

        kroki_url = data.get("kroki_url")
        if kroki_url is not None and not isinstance(kroki_url, str):
            raise ValueError("diagrams.kroki_url must be a string")

        include_dirs_raw = data.get("include_dirs", [])
        if not isinstance(include_dirs_raw, list):
            raise ValueError("diagrams.include_dirs must be a list")
        include_dirs: list[Path] = []
        for item in include_dirs_raw:
            if not isinstance(item, str):
                raise ValueError("diagrams.include_dirs items must be strings")
            include_dirs.append(config_dir / item)

        config_file = data.get("config_file")
        if config_file is not None and not isinstance(config_file, str):
            raise ValueError("diagrams.config_file must be a string")

        dpi = data.get("dpi", 192)
        if not isinstance(dpi, int):
            raise ValueError("diagrams.dpi must be an integer")

        return DiagramsConfig(
            kroki_url=kroki_url,
            include_dirs=include_dirs,
            config_file=config_file,
            dpi=dpi,
        )

    @classmethod
    def _parse_live_reload(cls, data: object) -> LiveReloadConfig:
        """Parse live_reload configuration section.

        Args:
            data: Raw live_reload section data

        Returns:
            LiveReloadConfig instance
        """
        if data is None:
            return LiveReloadConfig()

        if not isinstance(data, dict):
            raise ValueError("live_reload section must be a dictionary")

        enabled = data.get("enabled", True)
        if not isinstance(enabled, bool):
            raise ValueError("live_reload.enabled must be a boolean")

        watch_patterns_raw = data.get("watch_patterns")
        watch_patterns: list[str] | None = None
        if watch_patterns_raw is not None:
            if not isinstance(watch_patterns_raw, list):
                raise ValueError("live_reload.watch_patterns must be a list")
            watch_patterns = []
            for item in watch_patterns_raw:
                if not isinstance(item, str):
                    raise ValueError("live_reload.watch_patterns items must be strings")
                watch_patterns.append(item)

        return LiveReloadConfig(enabled=enabled, watch_patterns=watch_patterns)

    @classmethod
    def _parse_confluence(cls, data: object) -> ConfluenceConfig | None:
        """Parse confluence configuration section.

        Args:
            data: Raw confluence section data

        Returns:
            ConfluenceConfig instance or None if section not present
        """
        if data is None:
            return None

        if not isinstance(data, dict):
            raise ValueError("confluence section must be a dictionary")

        base_url = data.get("base_url")
        if base_url is None:
            return None
        if not isinstance(base_url, str):
            raise ValueError("confluence.base_url must be a string")

        access_token = data.get("access_token")
        if not isinstance(access_token, str):
            raise ValueError("confluence.access_token must be a string")

        access_secret = data.get("access_secret")
        if not isinstance(access_secret, str):
            raise ValueError("confluence.access_secret must be a string")

        consumer_key = data.get("consumer_key", "docstage")
        if not isinstance(consumer_key, str):
            raise ValueError("confluence.consumer_key must be a string")

        return ConfluenceConfig(
            base_url=base_url,
            access_token=access_token,
            access_secret=access_secret,
            consumer_key=consumer_key,
        )

    @classmethod
    def _parse_confluence_test(cls, data: object) -> ConfluenceTestConfig | None:
        """Parse confluence.test configuration section.

        Args:
            data: Raw confluence.test section data

        Returns:
            ConfluenceTestConfig instance or None if section not present
        """
        if data is None:
            return None

        if not isinstance(data, dict):
            raise ValueError("confluence.test section must be a dictionary")

        space_key = data.get("space_key")
        if not isinstance(space_key, str):
            raise ValueError("confluence.test.space_key must be a string")

        return ConfluenceTestConfig(space_key=space_key)

    def with_overrides(
        self,
        *,
        host: str | None = None,
        port: int | None = None,
        source_dir: Path | None = None,
        cache_dir: Path | None = None,
        kroki_url: str | None = None,
        live_reload_enabled: bool | None = None,
    ) -> Config:
        """Create a new Config with CLI overrides applied.

        Only non-None values override the existing config. This follows
        the immutable pattern - the original Config is not modified.

        Args:
            host: Override server.host
            port: Override server.port
            source_dir: Override docs.source_dir
            cache_dir: Override docs.cache_dir
            kroki_url: Override diagrams.kroki_url
            live_reload_enabled: Override live_reload.enabled

        Returns:
            New Config instance with overrides applied
        """
        server = self.server
        if host is not None or port is not None:
            server = replace(
                self.server,
                host=host if host is not None else self.server.host,
                port=port if port is not None else self.server.port,
            )

        docs = self.docs
        if source_dir is not None or cache_dir is not None:
            docs = replace(
                self.docs,
                source_dir=source_dir if source_dir is not None else self.docs.source_dir,
                cache_dir=cache_dir if cache_dir is not None else self.docs.cache_dir,
            )

        diagrams = self.diagrams
        if kroki_url is not None:
            diagrams = replace(self.diagrams, kroki_url=kroki_url)

        live_reload = self.live_reload
        if live_reload_enabled is not None:
            live_reload = replace(self.live_reload, enabled=live_reload_enabled)

        return replace(
            self,
            server=server,
            docs=docs,
            diagrams=diagrams,
            live_reload=live_reload,
        )
