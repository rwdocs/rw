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


@cli.command()
@click.argument('page_id')
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
def get_page(page_id: str, config: Path, key_file: Path) -> None:
    """Get page information by ID."""
    asyncio.run(_get_page(page_id, config, key_file))


@cli.command()
@click.argument('title')
@click.option(
    '--space',
    '-s',
    help='Space key (default: from config.toml test.space_key)',
)
@click.option(
    '--body',
    '-b',
    default='<p>Test page created by md2conf</p>',
    help='Page body HTML',
)
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
def test_create(
    title: str, space: str | None, body: str, config: Path, key_file: Path
) -> None:
    """Test creating a page."""
    asyncio.run(_test_create(title, space, body, config, key_file))


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
            config.confluence.consumer_key,
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


async def _get_page(page_id: str, config_path: Path, key_file: Path) -> None:
    """Get page information.

    Args:
        page_id: Page ID to fetch
        config_path: Path to config.toml
        key_file: Path to private key PEM file
    """
    try:
        from md2conf.config import Config
        from md2conf.confluence import ConfluenceClient
        from md2conf.oauth import create_confluence_client, read_private_key

        # Load configuration
        config = Config.from_toml(config_path)
        private_key = read_private_key(key_file)

        # Create authenticated client
        async with create_confluence_client(
            config.confluence.access_token,
            config.confluence.access_secret,
            private_key,
            config.confluence.consumer_key,
        ) as http_client:
            confluence = ConfluenceClient(http_client, config.confluence.base_url)

            # Get page
            click.echo(f'Fetching page {page_id}...')
            page = await confluence.get_page(page_id, expand=['body.storage', 'version'])

            # Display info
            click.echo(click.style('\nPage Information:', fg='green', bold=True))
            click.echo(f'ID: {page["id"]}')
            click.echo(f'Title: {page["title"]}')
            click.echo(f'Version: {page["version"]["number"]}')

            # Get URL
            url = await confluence.get_page_url(page_id)
            click.echo(f'URL: {url}')

            # Get comments
            comments = await confluence.get_comments(page_id)
            click.echo(f'\nComments: {comments["size"]}')

    except Exception as e:
        click.echo(click.style(f'Error: {e}', fg='red'), err=True)
        sys.exit(1)


async def _test_create(
    title: str,
    space: str | None,
    body: str,
    config_path: Path,
    key_file: Path,
) -> None:
    """Test creating a page.

    Args:
        title: Page title
        space: Space key (or None to use config)
        body: Page body HTML
        config_path: Path to config.toml
        key_file: Path to private key PEM file
    """
    try:
        from md2conf.config import Config
        from md2conf.confluence import ConfluenceClient
        from md2conf.oauth import create_confluence_client, read_private_key

        # Load configuration
        config = Config.from_toml(config_path)
        private_key = read_private_key(key_file)

        # Determine space key
        if not space:
            if not config.test or not config.test.space_key:
                click.echo(
                    click.style(
                        'Error: No space key provided and test.space_key not in config',
                        fg='red',
                    ),
                    err=True,
                )
                sys.exit(1)
            space = config.test.space_key

        # Create authenticated client
        async with create_confluence_client(
            config.confluence.access_token,
            config.confluence.access_secret,
            private_key,
            config.confluence.consumer_key,
        ) as http_client:
            confluence = ConfluenceClient(http_client, config.confluence.base_url)

            # Create page
            click.echo(f'Creating page "{title}" in space {space}...')
            page = await confluence.create_page(space, title, body)

            # Display result
            click.echo(click.style('\nPage created successfully!', fg='green', bold=True))
            click.echo(f'ID: {page["id"]}')
            click.echo(f'Title: {page["title"]}')
            click.echo(f'Version: {page["version"]["number"]}')

            # Get URL
            url = await confluence.get_page_url(page['id'])
            click.echo(f'URL: {url}')

    except Exception as e:
        click.echo(click.style(f'Error: {e}', fg='red'), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


if __name__ == '__main__':
    cli()
