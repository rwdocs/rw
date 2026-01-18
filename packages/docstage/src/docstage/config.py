"""Configuration management for Docstage.

This module re-exports configuration types from the Rust docstage_core.config module.
All configuration logic, including CLI overrides, is handled in Rust.
"""

from docstage_core.config import (
    Config,
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
