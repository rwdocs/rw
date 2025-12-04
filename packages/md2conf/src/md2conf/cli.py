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


@cli.command()
@click.argument('markdown_file', type=click.Path(exists=True, path_type=Path))
def convert(markdown_file: Path) -> None:
    """Convert a markdown file to Confluence storage format and display it."""
    from md2conf.confluence import MarkdownConverter

    converter = MarkdownConverter()
    confluence_html = converter.convert_file(markdown_file)

    click.echo(click.style('\nMarkdown file:', fg='cyan', bold=True))
    click.echo(f'{markdown_file}\n')
    click.echo(click.style('Converted to Confluence storage format:', fg='green', bold=True))
    click.echo(confluence_html)


@cli.command()
@click.argument('markdown_file', type=click.Path(exists=True, path_type=Path))
@click.argument('title')
@click.option(
    '--space',
    '-s',
    help='Space key (default: from config.toml test.space_key)',
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
def create(
    markdown_file: Path, title: str, space: str | None, config: Path, key_file: Path
) -> None:
    """Create a Confluence page from a markdown file."""
    asyncio.run(_create(markdown_file, title, space, config, key_file))


@cli.command()
@click.argument('markdown_file', type=click.Path(exists=True, path_type=Path))
@click.argument('page_id')
@click.option(
    '--message',
    '-m',
    help='Version message for the update',
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
def update(
    markdown_file: Path,
    page_id: str,
    message: str | None,
    config: Path,
    key_file: Path,
) -> None:
    """Update a Confluence page from a markdown file."""
    asyncio.run(_update(markdown_file, page_id, message, config, key_file))


@cli.command()
@click.argument('markdown_file', type=click.Path(exists=True, path_type=Path))
@click.argument('page_id')
@click.option(
    '--mkdocs-root',
    '-r',
    type=click.Path(exists=True, path_type=Path),
    required=True,
    help='Root directory of the MkDocs site',
)
@click.option(
    '--kroki-url',
    default='https://kroki.cian.tech',
    help='Kroki server URL for diagram rendering',
)
@click.option(
    '--message',
    '-m',
    help='Version message for the update',
)
@click.option(
    '--dry-run',
    is_flag=True,
    help='Preview changes without updating Confluence. Shows comments that would be lost.',
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
def upload_mkdocs(
    markdown_file: Path,
    page_id: str,
    mkdocs_root: Path,
    kroki_url: str,
    message: str | None,
    dry_run: bool,
    config: Path,
    key_file: Path,
) -> None:
    """Upload an MkDocs document with diagrams to Confluence.

    Renders PlantUML diagrams to PNG images and uploads them as attachments.
    """
    asyncio.run(
        _upload_mkdocs(
            markdown_file, page_id, mkdocs_root, kroki_url, message, dry_run, config, key_file
        )
    )


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
@click.option(
    '--include-resolved',
    is_flag=True,
    help='Include resolved comments in output',
)
def comments(
    page_id: str,
    config: Path,
    key_file: Path,
    include_resolved: bool,
) -> None:
    """Fetch and display comments from a Confluence page.

    Outputs comments in a format suitable for fixing issues in source markdown.
    """
    asyncio.run(_comments(page_id, config, key_file, include_resolved))


@cli.command()
@click.option(
    '--private-key',
    '-k',
    type=click.Path(exists=True, path_type=Path),
    default='private_key.pem',
    help='Path to RSA private key file',
)
@click.option(
    '--consumer-key',
    '-c',
    default='adrflow',
    help='OAuth consumer key (default: adrflow)',
)
@click.option(
    '--base-url',
    '-u',
    default='https://conf.cian.tech',
    help='Confluence base URL',
)
@click.option(
    '--port',
    '-p',
    default=8080,
    type=int,
    help='Local callback server port (default: 8080)',
)
def generate_tokens(
    private_key: Path, consumer_key: str, base_url: str, port: int
) -> None:
    """Generate OAuth access tokens for Confluence.

    This starts an interactive OAuth 1.0 flow to generate access tokens.
    You will need to authorize the application in your browser.
    """
    asyncio.run(_generate_tokens(private_key, consumer_key, base_url, port))


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

            # Display page content
            if 'body' in page and 'storage' in page['body']:
                content = page['body']['storage'].get('value', '')
                click.echo(click.style('\nPage Content (Confluence Storage Format):', fg='cyan', bold=True))
                click.echo(content)

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


async def _create(
    markdown_file: Path,
    title: str,
    space: str | None,
    config_path: Path,
    key_file: Path,
) -> None:
    """Create a page from markdown file.

    Args:
        markdown_file: Path to markdown file
        title: Page title
        space: Space key (or None to use config)
        config_path: Path to config.toml
        key_file: Path to private key PEM file
    """
    try:
        from md2conf.config import Config
        from md2conf.confluence import ConfluenceClient, MarkdownConverter
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

        # Convert markdown to Confluence format
        click.echo(f'Converting {markdown_file}...')
        converter = MarkdownConverter()
        confluence_body = converter.convert_file(markdown_file)

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
            page = await confluence.create_page(space, title, confluence_body)

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


async def _update(
    markdown_file: Path,
    page_id: str,
    message: str | None,
    config_path: Path,
    key_file: Path,
) -> None:
    """Update a page from markdown file.

    Args:
        markdown_file: Path to markdown file
        page_id: Page ID to update
        message: Optional version message
        config_path: Path to config.toml
        key_file: Path to private key PEM file
    """
    try:
        from md2conf.config import Config
        from md2conf.confluence import ConfluenceClient, MarkdownConverter
        from md2conf.confluence.comment_preservation import CommentPreserver
        from md2conf.oauth import create_confluence_client, read_private_key

        # Load configuration
        config = Config.from_toml(config_path)
        private_key = read_private_key(key_file)

        # Convert markdown to Confluence format
        click.echo(f'Converting {markdown_file}...')
        converter = MarkdownConverter()
        new_html = converter.convert_file(markdown_file)

        # Create authenticated client
        async with create_confluence_client(
            config.confluence.access_token,
            config.confluence.access_secret,
            private_key,
            config.confluence.consumer_key,
        ) as http_client:
            confluence = ConfluenceClient(http_client, config.confluence.base_url)

            # Get current page with body to preserve comments
            click.echo(f'Fetching current page {page_id}...')
            current_page = await confluence.get_page(
                page_id, expand=['body.storage', 'version']
            )
            current_version = current_page['version']['number']
            title = current_page['title']
            old_html = current_page['body']['storage']['value']

            # Preserve comment markers
            click.echo('Preserving comment markers...')
            preserver = CommentPreserver()
            preserve_result = preserver.preserve_comments(old_html, new_html)

            # Update page
            click.echo(
                f'Updating page "{title}" from version {current_version} to {current_version + 1}...'
            )
            updated_page = await confluence.update_page(
                page_id, title, preserve_result.html, current_version, message
            )

            # Display result
            click.echo(click.style('\nPage updated successfully!', fg='green', bold=True))
            click.echo(f'ID: {updated_page["id"]}')
            click.echo(f'Title: {updated_page["title"]}')
            click.echo(f'Version: {updated_page["version"]["number"]}')

            # Get URL
            url = await confluence.get_page_url(page_id)
            click.echo(f'URL: {url}')

            # Show comments count
            comments = await confluence.get_comments(page_id)
            click.echo(f'\nComments on page: {comments["size"]}')

            # Warn about unmatched comments
            if preserve_result.unmatched_comments:
                click.echo(
                    click.style(
                        f'\nWarning: {len(preserve_result.unmatched_comments)} comment(s) could not be placed:',
                        fg='yellow',
                    )
                )
                for comment in preserve_result.unmatched_comments:
                    click.echo(f'  - [{comment.ref}] "{comment.text}"')

    except Exception as e:
        click.echo(click.style(f'Error: {e}', fg='red'), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


async def _upload_mkdocs(
    markdown_file: Path,
    page_id: str,
    mkdocs_root: Path,
    kroki_url: str,
    message: str | None,
    dry_run: bool,
    config_path: Path,
    key_file: Path,
) -> None:
    """Upload an MkDocs document with diagrams to Confluence.

    Args:
        markdown_file: Path to markdown file
        page_id: Page ID to update
        mkdocs_root: Root directory of the MkDocs site
        kroki_url: Kroki server URL
        message: Optional version message
        dry_run: If True, only show what would happen without updating
        config_path: Path to config.toml
        key_file: Path to private key PEM file
    """
    import tempfile

    try:
        from md2conf.config import Config
        from md2conf.confluence import ConfluenceClient
        from md2conf.confluence.comment_preservation import CommentPreserver
        from md2conf_core import MarkdownConverter
        from md2conf.oauth import create_confluence_client, read_private_key

        # Load configuration
        config = Config.from_toml(config_path)
        private_key = read_private_key(key_file)

        # Build include directories from mkdocs root
        include_dirs = [
            mkdocs_root / 'includes',
            mkdocs_root / 'gen' / 'includes',
            mkdocs_root,
        ]
        include_dirs = [d for d in include_dirs if d.exists()]

        click.echo(f'Include directories: {[str(d) for d in include_dirs]}')

        # Read markdown file and convert to Confluence format with diagrams
        click.echo(f'Converting {markdown_file}...')
        markdown_text = markdown_file.read_text(encoding='utf-8')
        converter = MarkdownConverter(
            prepend_toc=True,
            extract_title=True,
            include_dirs=include_dirs,
            config_file='config.iuml',
        )

        # Use temp directory for diagram files
        with tempfile.TemporaryDirectory() as tmpdir:
            click.echo(f'Rendering diagrams via Kroki ({kroki_url})...')
            result = converter.convert_with_diagrams(markdown_text, kroki_url, Path(tmpdir))
            new_html = result.html

            click.echo(f'Rendered {len(result.diagrams)} diagrams')

            if result.title:
                click.echo(f'Title: {result.title}')

            # Collect attachment data from rendered diagrams
            attachment_data: list[tuple[str, bytes]] = []
            for diagram in result.diagrams:
                display_width = diagram.width // 2
                filepath = Path(tmpdir) / diagram.filename
                attachment_data.append((diagram.filename, filepath.read_bytes()))
                click.echo(
                    f'  -> {diagram.filename} ({diagram.width}x{diagram.height} -> {display_width}px)'
                )

            # Create authenticated client
            async with create_confluence_client(
                config.confluence.access_token,
                config.confluence.access_secret,
                private_key,
                config.confluence.consumer_key,
            ) as http_client:
                confluence = ConfluenceClient(http_client, config.confluence.base_url)

                # Get current page with body to preserve comments
                click.echo(f'Fetching current page {page_id}...')
                current_page = await confluence.get_page(
                    page_id, expand=['body.storage', 'version']
                )
                current_version = current_page['version']['number']
                old_html = current_page['body']['storage']['value']

                # Use extracted title or fall back to current page title
                title = result.title or current_page['title']

                # Preserve comment markers
                click.echo('Preserving comment markers...')
                preserver = CommentPreserver()
                preserve_result = preserver.preserve_comments(old_html, new_html)

                if dry_run:
                    click.echo(click.style('\n[DRY RUN] No changes made to Confluence.', fg='cyan', bold=True))
                    if preserve_result.unmatched_comments:
                        click.echo(
                            click.style(
                                f'\nComments that would be resolved ({len(preserve_result.unmatched_comments)}):',
                                fg='yellow',
                                bold=True,
                            )
                        )
                        for comment in preserve_result.unmatched_comments:
                            click.echo(f'  - [{comment.ref}] "{comment.text}"')
                    else:
                        click.echo(click.style('\nNo comments would be resolved.', fg='green'))
                    return

                # Upload attachments
                if attachment_data:
                    click.echo(f'Uploading {len(attachment_data)} attachments...')
                    for filename, image_data in attachment_data:
                        click.echo(f'  Uploading {filename}...')
                        await confluence.upload_attachment(
                            page_id, filename, image_data, "image/png"
                        )

                # Update page
                click.echo(
                    f'Updating page "{title}" from version {current_version} to {current_version + 1}...'
                )
                updated_page = await confluence.update_page(
                    page_id, title, preserve_result.html, current_version, message
                )

                # Display result
                click.echo(click.style('\nPage updated successfully!', fg='green', bold=True))
                click.echo(f'ID: {updated_page["id"]}')
                click.echo(f'Title: {updated_page["title"]}')
                click.echo(f'Version: {updated_page["version"]["number"]}')

                # Get URL
                url = await confluence.get_page_url(page_id)
                click.echo(f'URL: {url}')

                # Warn about unmatched comments
                if preserve_result.unmatched_comments:
                    click.echo(
                        click.style(
                            f'\nWarning: {len(preserve_result.unmatched_comments)} comment(s) could not be placed:',
                            fg='yellow',
                        )
                    )
                    for comment in preserve_result.unmatched_comments:
                        click.echo(f'  - [{comment.ref}] "{comment.text}"')

    except Exception as e:
        click.echo(click.style(f'Error: {e}', fg='red'), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


async def _generate_tokens(
    private_key_path: Path, consumer_key: str, base_url: str, port: int = 8080
) -> None:
    """Generate OAuth access tokens through interactive flow.

    Args:
        private_key_path: Path to RSA private key
        consumer_key: OAuth consumer key
        base_url: Confluence base URL
        port: Local callback server port
    """
    try:
        from http.server import BaseHTTPRequestHandler, HTTPServer
        from urllib.parse import parse_qs, urlparse

        from authlib.integrations.httpx_client import AsyncOAuth1Client

        from md2conf.oauth import read_private_key

        # OAuth callback handler
        class OAuthCallbackHandler(BaseHTTPRequestHandler):
            """HTTP handler for OAuth callback."""

            oauth_result: dict[str, str] = {}

            def do_GET(self) -> None:  # noqa: N802
                """Handle GET request for OAuth callback."""
                parsed_path = urlparse(self.path)
                query_params = parse_qs(parsed_path.query)

                # Log the request for debugging
                print(f'\nReceived callback: {self.path}')
                print(f'Query params: {query_params}')

                oauth_token = query_params.get('oauth_token', [''])[0]
                oauth_verifier = query_params.get('oauth_verifier', [''])[0]

                if oauth_token and oauth_verifier:
                    OAuthCallbackHandler.oauth_result = {
                        'oauth_token': oauth_token,
                        'oauth_verifier': oauth_verifier,
                    }
                    self.send_response(200)
                    self.send_header('Content-type', 'text/html')
                    self.end_headers()
                    self.wfile.write(
                        b'<html><body>'
                        b'<h1>Authorization successful!</h1>'
                        b'<p>You can close this window and return to the terminal.</p>'
                        b'</body></html>'
                    )
                else:
                    self.send_response(400)
                    self.send_header('Content-type', 'text/html')
                    self.end_headers()
                    self.wfile.write(
                        b'<html><body><h1>Authorization failed!</h1></body></html>'
                    )

            def log_message(self, format: str, *args: object) -> None:  # noqa: A002
                """Suppress HTTP server log messages."""
                pass

        # Read private key
        click.echo(f'Reading private key from {private_key_path}...')
        private_key = read_private_key(private_key_path)

        # Confluence OAuth endpoints
        base_url = base_url.rstrip('/')
        endpoints = {
            'request_token_url': f'{base_url}/plugins/servlet/oauth/request-token',
            'authorize_url': f'{base_url}/plugins/servlet/oauth/authorize',
            'access_token_url': f'{base_url}/plugins/servlet/oauth/access-token',
        }

        # Create OAuth client
        click.echo(f'Creating OAuth client for consumer key: {consumer_key}')
        client = AsyncOAuth1Client(
            client_id=consumer_key,
            rsa_key=private_key.decode('utf-8'),  # Use rsa_key parameter for RSA-SHA1
            signature_method='RSA-SHA1',
        )

        try:
            # Step 1: Request temporary token with 'oob' callback
            click.echo('\nStep 1: Requesting temporary credentials...')
            # Use 'oob' (out-of-band) to get verification code displayed on page
            # See: https://developer.atlassian.com/server/jira/platform/oauth/
            client.redirect_uri = 'oob'
            response = await client.fetch_request_token(
                endpoints['request_token_url']
            )
            oauth_token = response.get('oauth_token')
            oauth_token_secret = response.get('oauth_token_secret')

            if not oauth_token or not oauth_token_secret:
                click.echo(click.style('Failed to get request token', fg='red'), err=True)
                sys.exit(1)

            click.echo(click.style('✓ Temporary token received', fg='green'))

            # Step 2: Build authorization URL
            auth_url = f'{endpoints["authorize_url"]}?oauth_token={oauth_token}'

            click.echo('\n' + '=' * 70)
            click.echo(click.style('Step 2: Authorization Required', fg='cyan', bold=True))
            click.echo('=' * 70)
            click.echo('\nPlease open this URL in your browser to authorize the application:')
            click.echo(click.style(f'\n{auth_url}\n', fg='cyan', bold=True))
            click.echo('\nAfter clicking "Allow" in Confluence:')
            click.echo('  - Confluence will display a VERIFICATION CODE on the page')
            click.echo('  - Copy that code and paste it below')
            click.echo('=' * 70)

            # Step 3: Get verification code from user (OOB flow)
            click.echo('\nStep 3: Enter the verification code...')
            click.echo('After authorizing, Confluence should display a verification code.')
            oauth_verifier = click.prompt('Enter the verification code', type=str).strip()

            if not oauth_verifier:
                click.echo(click.style('Error: Verification code is required', fg='red'), err=True)
                sys.exit(1)

            click.echo(click.style('✓ Verification code received', fg='green'))

            # Step 4: Exchange for access token
            click.echo('\nStep 4: Exchanging for access token...')
            client.token = {
                'oauth_token': oauth_token,
                'oauth_token_secret': oauth_token_secret,
            }
            response = await client.fetch_access_token(
                endpoints['access_token_url'], verifier=oauth_verifier
            )

            access_token = response.get('oauth_token')
            access_secret = response.get('oauth_token_secret')

            if not access_token or not access_secret:
                click.echo(
                    click.style('Failed to get access token', fg='red'), err=True
                )
                sys.exit(1)

            # Success!
            click.echo('\n' + '=' * 70)
            click.echo(click.style('✓ OAuth Authorization Successful!', fg='green', bold=True))
            click.echo('=' * 70)
            click.echo('\nAdd these credentials to your config.toml:')
            click.echo('\n[confluence]')
            click.echo(f'base_url = "{base_url}"')
            click.echo(f'access_token = "{access_token}"')
            click.echo(f'access_secret = "{access_secret}"')
            click.echo(f'consumer_key = "{consumer_key}"')
            click.echo('\n' + '=' * 70)
            click.echo(
                click.style(
                    '\nNote: These tokens inherit YOUR permissions in Confluence.',
                    fg='yellow',
                )
            )
            click.echo(
                'If you can create/edit pages, these tokens will have write access.'
            )
            click.echo('=' * 70 + '\n')

        except Exception as e:
            click.echo(click.style(f'OAuth flow failed: {e}', fg='red'), err=True)
            import traceback

            traceback.print_exc()
            sys.exit(1)
        finally:
            await client.aclose()

    except Exception as e:
        click.echo(click.style(f'Error: {e}', fg='red'), err=True)
        sys.exit(1)


def _strip_html_tags(html: str) -> str:
    """Strip HTML tags and convert to plain text.

    Args:
        html: HTML string to convert

    Returns:
        Plain text with HTML tags removed
    """
    import re

    # Remove HTML tags
    text = re.sub(r'<[^>]+>', ' ', html)
    # Collapse whitespace
    text = re.sub(r'\s+', ' ', text)
    return text.strip()


def _extract_comment_contexts(html: str, context_chars: int = 100) -> dict[str, str]:
    """Extract surrounding context for each inline comment marker.

    Args:
        html: Page body HTML in Confluence storage format
        context_chars: Number of characters of context on each side

    Returns:
        Dictionary mapping marker ref to context string
    """
    import re

    contexts: dict[str, str] = {}

    # Find all inline comment markers with their refs
    # Pattern: <ac:inline-comment-marker ac:ref="UUID">text</ac:inline-comment-marker>
    marker_pattern = re.compile(
        r'<ac:inline-comment-marker[^>]*ac:ref="([^"]+)"[^>]*>(.*?)</ac:inline-comment-marker>',
        re.DOTALL,
    )

    # Convert HTML to plain text while preserving marker positions
    # Use [[[ ]]] as placeholders since they won't be stripped by HTML tag removal
    placeholder_map: dict[str, str] = {}
    html_with_placeholders = html

    for match in marker_pattern.finditer(html):
        ref = match.group(1)
        marker_text = match.group(2)
        placeholder = f'[[[MARKER:{ref}:{marker_text}]]]'
        placeholder_map[ref] = marker_text
        html_with_placeholders = html_with_placeholders.replace(
            match.group(0), placeholder, 1
        )

    # Strip HTML tags
    plain_text = _strip_html_tags(html_with_placeholders)

    # Find each placeholder and extract context
    for ref, marker_text in placeholder_map.items():
        placeholder = f'[[[MARKER:{ref}:{marker_text}]]]'
        pos = plain_text.find(placeholder)
        if pos == -1:
            continue

        # Get context before and after
        start = max(0, pos - context_chars)
        end = min(len(plain_text), pos + len(placeholder) + context_chars)

        # Extract context and replace placeholder with marked text
        context = plain_text[start:end]
        context = context.replace(placeholder, f'>>>{marker_text}<<<')

        # Clean up the context
        context = context.strip()
        contexts[ref] = context

    return contexts


async def _comments(
    page_id: str,
    config_path: Path,
    key_file: Path,
    include_resolved: bool,
) -> None:
    """Fetch and display comments from a Confluence page.

    Args:
        page_id: Page ID to fetch comments from
        config_path: Path to config.toml
        key_file: Path to private key PEM file
        include_resolved: Whether to include resolved comments
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

            # Fetch page info with body to extract context
            page = await confluence.get_page(page_id, expand=['body.storage'])
            page_title = page['title']
            page_url = await confluence.get_page_url(page_id)
            page_body = page.get('body', {}).get('storage', {}).get('value', '')

            # Build context map for inline comments
            context_map = _extract_comment_contexts(page_body)

            # Fetch both inline and footer comments
            inline_comments = await confluence.get_inline_comments(page_id)
            footer_comments = await confluence.get_footer_comments(page_id)

            # Filter by resolution status if needed
            inline_results = inline_comments['results']
            footer_results = footer_comments['results']

            if not include_resolved:
                inline_results = [
                    c for c in inline_results
                    if c.get('extensions', {}).get('resolution', {}).get('status', 'open') == 'open'
                ]
                footer_results = [
                    c for c in footer_results
                    if c.get('extensions', {}).get('resolution', {}).get('status', 'open') == 'open'
                ]

            total_count = len(inline_results) + len(footer_results)

            if total_count == 0:
                click.echo('No comments found.')
                return

            # Output header
            click.echo(f'# Comments on "{page_title}"')
            click.echo(f'Page URL: {page_url}')
            click.echo()

            # Output inline comments
            if inline_results:
                click.echo(f'## Inline Comments ({len(inline_results)})')
                click.echo()
                for comment in inline_results:
                    extensions = comment.get('extensions', {})
                    inline_props = extensions.get('inlineProperties', {})
                    resolution = extensions.get('resolution', {})

                    marker_ref = inline_props.get('markerRef', '')
                    original_text = inline_props.get('originalSelection', 'N/A')
                    status = resolution.get('status', 'open')
                    body_html = comment.get('body', {}).get('storage', {}).get('value', '')
                    body_text = _strip_html_tags(body_html)

                    # Get context from page body
                    context = context_map.get(marker_ref)

                    click.echo(f'### On text: "{original_text}"')
                    if context:
                        click.echo(f'Context: ...{context}...')
                    if include_resolved:
                        click.echo(f'Status: {status}')
                    click.echo(f'Comment: {body_text}')
                    click.echo()

            # Output footer comments
            if footer_results:
                click.echo(f'## Page Comments ({len(footer_results)})')
                click.echo()
                for comment in footer_results:
                    resolution = comment.get('extensions', {}).get('resolution', {})
                    status = resolution.get('status', 'open')
                    body_html = comment.get('body', {}).get('storage', {}).get('value', '')
                    body_text = _strip_html_tags(body_html)

                    if include_resolved:
                        click.echo(f'Status: {status}')
                    click.echo(f'Comment: {body_text}')
                    click.echo()

    except Exception as e:
        click.echo(click.style(f'Error: {e}', fg='red'), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


if __name__ == '__main__':
    cli()
