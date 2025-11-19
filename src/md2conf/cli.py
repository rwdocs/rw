"""CLI interface for md2conf.

Command-line tool for converting markdown to Confluence pages.
"""

import asyncio
import sys
from pathlib import Path

import click


@click.group()
def cli() -> None:
    """Convert markdown files to Confluence pages."""
    pass


@cli.command()
@click.option(
    '--config',
    '-c',
    type=click.Path(exists=True, path_type=Path),
    default='config.toml',
    help='Path to configuration file',
)
@click.option(
    '--key-file',
    '-k',
    type=click.Path(exists=True, path_type=Path),
    default='private_key.pem',
    help='Path to OAuth private key file',
)
def test_auth(config: Path, key_file: Path) -> None:
    """Test Confluence authentication."""
    asyncio.run(_test_auth(config, key_file))


async def _test_auth(config_path: Path, key_file: Path) -> None:
    """Test authentication with Confluence API.

    Args:
        config_path: Path to config.toml
        key_file: Path to private key PEM file
    """
    try:
        from md2conf.config import Config
        from md2conf.oauth import create_confluence_client, read_private_key

        # Load configuration
        click.echo(f'Loading config from {config_path}...')
        config = Config.from_toml(config_path)

        # Read private key
        click.echo(f'Reading private key from {key_file}...')
        private_key = read_private_key(key_file)

        # Create authenticated client
        click.echo('Creating authenticated client...')
        async with create_confluence_client(
            config.confluence.access_token,
            config.confluence.access_secret,
            private_key,
        ) as client:
            # Test API call - get current user info
            base_url = config.confluence.base_url
            click.echo(f'Testing connection to {base_url}...')

            response = await client.get(f'{base_url}/rest/api/user/current')
            response.raise_for_status()

            user_data = response.json()
            username = user_data.get('username', user_data.get('displayName', 'Unknown'))

            click.echo(click.style('Authentication successful!', fg='green'))
            click.echo(f'Authenticated as: {username}')

    except FileNotFoundError as e:
        click.echo(click.style(f'Error: {e}', fg='red'), err=True)
        click.echo('\nMake sure you have:')
        click.echo('1. Copied config.toml.example to config.toml')
        click.echo('2. Filled in your OAuth credentials')
        click.echo('3. Placed your private_key.pem file in the project root')
        sys.exit(1)
    except Exception as e:
        click.echo(click.style(f'Authentication failed: {e}', fg='red'), err=True)
        sys.exit(1)


if __name__ == '__main__':
    cli()
