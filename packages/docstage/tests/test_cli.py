"""Tests for CLI commands."""

from pathlib import Path

from click.testing import CliRunner
from docstage.cli import cli


class TestConvertCommand:
    """Tests for the convert command."""

    def test_converts_markdown_file(self, tmp_path: Path) -> None:
        """Convert a simple markdown file to Confluence format."""
        markdown_file = tmp_path / "test.md"
        markdown_file.write_text("# Hello World\n\nThis is a test.")

        config_file = tmp_path / "docstage.toml"
        config_file.write_text('[diagrams]\nkroki_url = "https://kroki.io"')

        runner = CliRunner()
        result = runner.invoke(
            cli, ["convert", str(markdown_file), "-c", str(config_file)]
        )

        assert result.exit_code == 0
        assert "Converted to Confluence storage format" in result.output
        assert "<p>This is a test.</p>" in result.output

    def test_converts_with_code_blocks(self, tmp_path: Path) -> None:
        """Convert markdown with code blocks."""
        markdown_file = tmp_path / "test.md"
        markdown_file.write_text("""# Code Example

```python
print("Hello")
```
""")

        config_file = tmp_path / "docstage.toml"
        config_file.write_text('[diagrams]\nkroki_url = "https://kroki.io"')

        runner = CliRunner()
        result = runner.invoke(
            cli, ["convert", str(markdown_file), "-c", str(config_file)]
        )

        assert result.exit_code == 0
        assert "ac:structured-macro" in result.output
        assert 'print("Hello")' in result.output

    def test_fails_on_missing_file(self, tmp_path: Path) -> None:
        """Fail gracefully when markdown file doesn't exist."""
        nonexistent = tmp_path / "nonexistent.md"

        runner = CliRunner()
        result = runner.invoke(cli, ["convert", str(nonexistent)])

        assert result.exit_code != 0

    def test_fails_without_kroki_url(self, tmp_path: Path) -> None:
        """Fail when kroki_url is not provided."""
        markdown_file = tmp_path / "test.md"
        markdown_file.write_text("# Hello World")

        config_file = tmp_path / "docstage.toml"
        config_file.write_text("")

        runner = CliRunner()
        result = runner.invoke(
            cli, ["convert", str(markdown_file), "-c", str(config_file)]
        )

        assert result.exit_code == 1
        assert "kroki_url required" in result.output


class TestGetPageCommand:
    """Tests for the get-page command."""

    def test_fails_without_config(self, tmp_path: Path) -> None:
        """Fail when config file doesn't exist."""
        runner = CliRunner()
        result = runner.invoke(
            cli,
            ["get-page", "12345", "--config", str(tmp_path / "nonexistent.toml")],
        )

        assert result.exit_code != 0


class TestTestAuthCommand:
    """Tests for the test-auth command."""

    def test_fails_without_config(self, tmp_path: Path) -> None:
        """Fail when config file doesn't exist."""
        runner = CliRunner()
        result = runner.invoke(
            cli,
            ["test-auth", "--config", str(tmp_path / "nonexistent.toml")],
        )

        assert result.exit_code != 0


class TestGenerateTokensCommand:
    """Tests for the generate-tokens command."""

    def test_fails_without_private_key(self, tmp_path: Path) -> None:
        """Fail when private key file doesn't exist."""
        runner = CliRunner()
        result = runner.invoke(
            cli,
            ["generate-tokens", "--private-key", str(tmp_path / "nonexistent.pem")],
        )

        assert result.exit_code != 0
