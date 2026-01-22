"""CLI interface for Docstage.

Command-line tool for converting markdown to Confluence pages.
"""

import sys
from pathlib import Path
from typing import cast

import click
from docstage_core import ConfluenceClient, DryRunResult, UpdateResult, read_private_key
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
    try:
        cli_settings = CliSettings(kroki_url=kroki_url)
        config = Config.load(config_path, cli_settings)
        conf_config = _require_confluence_config(config)
        confluence_client = _create_confluence_client(conf_config, key_file)

        markdown_text = markdown_file.read_text(encoding="utf-8")
        click.echo(f"Converting {markdown_file}...")

        if dry_run:
            result = confluence_client.dry_run_update(
                page_id, markdown_text, config.diagrams, extract_title
            )
            _print_dry_run_result(result)
        else:
            result = confluence_client.update_page_from_markdown(
                page_id, markdown_text, config.diagrams, extract_title, message
            )
            _print_update_result(result)

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
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
    config_path: Path | None,
) -> None:
    """Generate OAuth access tokens for Confluence.

    This starts an interactive OAuth 1.0 flow to generate access tokens.
    You will need to authorize the application in your browser.
    """
    from docstage_core import OAuthTokenGenerator

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

    try:
        click.echo(f"Reading private key from {private_key}...")
        key_bytes = read_private_key(private_key)

        generator = OAuthTokenGenerator(
            effective_base_url, effective_consumer_key, key_bytes
        )

        # Step 1: Get request token
        click.echo("\nStep 1: Requesting temporary credentials...")
        request_token = generator.request_token()
        click.echo(click.style("Temporary token received", fg="green"))

        # Step 2: User authorization
        click.echo("\n" + "=" * 70)
        click.echo(click.style("Step 2: Authorization Required", fg="cyan", bold=True))
        click.echo("=" * 70)
        click.echo("\nPlease open this URL in your browser:")
        click.echo(
            click.style(f"\n{request_token.authorization_url}\n", fg="cyan", bold=True)
        )

        verifier = click.prompt("Enter the verification code", type=str).strip()

        # Step 3: Exchange for access token
        click.echo("\nStep 3: Exchanging for access token...")
        access_token = generator.exchange_verifier(
            request_token.oauth_token,
            request_token.oauth_token_secret,
            verifier,
        )

        # Output results
        click.echo("\n" + "=" * 70)
        click.echo(
            click.style("OAuth Authorization Successful!", fg="green", bold=True)
        )
        click.echo("=" * 70)
        click.echo("\nAdd these credentials to your docstage.toml:")
        click.echo("\n[confluence]")
        click.echo(f'base_url = "{effective_base_url}"')
        click.echo(f'access_token = "{access_token.oauth_token}"')
        click.echo(f'access_secret = "{access_token.oauth_token_secret}"')
        click.echo(f'consumer_key = "{effective_consumer_key}"')

    except Exception as e:
        click.echo(click.style(f"Error: {e}", fg="red"), err=True)
        sys.exit(1)


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


def _print_dry_run_result(result: DryRunResult) -> None:
    """Print dry run results."""
    click.echo(click.style("\n[DRY RUN] No changes made.", fg="cyan", bold=True))

    if result.title:
        click.echo(f"Title: {result.title}")
    click.echo(f'Current page: "{result.current_title}" (v{result.current_version})')

    if result.attachment_count > 0:
        click.echo(f"\nAttachments ({result.attachment_count}):")
        for name in result.attachment_names:
            click.echo(f"  -> {name}")

    if result.unmatched_comments:
        click.echo(
            click.style(
                f"\nComments that would be resolved ({len(result.unmatched_comments)}):",
                fg="yellow",
                bold=True,
            )
        )
        for comment in result.unmatched_comments:
            click.echo(f'  - [{comment.ref_id}] "{comment.text}"')
    else:
        click.echo(click.style("\nNo comments would be resolved.", fg="green"))


def _print_update_result(result: UpdateResult) -> None:
    """Print update results."""
    click.echo(click.style("\nPage updated successfully!", fg="green", bold=True))
    click.echo(f"ID: {result.page.id}")
    click.echo(f"Title: {result.page.title}")
    click.echo(f"Version: {result.page.version}")
    click.echo(f"URL: {result.url}")
    click.echo(f"\nComments on page: {result.comment_count}")

    if result.attachments_uploaded > 0:
        click.echo(f"Attachments uploaded: {result.attachments_uploaded}")

    if result.unmatched_comments:
        click.echo(
            click.style(
                f"\nWarning: {len(result.unmatched_comments)} comment(s) could not be placed:",
                fg="yellow",
            )
        )
        for comment in result.unmatched_comments:
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


if __name__ == "__main__":
    cli()
