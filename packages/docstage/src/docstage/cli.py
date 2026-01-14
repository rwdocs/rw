"""CLI interface for Docstage.

Command-line tool for converting markdown to Confluence pages.
"""

import asyncio
import sys
from pathlib import Path

import click

from docstage.config import Config, ConfluenceConfig


@click.group()
def cli() -> None:
    """Docstage - Where documentation takes the stage."""
    pass


@click.group()
def confluence() -> None:
    """Confluence publishing commands."""
    pass


cli.add_command(confluence)


@cli.command()
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
@click.option(
    "--source-dir",
    "-s",
    type=click.Path(exists=True, path_type=Path, file_okay=False),
    default=None,
    help="Documentation source directory (overrides config)",
)
@click.option(
    "--cache-dir",
    type=click.Path(path_type=Path, file_okay=False),
    default=None,
    help="Cache directory (overrides config)",
)
@click.option(
    "--host",
    default=None,
    help="Host to bind to (overrides config)",
)
@click.option(
    "--port",
    "-p",
    type=int,
    default=None,
    help="Port to bind to (overrides config)",
)
@click.option(
    "--kroki-url",
    default=None,
    help="Kroki server URL for diagram rendering (overrides config)",
)
@click.option(
    "--verbose",
    "-v",
    is_flag=True,
    help="Enable verbose output (show diagram warnings)",
)
@click.option(
    "--live-reload/--no-live-reload",
    default=None,
    help="Enable/disable live reload (overrides config, default: enabled)",
)
def serve(
    config_path: Path | None,
    source_dir: Path | None,
    cache_dir: Path | None,
    host: str | None,
    port: int | None,
    kroki_url: str | None,
    verbose: bool,
    live_reload: bool | None,
) -> None:
    """Start the documentation server."""
    from docstage.server import run_server

    config = Config.load(config_path).with_overrides(
        host=host,
        port=port,
        source_dir=source_dir,
        cache_dir=cache_dir,
        kroki_url=kroki_url,
        live_reload_enabled=live_reload,
    )

    click.echo(f"Starting server on {config.server.host}:{config.server.port}")
    click.echo(f"Source directory: {config.docs.source_dir}")
    click.echo(f"Cache directory: {config.docs.cache_dir}")
    if config.diagrams.kroki_url:
        click.echo(f"Kroki URL: {config.diagrams.kroki_url}")
    else:
        click.echo("Diagram rendering: disabled (no kroki_url in config)")
    if config.live_reload.enabled:
        click.echo("Live reload: enabled")
    else:
        click.echo("Live reload: disabled")

    run_server(config, verbose=verbose)


@confluence.command()
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
@click.option(
    "--key-file",
    "-k",
    type=click.Path(exists=True, path_type=Path),
    default="private_key.pem",
    help="Path to OAuth private key file",
)
def test_auth(config_path: Path | None, key_file: Path) -> None:
    """Test Confluence authentication."""
    asyncio.run(_test_auth(config_path, key_file))


@confluence.command()
@click.argument("page_id")
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
@click.option(
    "--key-file",
    "-k",
    type=click.Path(exists=True, path_type=Path),
    default="private_key.pem",
    help="Path to OAuth private key file",
)
def get_page(page_id: str, config_path: Path | None, key_file: Path) -> None:
    """Get page information by ID."""
    asyncio.run(_get_page(page_id, config_path, key_file))


@confluence.command()
@click.argument("title")
@click.option(
    "--space",
    "-s",
    help="Space key (default: from config confluence.test.space_key)",
)
@click.option(
    "--body",
    "-b",
    default="<p>Test page created by docstage</p>",
    help="Page body HTML",
)
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
@click.option(
    "--key-file",
    "-k",
    type=click.Path(exists=True, path_type=Path),
    default="private_key.pem",
    help="Path to OAuth private key file",
)
def test_create(
    title: str,
    space: str | None,
    body: str,
    config_path: Path | None,
    key_file: Path,
) -> None:
    """Test creating a page."""
    asyncio.run(_test_create(title, space, body, config_path, key_file))


@confluence.command()
@click.argument("markdown_file", type=click.Path(exists=True, path_type=Path))
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
@click.option(
    "--kroki-url",
    default=None,
    help="Kroki server URL for diagram rendering (overrides config)",
)
def convert(
    markdown_file: Path,
    config_path: Path | None,
    kroki_url: str | None,
) -> None:
    """Convert a markdown file to Confluence storage format and display it."""
    import tempfile

    from docstage.confluence import MarkdownConverter

    config = Config.load(config_path)
    effective_kroki_url = _require_kroki_url(kroki_url, config)

    converter = MarkdownConverter()
    markdown_text = markdown_file.read_text(encoding="utf-8")

    with tempfile.TemporaryDirectory() as tmpdir:
        result = converter.convert(markdown_text, effective_kroki_url, Path(tmpdir))

    click.echo(click.style("\nMarkdown file:", fg="cyan", bold=True))
    click.echo(f"{markdown_file}\n")
    click.echo(
        click.style("Converted to Confluence storage format:", fg="green", bold=True),
    )
    click.echo(result.html)


@confluence.command()
@click.argument("markdown_file", type=click.Path(exists=True, path_type=Path))
@click.argument("title")
@click.option(
    "--space",
    "-s",
    help="Space key (default: from config confluence.test.space_key)",
)
@click.option(
    "--kroki-url",
    default=None,
    help="Kroki server URL for diagram rendering (overrides config)",
)
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
@click.option(
    "--key-file",
    "-k",
    type=click.Path(exists=True, path_type=Path),
    default="private_key.pem",
    help="Path to OAuth private key file",
)
def create(
    markdown_file: Path,
    title: str,
    space: str | None,
    kroki_url: str | None,
    config_path: Path | None,
    key_file: Path,
) -> None:
    """Create a Confluence page from a markdown file."""
    asyncio.run(_create(markdown_file, title, space, kroki_url, config_path, key_file))


@confluence.command()
@click.argument("markdown_file", type=click.Path(exists=True, path_type=Path))
@click.argument("page_id")
@click.option(
    "--message",
    "-m",
    help="Version message for the update",
)
@click.option(
    "--kroki-url",
    default=None,
    help="Kroki server URL for diagram rendering (overrides config)",
)
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
@click.option(
    "--key-file",
    "-k",
    type=click.Path(exists=True, path_type=Path),
    default="private_key.pem",
    help="Path to OAuth private key file",
)
def update(
    markdown_file: Path,
    page_id: str,
    message: str | None,
    kroki_url: str | None,
    config_path: Path | None,
    key_file: Path,
) -> None:
    """Update a Confluence page from a markdown file."""
    asyncio.run(
        _update(markdown_file, page_id, message, kroki_url, config_path, key_file),
    )


@confluence.command()
@click.argument("markdown_file", type=click.Path(exists=True, path_type=Path))
@click.argument("page_id")
@click.option(
    "--mkdocs-root",
    "-r",
    type=click.Path(exists=True, path_type=Path),
    required=True,
    help="Root directory of the MkDocs site",
)
@click.option(
    "--kroki-url",
    default=None,
    help="Kroki server URL for diagram rendering (overrides config)",
)
@click.option(
    "--message",
    "-m",
    help="Version message for the update",
)
@click.option(
    "--dry-run",
    is_flag=True,
    help="Preview changes without updating Confluence. Shows comments that would be lost.",
)
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
@click.option(
    "--key-file",
    "-k",
    type=click.Path(exists=True, path_type=Path),
    default="private_key.pem",
    help="Path to OAuth private key file",
)
def upload_mkdocs(
    markdown_file: Path,
    page_id: str,
    mkdocs_root: Path,
    kroki_url: str | None,
    message: str | None,
    dry_run: bool,
    config_path: Path | None,
    key_file: Path,
) -> None:
    """Upload an MkDocs document with diagrams to Confluence.

    Renders PlantUML diagrams to PNG images and uploads them as attachments.
    """
    asyncio.run(
        _upload_mkdocs(
            markdown_file,
            page_id,
            mkdocs_root,
            kroki_url,
            message,
            dry_run,
            config_path,
            key_file,
        ),
    )


@confluence.command()
@click.argument("page_id")
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
@click.option(
    "--key-file",
    "-k",
    type=click.Path(exists=True, path_type=Path),
    default="private_key.pem",
    help="Path to OAuth private key file",
)
@click.option(
    "--include-resolved",
    is_flag=True,
    help="Include resolved comments in output",
)
def comments(
    page_id: str,
    config_path: Path | None,
    key_file: Path,
    include_resolved: bool,
) -> None:
    """Fetch and display comments from a Confluence page.

    Outputs comments in a format suitable for fixing issues in source markdown.
    """
    asyncio.run(_comments(page_id, config_path, key_file, include_resolved))


@confluence.command()
@click.option(
    "--private-key",
    "-k",
    type=click.Path(exists=True, path_type=Path),
    default="private_key.pem",
    help="Path to RSA private key file",
)
@click.option(
    "--consumer-key",
    default=None,
    help='OAuth consumer key (default: from config or "docstage")',
)
@click.option(
    "--base-url",
    "-u",
    default=None,
    help="Confluence base URL (default: from config)",
)
@click.option(
    "--port",
    "-p",
    default=8080,
    type=int,
    help="Local callback server port (default: 8080)",
)
@click.option(
    "--config",
    "-c",
    "config_path",
    type=click.Path(exists=True, path_type=Path),
    default=None,
    help="Path to configuration file (default: auto-discover docstage.toml)",
)
def generate_tokens(
    private_key: Path,
    consumer_key: str | None,
    base_url: str | None,
    port: int,
    config_path: Path | None,
) -> None:
    """Generate OAuth access tokens for Confluence.

    This starts an interactive OAuth 1.0 flow to generate access tokens.
    You will need to authorize the application in your browser.
    """
    config = Config.load(config_path)

    effective_consumer_key = (
        consumer_key
        or (config.confluence.consumer_key if config.confluence else None)
        or "docstage"
    )

    effective_base_url = base_url or (
        config.confluence.base_url if config.confluence else None
    )
    if effective_base_url is None:
        click.echo(
            click.style(
                "Error: base_url required (via --base-url or config)",
                fg="red",
            ),
            err=True,
        )
        sys.exit(1)

    asyncio.run(
        _generate_tokens(private_key, effective_consumer_key, effective_base_url, port),
    )


def _require_confluence_config(config: Config) -> ConfluenceConfig:
    """Check that confluence configuration is present and return it.

    Args:
        config: Application config

    Returns:
        Confluence configuration

    Raises:
        SystemExit: If confluence config is missing
    """
    if config.confluence is None:
        click.echo(
            click.style(
                "Error: confluence configuration required in docstage.toml",
                fg="red",
            ),
            err=True,
        )
        click.echo("\nAdd the following to your docstage.toml:")
        click.echo("\n[confluence]")
        click.echo('base_url = "https://confluence.example.com"')
        click.echo('access_token = "your-token"')
        click.echo('access_secret = "your-secret"')
        sys.exit(1)
    return config.confluence


def _require_kroki_url(kroki_url: str | None, config: Config) -> str:
    """Get effective kroki_url or exit with error.

    Args:
        kroki_url: CLI-provided kroki_url (overrides config if set)
        config: Application config

    Returns:
        Effective kroki_url

    Raises:
        SystemExit: If kroki_url is not provided
    """
    effective = kroki_url if kroki_url is not None else config.diagrams.kroki_url
    if not effective:
        click.echo(
            click.style(
                "Error: kroki_url required (via --kroki-url or config)",
                fg="red",
            ),
            err=True,
        )
        sys.exit(1)
    return effective


def _require_space_key(space: str | None, config: Config) -> str:
    """Get effective space key or exit with error.

    Args:
        space: CLI-provided space key (overrides config if set)
        config: Application config

    Returns:
        Effective space key

    Raises:
        SystemExit: If space key is not provided
    """
    if space:
        return space
    if config.confluence_test and config.confluence_test.space_key:
        return config.confluence_test.space_key
    click.echo(
        click.style(
            "Error: No space key provided and confluence.test.space_key not in config",
            fg="red",
        ),
        err=True,
    )
    sys.exit(1)


async def _test_auth(config_path: Path | None, key_file: Path) -> None:
    """Test authentication with Confluence API.

    Args:
        config_path: Path to config file
        key_file: Path to private key PEM file
    """
    try:
        from docstage.oauth import create_confluence_client, read_private_key

        config = Config.load(config_path)
        conf_config = _require_confluence_config(config)

        click.echo(f"Reading private key from {key_file}...")
        private_key = read_private_key(key_file)

        click.echo("Creating authenticated client...")
        async with create_confluence_client(
            conf_config.access_token,
            conf_config.access_secret,
            private_key,
            conf_config.consumer_key,
        ) as client:
            base_url = conf_config.base_url
            click.echo(f"Testing connection to {base_url}...")

            response = await client.get(f"{base_url}/rest/api/user/current")
            response.raise_for_status()

            user_data = response.json()
            username = user_data.get(
                "username",
                user_data.get("displayName", "Unknown"),
            )

            click.echo(click.style("Authentication successful!", fg="green"))
            click.echo(f"Authenticated as: {username}")

    except FileNotFoundError as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        click.echo("\nMake sure you have:")
        click.echo("1. Created docstage.toml with your OAuth credentials")
        click.echo("2. Placed your private_key.pem file in the project root")
        sys.exit(1)
    except Exception as e:
        click.echo(click.style(f"Authentication failed: {e}", fg="red"), err=True)
        sys.exit(1)


async def _get_page(page_id: str, config_path: Path | None, key_file: Path) -> None:
    """Get page information.

    Args:
        page_id: Page ID to fetch
        config_path: Path to config file
        key_file: Path to private key PEM file
    """
    try:
        from docstage.confluence import ConfluenceClient
        from docstage.oauth import create_confluence_client, read_private_key

        config = Config.load(config_path)
        conf_config = _require_confluence_config(config)

        private_key = read_private_key(key_file)

        async with create_confluence_client(
            conf_config.access_token,
            conf_config.access_secret,
            private_key,
            conf_config.consumer_key,
        ) as http_client:
            confluence = ConfluenceClient(http_client, conf_config.base_url)

            click.echo(f"Fetching page {page_id}...")
            page = await confluence.get_page(
                page_id,
                expand=["body.storage", "version"],
            )

            click.echo(click.style("\nPage Information:", fg="green", bold=True))
            click.echo(f"ID: {page['id']}")
            click.echo(f"Title: {page['title']}")
            click.echo(f"Version: {page['version']['number']}")

            url = await confluence.get_page_url(page_id)
            click.echo(f"URL: {url}")

            comments = await confluence.get_comments(page_id)
            click.echo(f"\nComments: {comments['size']}")

            if "body" in page and "storage" in page["body"]:
                content = page["body"]["storage"].get("value", "")
                click.echo(
                    click.style(
                        "\nPage Content (Confluence Storage Format):",
                        fg="cyan",
                        bold=True,
                    ),
                )
                click.echo(content)

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        sys.exit(1)


async def _test_create(
    title: str,
    space: str | None,
    body: str,
    config_path: Path | None,
    key_file: Path,
) -> None:
    """Test creating a page.

    Args:
        title: Page title
        space: Space key (or None to use config)
        body: Page body HTML
        config_path: Path to config file
        key_file: Path to private key PEM file
    """
    try:
        from docstage.confluence import ConfluenceClient
        from docstage.oauth import create_confluence_client, read_private_key

        config = Config.load(config_path)
        conf_config = _require_confluence_config(config)

        private_key = read_private_key(key_file)
        space = _require_space_key(space, config)

        async with create_confluence_client(
            conf_config.access_token,
            conf_config.access_secret,
            private_key,
            conf_config.consumer_key,
        ) as http_client:
            confluence = ConfluenceClient(http_client, conf_config.base_url)

            click.echo(f'Creating page "{title}" in space {space}...')
            page = await confluence.create_page(space, title, body)

            click.echo(
                click.style("\nPage created successfully!", fg="green", bold=True),
            )
            click.echo(f"ID: {page['id']}")
            click.echo(f"Title: {page['title']}")
            click.echo(f"Version: {page['version']['number']}")

            url = await confluence.get_page_url(page["id"])
            click.echo(f"URL: {url}")

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


async def _create(
    markdown_file: Path,
    title: str,
    space: str | None,
    kroki_url: str | None,
    config_path: Path | None,
    key_file: Path,
) -> None:
    """Create a page from markdown file.

    Args:
        markdown_file: Path to markdown file
        title: Page title
        space: Space key (or None to use config)
        kroki_url: Kroki server URL (or None to use config)
        config_path: Path to config file
        key_file: Path to private key PEM file
    """
    import tempfile

    try:
        from docstage.confluence import ConfluenceClient, MarkdownConverter
        from docstage.oauth import create_confluence_client, read_private_key

        config = Config.load(config_path)
        conf_config = _require_confluence_config(config)

        private_key = read_private_key(key_file)
        effective_kroki_url = _require_kroki_url(kroki_url, config)
        space = _require_space_key(space, config)

        click.echo(f"Converting {markdown_file}...")
        converter = MarkdownConverter()
        markdown_text = markdown_file.read_text(encoding="utf-8")

        with tempfile.TemporaryDirectory() as tmpdir:
            result = converter.convert(markdown_text, effective_kroki_url, Path(tmpdir))
        confluence_body = result.html

        async with create_confluence_client(
            conf_config.access_token,
            conf_config.access_secret,
            private_key,
            conf_config.consumer_key,
        ) as http_client:
            confluence = ConfluenceClient(http_client, conf_config.base_url)

            click.echo(f'Creating page "{title}" in space {space}...')
            page = await confluence.create_page(space, title, confluence_body)

            click.echo(
                click.style("\nPage created successfully!", fg="green", bold=True),
            )
            click.echo(f"ID: {page['id']}")
            click.echo(f"Title: {page['title']}")
            click.echo(f"Version: {page['version']['number']}")

            url = await confluence.get_page_url(page["id"])
            click.echo(f"URL: {url}")

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


async def _update(
    markdown_file: Path,
    page_id: str,
    message: str | None,
    kroki_url: str | None,
    config_path: Path | None,
    key_file: Path,
) -> None:
    """Update a page from markdown file.

    Args:
        markdown_file: Path to markdown file
        page_id: Page ID to update
        message: Optional version message
        kroki_url: Kroki server URL (or None to use config)
        config_path: Path to config file
        key_file: Path to private key PEM file
    """
    import tempfile

    try:
        from docstage.confluence import ConfluenceClient, MarkdownConverter
        from docstage.confluence.comment_preservation import CommentPreserver
        from docstage.oauth import create_confluence_client, read_private_key

        config = Config.load(config_path)
        conf_config = _require_confluence_config(config)

        private_key = read_private_key(key_file)
        effective_kroki_url = _require_kroki_url(kroki_url, config)

        click.echo(f"Converting {markdown_file}...")
        converter = MarkdownConverter()
        markdown_text = markdown_file.read_text(encoding="utf-8")

        with tempfile.TemporaryDirectory() as tmpdir:
            result = converter.convert(markdown_text, effective_kroki_url, Path(tmpdir))
        new_html = result.html

        async with create_confluence_client(
            conf_config.access_token,
            conf_config.access_secret,
            private_key,
            conf_config.consumer_key,
        ) as http_client:
            confluence = ConfluenceClient(http_client, conf_config.base_url)

            click.echo(f"Fetching current page {page_id}...")
            current_page = await confluence.get_page(
                page_id,
                expand=["body.storage", "version"],
            )
            current_version = current_page["version"]["number"]
            title = current_page["title"]
            old_html = current_page["body"]["storage"]["value"]

            click.echo("Preserving comment markers...")
            preserver = CommentPreserver()
            preserve_result = preserver.preserve_comments(old_html, new_html)

            click.echo(
                f'Updating page "{title}" from version {current_version} to {current_version + 1}...',
            )
            updated_page = await confluence.update_page(
                page_id,
                title,
                preserve_result.html,
                current_version,
                message,
            )

            click.echo(
                click.style("\nPage updated successfully!", fg="green", bold=True),
            )
            click.echo(f"ID: {updated_page['id']}")
            click.echo(f"Title: {updated_page['title']}")
            click.echo(f"Version: {updated_page['version']['number']}")

            url = await confluence.get_page_url(page_id)
            click.echo(f"URL: {url}")

            comments = await confluence.get_comments(page_id)
            click.echo(f"\nComments on page: {comments['size']}")

            if preserve_result.unmatched_comments:
                click.echo(
                    click.style(
                        f"\nWarning: {len(preserve_result.unmatched_comments)} comment(s) could not be placed:",
                        fg="yellow",
                    ),
                )
                for comment in preserve_result.unmatched_comments:
                    click.echo(f'  - [{comment.ref}] "{comment.text}"')

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


async def _upload_mkdocs(
    markdown_file: Path,
    page_id: str,
    mkdocs_root: Path,
    kroki_url: str | None,
    message: str | None,
    dry_run: bool,
    config_path: Path | None,
    key_file: Path,
) -> None:
    """Upload an MkDocs document with diagrams to Confluence.

    Args:
        markdown_file: Path to markdown file
        page_id: Page ID to update
        mkdocs_root: Root directory of the MkDocs site
        kroki_url: Kroki server URL (or None to use config)
        message: Optional version message
        dry_run: If True, only show what would happen without updating
        config_path: Path to config file
        key_file: Path to private key PEM file
    """
    import tempfile

    try:
        from docstage_core import MarkdownConverter

        from docstage.confluence import ConfluenceClient
        from docstage.confluence.comment_preservation import CommentPreserver
        from docstage.oauth import create_confluence_client, read_private_key

        config = Config.load(config_path)
        conf_config = _require_confluence_config(config)

        private_key = read_private_key(key_file)
        effective_kroki_url = _require_kroki_url(kroki_url, config)

        include_dirs = [
            mkdocs_root / "includes",
            mkdocs_root / "gen" / "includes",
            mkdocs_root,
        ]
        include_dirs = [d for d in include_dirs if d.exists()]

        click.echo(f"Include directories: {[str(d) for d in include_dirs]}")

        click.echo(f"Converting {markdown_file}...")
        markdown_text = markdown_file.read_text(encoding="utf-8")
        converter = MarkdownConverter(
            prepend_toc=True,
            extract_title=True,
            include_dirs=include_dirs,
            config_file="config.iuml",
        )

        with tempfile.TemporaryDirectory() as tmpdir:
            click.echo(f"Rendering diagrams via Kroki ({effective_kroki_url})...")
            result = converter.convert(markdown_text, effective_kroki_url, Path(tmpdir))
            new_html = result.html

            click.echo(f"Rendered {len(result.diagrams)} diagrams")

            if result.title:
                click.echo(f"Title: {result.title}")

            attachment_data: list[tuple[str, bytes]] = []
            for diagram in result.diagrams:
                display_width = diagram.width // 2
                filepath = Path(tmpdir) / diagram.filename
                attachment_data.append((diagram.filename, filepath.read_bytes()))
                click.echo(
                    f"  -> {diagram.filename} ({diagram.width}x{diagram.height} -> {display_width}px)",
                )

            async with create_confluence_client(
                conf_config.access_token,
                conf_config.access_secret,
                private_key,
                conf_config.consumer_key,
            ) as http_client:
                confluence = ConfluenceClient(http_client, conf_config.base_url)

                click.echo(f"Fetching current page {page_id}...")
                current_page = await confluence.get_page(
                    page_id,
                    expand=["body.storage", "version"],
                )
                current_version = current_page["version"]["number"]
                old_html = current_page["body"]["storage"]["value"]

                title = result.title or current_page["title"]

                click.echo("Preserving comment markers...")
                preserver = CommentPreserver()
                preserve_result = preserver.preserve_comments(old_html, new_html)

                if dry_run:
                    click.echo(
                        click.style(
                            "\n[DRY RUN] No changes made to Confluence.",
                            fg="cyan",
                            bold=True,
                        ),
                    )
                    if preserve_result.unmatched_comments:
                        click.echo(
                            click.style(
                                f"\nComments that would be resolved ({len(preserve_result.unmatched_comments)}):",
                                fg="yellow",
                                bold=True,
                            ),
                        )
                        for comment in preserve_result.unmatched_comments:
                            click.echo(f'  - [{comment.ref}] "{comment.text}"')
                    else:
                        click.echo(
                            click.style("\nNo comments would be resolved.", fg="green"),
                        )
                    return

                if attachment_data:
                    click.echo(f"Uploading {len(attachment_data)} attachments...")
                    for filename, image_data in attachment_data:
                        click.echo(f"  Uploading {filename}...")
                        await confluence.upload_attachment(
                            page_id,
                            filename,
                            image_data,
                            "image/png",
                        )

                click.echo(
                    f'Updating page "{title}" from version {current_version} to {current_version + 1}...',
                )
                updated_page = await confluence.update_page(
                    page_id,
                    title,
                    preserve_result.html,
                    current_version,
                    message,
                )

                click.echo(
                    click.style("\nPage updated successfully!", fg="green", bold=True),
                )
                click.echo(f"ID: {updated_page['id']}")
                click.echo(f"Title: {updated_page['title']}")
                click.echo(f"Version: {updated_page['version']['number']}")

                url = await confluence.get_page_url(page_id)
                click.echo(f"URL: {url}")

                if preserve_result.unmatched_comments:
                    click.echo(
                        click.style(
                            f"\nWarning: {len(preserve_result.unmatched_comments)} comment(s) could not be placed:",
                            fg="yellow",
                        ),
                    )
                    for comment in preserve_result.unmatched_comments:
                        click.echo(f'  - [{comment.ref}] "{comment.text}"')

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


async def _generate_tokens(
    private_key_path: Path,
    consumer_key: str,
    base_url: str,
    port: int = 8080,
) -> None:
    """Generate OAuth access tokens through interactive flow.

    Args:
        private_key_path: Path to RSA private key
        consumer_key: OAuth consumer key
        base_url: Confluence base URL
        port: Local callback server port
    """
    try:
        from authlib.integrations.httpx_client import AsyncOAuth1Client

        from docstage.oauth import read_private_key

        click.echo(f"Reading private key from {private_key_path}...")
        private_key = read_private_key(private_key_path)

        base_url = base_url.rstrip("/")
        endpoints = {
            "request_token_url": f"{base_url}/plugins/servlet/oauth/request-token",
            "authorize_url": f"{base_url}/plugins/servlet/oauth/authorize",
            "access_token_url": f"{base_url}/plugins/servlet/oauth/access-token",
        }

        click.echo(f"Creating OAuth client for consumer key: {consumer_key}")
        client = AsyncOAuth1Client(
            client_id=consumer_key,
            rsa_key=private_key.decode("utf-8"),
            signature_method="RSA-SHA1",
        )

        try:
            click.echo("\nStep 1: Requesting temporary credentials...")
            client.redirect_uri = "oob"
            response = await client.fetch_request_token(
                endpoints["request_token_url"],
            )
            oauth_token = response.get("oauth_token")
            oauth_token_secret = response.get("oauth_token_secret")

            if not oauth_token or not oauth_token_secret:
                click.echo(
                    click.style("Failed to get request token", fg="red"),
                    err=True,
                )
                sys.exit(1)

            click.echo(click.style("✓ Temporary token received", fg="green"))

            auth_url = f"{endpoints['authorize_url']}?oauth_token={oauth_token}"

            click.echo("\n" + "=" * 70)
            click.echo(
                click.style("Step 2: Authorization Required", fg="cyan", bold=True),
            )
            click.echo("=" * 70)
            click.echo(
                "\nPlease open this URL in your browser to authorize the application:",
            )
            click.echo(click.style(f"\n{auth_url}\n", fg="cyan", bold=True))
            click.echo('\nAfter clicking "Allow" in Confluence:')
            click.echo("  - Confluence will display a VERIFICATION CODE on the page")
            click.echo("  - Copy that code and paste it below")
            click.echo("=" * 70)

            click.echo("\nStep 3: Enter the verification code...")
            click.echo(
                "After authorizing, Confluence should display a verification code.",
            )
            oauth_verifier = click.prompt(
                "Enter the verification code",
                type=str,
            ).strip()

            if not oauth_verifier:
                click.echo(
                    click.style("Error: Verification code is required", fg="red"),
                    err=True,
                )
                sys.exit(1)

            click.echo(click.style("✓ Verification code received", fg="green"))

            click.echo("\nStep 4: Exchanging for access token...")
            client.token = {
                "oauth_token": oauth_token,
                "oauth_token_secret": oauth_token_secret,
            }
            response = await client.fetch_access_token(
                endpoints["access_token_url"],
                verifier=oauth_verifier,
            )

            access_token = response.get("oauth_token")
            access_secret = response.get("oauth_token_secret")

            if not access_token or not access_secret:
                click.echo(
                    click.style("Failed to get access token", fg="red"),
                    err=True,
                )
                sys.exit(1)

            click.echo("\n" + "=" * 70)
            click.echo(
                click.style("✓ OAuth Authorization Successful!", fg="green", bold=True),
            )
            click.echo("=" * 70)
            click.echo("\nAdd these credentials to your docstage.toml:")
            click.echo("\n[confluence]")
            click.echo(f'base_url = "{base_url}"')
            click.echo(f'access_token = "{access_token}"')
            click.echo(f'access_secret = "{access_secret}"')
            click.echo(f'consumer_key = "{consumer_key}"')
            click.echo("\n" + "=" * 70)
            click.echo(
                click.style(
                    "\nNote: These tokens inherit YOUR permissions in Confluence.",
                    fg="yellow",
                ),
            )
            click.echo(
                "If you can create/edit pages, these tokens will have write access.",
            )
            click.echo("=" * 70 + "\n")

        except Exception as e:
            click.echo(click.style(f"OAuth flow failed: {e}", fg="red"), err=True)
            import traceback

            traceback.print_exc()
            sys.exit(1)
        finally:
            await client.aclose()

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        sys.exit(1)


def _strip_html_tags(html: str) -> str:
    """Strip HTML tags and convert to plain text.

    Args:
        html: HTML string to convert

    Returns:
        Plain text with HTML tags removed
    """
    import re

    text = re.sub(r"<[^>]+>", " ", html)
    text = re.sub(r"\s+", " ", text)
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

    marker_pattern = re.compile(
        r'<ac:inline-comment-marker[^>]*ac:ref="([^"]+)"[^>]*>(.*?)</ac:inline-comment-marker>',
        re.DOTALL,
    )

    # Collect markers and create placeholders in single pass
    markers: list[tuple[str, str]] = []

    def replace_with_placeholder(match: re.Match[str]) -> str:
        ref, text = match.group(1), match.group(2)
        markers.append((ref, text))
        return f"[[[MARKER:{len(markers) - 1}]]]"

    html_with_placeholders = marker_pattern.sub(replace_with_placeholder, html)
    plain_text = _strip_html_tags(html_with_placeholders)

    # Build context map
    contexts: dict[str, str] = {}
    for idx, (ref, marker_text) in enumerate(markers):
        placeholder = f"[[[MARKER:{idx}]]]"
        pos = plain_text.find(placeholder)
        if pos == -1:
            continue

        start = max(0, pos - context_chars)
        end = min(len(plain_text), pos + len(placeholder) + context_chars)
        context = (
            plain_text[start:end].replace(placeholder, f">>>{marker_text}<<<").strip()
        )
        contexts[ref] = context

    return contexts


async def _comments(
    page_id: str,
    config_path: Path | None,
    key_file: Path,
    include_resolved: bool,
) -> None:
    """Fetch and display comments from a Confluence page.

    Args:
        page_id: Page ID to fetch comments from
        config_path: Path to config file
        key_file: Path to private key PEM file
        include_resolved: Whether to include resolved comments
    """
    try:
        from docstage.confluence import ConfluenceClient
        from docstage.oauth import create_confluence_client, read_private_key

        config = Config.load(config_path)
        conf_config = _require_confluence_config(config)

        private_key = read_private_key(key_file)

        async with create_confluence_client(
            conf_config.access_token,
            conf_config.access_secret,
            private_key,
            conf_config.consumer_key,
        ) as http_client:
            confluence = ConfluenceClient(http_client, conf_config.base_url)

            page = await confluence.get_page(page_id, expand=["body.storage"])
            page_title = page["title"]
            page_url = await confluence.get_page_url(page_id)
            page_body = page.get("body", {}).get("storage", {}).get("value", "")

            context_map = _extract_comment_contexts(page_body)

            inline_comments = await confluence.get_inline_comments(page_id)
            footer_comments = await confluence.get_footer_comments(page_id)

            inline_results = inline_comments["results"]
            footer_results = footer_comments["results"]

            if not include_resolved:
                inline_results = [
                    c
                    for c in inline_results
                    if c.get("extensions", {})
                    .get("resolution", {})
                    .get("status", "open")
                    == "open"
                ]
                footer_results = [
                    c
                    for c in footer_results
                    if c.get("extensions", {})
                    .get("resolution", {})
                    .get("status", "open")
                    == "open"
                ]

            total_count = len(inline_results) + len(footer_results)

            if total_count == 0:
                click.echo("No comments found.")
                return

            click.echo(f'# Comments on "{page_title}"')
            click.echo(f"Page URL: {page_url}")
            click.echo()

            if inline_results:
                click.echo(f"## Inline Comments ({len(inline_results)})")
                click.echo()
                for comment in inline_results:
                    extensions = comment.get("extensions", {})
                    inline_props = extensions.get("inlineProperties", {})
                    resolution = extensions.get("resolution", {})

                    marker_ref = inline_props.get("markerRef", "")
                    original_text = inline_props.get("originalSelection", "N/A")
                    status = resolution.get("status", "open")
                    body_html = (
                        comment.get("body", {}).get("storage", {}).get("value", "")
                    )
                    body_text = _strip_html_tags(body_html)

                    context = context_map.get(marker_ref)

                    click.echo(f'### On text: "{original_text}"')
                    if context:
                        click.echo(f"Context: ...{context}...")
                    if include_resolved:
                        click.echo(f"Status: {status}")
                    click.echo(f"Comment: {body_text}")
                    click.echo()

            if footer_results:
                click.echo(f"## Page Comments ({len(footer_results)})")
                click.echo()
                for comment in footer_results:
                    resolution = comment.get("extensions", {}).get("resolution", {})
                    status = resolution.get("status", "open")
                    body_html = (
                        comment.get("body", {}).get("storage", {}).get("value", "")
                    )
                    body_text = _strip_html_tags(body_html)

                    if include_resolved:
                        click.echo(f"Status: {status}")
                    click.echo(f"Comment: {body_text}")
                    click.echo()

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    cli()
