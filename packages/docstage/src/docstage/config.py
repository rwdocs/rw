"""Configuration management for Docstage.

This module provides a thin Python wrapper around the Rust config parser,
adding CLI override functionality.
"""

from dataclasses import dataclass
from pathlib import Path

from docstage_core.config import (
    Config as RustConfig,
    ConfluenceConfig,
    ConfluenceTestConfig,
    DiagramsConfig,
    DocsConfig,
    LiveReloadConfig,
    ServerConfig,
)

__all__ = [
    "Config",
    "ConfluenceConfig",
    "ConfluenceTestConfig",
    "DiagramsConfig",
    "DocsConfig",
    "LiveReloadConfig",
    "ServerConfig",
]


@dataclass
class _OverriddenServerConfig:
    """Server config with overridden values."""

    host: str
    port: int


@dataclass
class _OverriddenDocsConfig:
    """Docs config with overridden values."""

    source_dir: Path
    cache_dir: Path
    cache_enabled: bool


@dataclass
class _OverriddenDiagramsConfig:
    """Diagrams config with overridden values."""

    kroki_url: str | None
    include_dirs: list[Path]
    config_file: str | None
    dpi: int


@dataclass
class _OverriddenLiveReloadConfig:
    """Live reload config with overridden values."""

    enabled: bool
    watch_patterns: list[str] | None


@dataclass
class Config:
    """Application configuration with CLI override support.

    This is a thin wrapper around the Rust config that adds the ability
    to apply CLI overrides via `with_overrides()`.
    """

    server: ServerConfig | _OverriddenServerConfig
    docs: DocsConfig | _OverriddenDocsConfig
    diagrams: DiagramsConfig | _OverriddenDiagramsConfig
    live_reload: LiveReloadConfig | _OverriddenLiveReloadConfig
    confluence: ConfluenceConfig | None
    confluence_test: ConfluenceTestConfig | None
    config_path: Path | None

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
        rust_config = RustConfig.load(config_path)
        return cls(
            server=rust_config.server,
            docs=rust_config.docs,
            diagrams=rust_config.diagrams,
            live_reload=rust_config.live_reload,
            confluence=rust_config.confluence,
            confluence_test=rust_config.confluence_test,
            config_path=rust_config.config_path,
        )

    def with_overrides(
        self,
        *,
        host: str | None = None,
        port: int | None = None,
        source_dir: Path | None = None,
        cache_dir: Path | None = None,
        cache_enabled: bool | None = None,
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
            cache_enabled: Override docs.cache_enabled
            kroki_url: Override diagrams.kroki_url
            live_reload_enabled: Override live_reload.enabled

        Returns:
            New Config instance with overrides applied
        """
        return Config(
            server=_OverriddenServerConfig(
                host=host if host is not None else self.server.host,
                port=port if port is not None else self.server.port,
            ),
            docs=_OverriddenDocsConfig(
                source_dir=source_dir
                if source_dir is not None
                else self.docs.source_dir,
                cache_dir=cache_dir if cache_dir is not None else self.docs.cache_dir,
                cache_enabled=cache_enabled
                if cache_enabled is not None
                else self.docs.cache_enabled,
            ),
            diagrams=_OverriddenDiagramsConfig(
                kroki_url=kroki_url
                if kroki_url is not None
                else self.diagrams.kroki_url,
                include_dirs=list(self.diagrams.include_dirs),
                config_file=self.diagrams.config_file,
                dpi=self.diagrams.dpi,
            ),
            live_reload=_OverriddenLiveReloadConfig(
                enabled=live_reload_enabled
                if live_reload_enabled is not None
                else self.live_reload.enabled,
                watch_patterns=self.live_reload.watch_patterns,
            ),
            confluence=self.confluence,
            confluence_test=self.confluence_test,
            config_path=self.config_path,
        )
