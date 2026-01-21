"""CLI interface for Docstage.

Command-line tool for converting markdown to Confluence pages.
"""

import sys
import tempfile
from pathlib import Path
from typing import cast

import click
from docstage_core import ConfluenceClient, read_private_key
from docstage_core.config import CliSettings, Config, ConfluenceConfig


@click.group()
def cli() -> None:
    """Docstage - Where documentation takes the stage."""


@click.group()
def confluence() -> None:
    """Confluence publishing commands."""


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
@click.option(
    "--cache/--no-cache",
    default=None,
    help="Enable/disable caching (overrides config, default: enabled)",
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
    cache: bool | None,
) -> None:
    """Start the documentation server."""
    from docstage.server import run_server

    cli_settings = CliSettings(
        host=host,
        port=port,
        source_dir=source_dir,
        cache_dir=cache_dir,
        cache_enabled=cache,
        kroki_url=kroki_url,
        live_reload_enabled=live_reload,
    )
    config = Config.load(config_path, cli_settings)

    click.echo(f"Starting server on {config.server.host}:{config.server.port}")
    click.echo(f"Source directory: {config.docs.source_dir}")
    if config.docs.cache_enabled:
        click.echo(f"Cache directory: {config.docs.cache_dir}")
    else:
        click.echo("Cache: disabled")
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
    "--extract-title/--no-extract-title",
    default=True,
    help="Extract title from first H1 heading and update page title (default: enabled)",
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
def update(
    markdown_file: Path,
    page_id: str,
    message: str | None,
    kroki_url: str | None,
    extract_title: bool,
    dry_run: bool,
    config_path: Path | None,
    key_file: Path,
) -> None:
    """Update a Confluence page from a markdown file."""
    from docstage_core import MarkdownConverter, preserve_comments

    try:
        config = Config.load(config_path)
        conf_config = _require_confluence_config(config)
        confluence_client = _create_confluence_client(conf_config, key_file)
        effective_kroki_url = _require_kroki_url(kroki_url, config)

        diagrams_config = config.diagrams
        click.echo(f"Converting {markdown_file}...")
        converter = MarkdownConverter(
            prepend_toc=True,
            extract_title=extract_title,
            include_dirs=diagrams_config.include_dirs,
            config_file=diagrams_config.config_file,
            dpi=diagrams_config.dpi,
        )
        markdown_text = markdown_file.read_text(encoding="utf-8")

        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir_path = Path(tmpdir)
            result = converter.convert(markdown_text, effective_kroki_url, tmpdir_path)
            new_html = result.html
            attachments = _collect_diagram_attachments(tmpdir_path)

            if result.title:
                click.echo(f"Title: {result.title}")

            click.echo(f"Fetching current page {page_id}...")
            current_page = confluence_client.get_page(
                page_id,
                expand=["body.storage", "version"],
            )
            current_version = current_page.version
            old_html = current_page.body or ""

            title = result.title or current_page.title

            click.echo("Preserving comment markers...")
            preserve_result = preserve_comments(old_html, new_html)

            if dry_run:
                _print_dry_run_summary(preserve_result.unmatched_comments)
                return

            _upload_attachments(confluence_client, page_id, attachments)

            click.echo(
                f'Updating page "{title}" from version {current_version} to {current_version + 1}...',
            )
            updated_page = confluence_client.update_page(
                page_id,
                title,
                preserve_result.html,
                current_version,
                message,
            )

            click.echo(
                click.style("\nPage updated successfully!", fg="green", bold=True),
            )
            click.echo(f"ID: {updated_page.id}")
            click.echo(f"Title: {updated_page.title}")
            click.echo(f"Version: {updated_page.version}")

            url = confluence_client.get_page_url(page_id)
            click.echo(f"URL: {url}")

            comments_response = confluence_client.get_comments(page_id)
            click.echo(f"\nComments on page: {comments_response.size}")

            _print_unmatched_comments_warning(preserve_result.unmatched_comments)

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        import traceback

        traceback.print_exc()
        sys.exit(1)


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
    effective_base_url = cast(str, effective_base_url)  # narrowing after sys.exit

    import asyncio

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
    return cast(ConfluenceConfig, config.confluence)  # narrowing after sys.exit


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
    return cast(str, effective)  # narrowing after sys.exit


def _collect_diagram_attachments(
    output_dir: Path,
    *,
    verbose: bool = True,
) -> list[tuple[str, bytes]]:
    """Collect PNG diagram files from output directory.

    Args:
        output_dir: Directory containing rendered diagram files
        verbose: If True, print each filename

    Returns:
        List of (filename, bytes) tuples
    """
    attachments: list[tuple[str, bytes]] = []
    for filepath in sorted(output_dir.glob("*.png")):
        attachments.append((filepath.name, filepath.read_bytes()))
        if verbose:
            click.echo(f"  -> {filepath.name}")
    return attachments


def _upload_attachments(
    confluence: ConfluenceClient,
    page_id: str,
    attachments: list[tuple[str, bytes]],
) -> None:
    """Upload diagram attachments to a Confluence page.

    Args:
        confluence: Confluence API client
        page_id: Target page ID
        attachments: List of (filename, bytes) tuples to upload
    """
    if not attachments:
        return

    click.echo(f"Uploading {len(attachments)} attachments...")
    for filename, image_data in attachments:
        click.echo(f"  Uploading {filename}...")
        confluence.upload_attachment(
            page_id,
            filename,
            image_data,
            "image/png",
        )


def _print_dry_run_summary(unmatched_comments: list) -> None:
    """Print dry run summary showing comments that would be resolved.

    Args:
        unmatched_comments: List of UnmatchedComment objects
    """
    click.echo(
        click.style(
            "\n[DRY RUN] No changes made to Confluence.",
            fg="cyan",
            bold=True,
        ),
    )
    if unmatched_comments:
        click.echo(
            click.style(
                f"\nComments that would be resolved ({len(unmatched_comments)}):",
                fg="yellow",
                bold=True,
            ),
        )
        for comment in unmatched_comments:
            click.echo(f'  - [{comment.ref_id}] "{comment.text}"')
    else:
        click.echo(
            click.style("\nNo comments would be resolved.", fg="green"),
        )


def _print_unmatched_comments_warning(unmatched_comments: list) -> None:
    """Print warning about comments that could not be placed.

    Args:
        unmatched_comments: List of UnmatchedComment objects
    """
    if not unmatched_comments:
        return

    click.echo(
        click.style(
            f"\nWarning: {len(unmatched_comments)} comment(s) could not be placed:",
            fg="yellow",
        ),
    )
    for comment in unmatched_comments:
        click.echo(f'  - [{comment.ref_id}] "{comment.text}"')


def _create_confluence_client(
    conf_config: ConfluenceConfig,
    key_file: Path,
) -> ConfluenceClient:
    """Create a Confluence client from config and key file.

    Args:
        conf_config: Confluence configuration
        key_file: Path to private key PEM file

    Returns:
        Configured Confluence client
    """
    private_key = read_private_key(key_file)
    return ConfluenceClient(
        conf_config.base_url,
        conf_config.consumer_key,
        private_key,
        conf_config.access_token,
        conf_config.access_secret,
    )


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


if __name__ == "__main__":
    cli()
