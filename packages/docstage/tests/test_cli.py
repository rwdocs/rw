"""Tests for CLI commands."""

from pathlib import Path

from click.testing import CliRunner
from docstage.cli import cli


class TestGenerateTokensCommand:
    """Tests for the confluence generate-tokens command."""

    def test_fails_without_private_key(self, tmp_path: Path) -> None:
        """Fail when private key file doesn't exist."""
        runner = CliRunner()
        result = runner.invoke(
            cli,
            [
                "confluence",
                "generate-tokens",
                "--private-key",
                str(tmp_path / "nonexistent.pem"),
            ],
        )

        assert result.exit_code != 0
