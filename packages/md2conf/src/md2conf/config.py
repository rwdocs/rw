"""Configuration management for md2conf.

Supports TOML configuration format for Confluence credentials.
Adapted from adrflow project.
"""

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import NotRequired, TypedDict, cast


class ConfluenceConfigDict(TypedDict):
    """Confluence configuration dictionary from TOML."""

    base_url: str
    access_token: str
    access_secret: str
    consumer_key: NotRequired[str]


class TestConfigDict(TypedDict):
    """Test configuration dictionary from TOML."""

    space_key: str


class ConfigDict(TypedDict):
    """Configuration dictionary from TOML."""

    confluence: ConfluenceConfigDict
    test: NotRequired[TestConfigDict]


@dataclass
class ConfluenceConfig:
    """Confluence configuration."""

    base_url: str
    access_token: str
    access_secret: str
    consumer_key: str = 'adrflow'


@dataclass
class TestConfig:
    """Test configuration."""

    space_key: str


@dataclass
class Config:
    """Application configuration."""

    confluence: ConfluenceConfig
    test: TestConfig | None

    @classmethod
    def from_toml(cls, path: str | Path) -> 'Config':
        """Load configuration from TOML file.

        Args:
            path: Path to TOML configuration file

        Returns:
            Config instance

        Raises:
            FileNotFoundError: If configuration file doesn't exist
            ValueError: If configuration is invalid
        """
        config_path = Path(path)
        if not config_path.exists():
            raise FileNotFoundError(f'Configuration file not found: {path}')

        with config_path.open('rb') as f:
            data = tomllib.load(f)

        # Validate and cast to ConfigDict
        validated_data = cls._validate_config(data)
        return cls._from_dict(validated_data)

    @classmethod
    def _validate_config(cls, data: object) -> ConfigDict:
        """Validate configuration dictionary structure.

        Args:
            data: Raw configuration data from TOML

        Returns:
            Validated ConfigDict

        Raises:
            ValueError: If configuration structure is invalid
        """
        if not isinstance(data, dict):
            raise ValueError('Configuration must be a dictionary')

        # Validate confluence section
        confluence = data.get('confluence')
        if not isinstance(confluence, dict):
            raise ValueError('confluence section is required and must be a dictionary')
        if not isinstance(confluence.get('base_url'), str):
            raise ValueError('confluence.base_url must be a string')
        if not isinstance(confluence.get('access_token'), str):
            raise ValueError('confluence.access_token must be a string')
        if not isinstance(confluence.get('access_secret'), str):
            raise ValueError('confluence.access_secret must be a string')

        # Validate test section (optional)
        test = data.get('test')
        if test is not None:
            if not isinstance(test, dict):
                raise ValueError('test section must be a dictionary if provided')
            if not isinstance(test.get('space_key'), str):
                raise ValueError('test.space_key must be a string')

        # Safe to cast after validation
        return cast(ConfigDict, data)

    @classmethod
    def _from_dict(cls, data: ConfigDict) -> 'Config':
        """Create Config from typed dictionary.

        Args:
            data: Configuration dictionary (validated by TypedDict)

        Returns:
            Config instance
        """
        confluence_data = data['confluence']
        confluence = ConfluenceConfig(
            base_url=confluence_data['base_url'],
            access_token=confluence_data['access_token'],
            access_secret=confluence_data['access_secret'],
            consumer_key=confluence_data.get('consumer_key', 'adrflow'),
        )

        test = None
        test_data = data.get('test')
        if test_data is not None:
            test = TestConfig(space_key=test_data['space_key'])

        return cls(confluence=confluence, test=test)
