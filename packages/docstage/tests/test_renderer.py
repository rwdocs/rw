"""Tests for page renderer."""

from pathlib import Path

import pytest
from docstage.core.cache import FileCache
from docstage.core.renderer import PageRenderer


class TestPageRendererRender:
    """Tests for PageRenderer.render()."""

    def test__simple_markdown__renders_to_html(self, tmp_path: Path) -> None:
        """Render a simple markdown file to HTML."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nThis is a guide.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        result = renderer.render(source_path, "guide")

        # extract_title=True removes H1 from HTML and extracts it as title
        assert "This is a guide" in result.html
        assert result.title == "Guide"
        assert result.from_cache is False

    def test__missing_file__raises_file_not_found(self, tmp_path: Path) -> None:
        """Raise FileNotFoundError when source file doesn't exist."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        with pytest.raises(FileNotFoundError, match="Source file not found"):
            renderer.render(source_dir / "nonexistent.md", "nonexistent")

    def test__second_render__returns_cached_result(self, tmp_path: Path) -> None:
        """Return cached result on subsequent renders."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nOriginal content.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        result1 = renderer.render(source_path, "guide")
        assert result1.from_cache is False

        result2 = renderer.render(source_path, "guide")
        assert result2.from_cache is True
        assert result2.html == result1.html
        assert result2.title == result1.title

    def test__mtime_change__invalidates_cache(self, tmp_path: Path) -> None:
        """Re-render when source file mtime changes."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nOriginal content.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        result1 = renderer.render(source_path, "guide")
        assert "Original content" in result1.html

        # Modify file (changes mtime)
        import time

        time.sleep(0.01)  # Ensure mtime differs
        source_path.write_text("# Guide\n\nUpdated content.")

        result2 = renderer.render(source_path, "guide")
        assert result2.from_cache is False
        assert "Updated content" in result2.html

    def test__nested_path__renders_correctly(self, tmp_path: Path) -> None:
        """Render markdown file in nested directory."""
        source_dir = tmp_path / "docs"
        nested_dir = source_dir / "domain" / "subdomain"
        nested_dir.mkdir(parents=True)
        source_path = nested_dir / "guide.md"
        source_path.write_text("# Nested Guide\n\nDeep content.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        result = renderer.render(source_path, "domain/subdomain/guide")

        assert result.title == "Nested Guide"
        assert "Deep content" in result.html

    def test__headings__extracts_toc(self, tmp_path: Path) -> None:
        """Extract table of contents from markdown."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        # HTML renderer keeps original heading levels (H1 stays H1, H2 stays H2)
        # Title (first H1) is excluded from ToC
        source_path = source_dir / "guide.md"
        source_path.write_text("""# Guide

## Introduction

Content here.

## Getting Started

More content.

### Installation

Steps.
""")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        result = renderer.render(source_path, "guide")

        # HTML renderer preserves levels, title excluded from ToC
        assert len(result.toc) == 3
        assert result.toc[0].level == 2
        assert result.toc[0].title == "Introduction"
        assert result.toc[1].level == 2
        assert result.toc[1].title == "Getting Started"
        assert result.toc[2].level == 3
        assert result.toc[2].title == "Installation"

    def test__cached_result__preserves_toc(self, tmp_path: Path) -> None:
        """Preserve ToC structure when loaded from cache."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        # HTML renderer keeps original heading levels
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\n## Section\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        renderer.render(source_path, "guide")
        result = renderer.render(source_path, "guide")

        assert result.from_cache is True
        assert len(result.toc) == 1
        assert result.toc[0].level == 2  # H2 stays H2 with HTML renderer
        assert result.toc[0].title == "Section"


class TestPageRendererInvalidate:
    """Tests for PageRenderer.invalidate()."""

    def test__cached_entry__invalidates_on_call(self, tmp_path: Path) -> None:
        """Invalidate cache entry for path."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        renderer.render(source_path, "guide")
        renderer.invalidate("guide")
        result = renderer.render(source_path, "guide")

        assert result.from_cache is False


class TestPageRendererWithKroki:
    """Tests for PageRenderer with Kroki diagram rendering."""

    def test__kroki_set_no_diagrams__renders_content(self, tmp_path: Path) -> None:
        """Render markdown without diagrams even when kroki_url is set."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nNo diagrams here.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(
            cache,
            kroki_url="https://kroki.io",
        )

        result = renderer.render(source_path, "guide")

        assert result.title == "Guide"
        assert "No diagrams here" in result.html
        assert result.from_cache is False

    def test__kroki_set_with_diagram__extracts_diagram(self, tmp_path: Path) -> None:
        """Extract diagrams when kroki_url is set (Kroki call expected to fail)."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text(
            "# Guide\n\n```plantuml\n@startuml\nA -> B\n@enduml\n```\n\nText after."
        )

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(
            cache,
            kroki_url="https://kroki.io",
        )

        # Kroki request will fail - diagram appears as error in result
        result = renderer.render(source_path, "guide")

        assert result.title == "Guide"
        # Verify diagram extraction was attempted (error HTML contains diagram placeholder)
        assert "diagram" in result.html.lower()


class TestPageRendererOptions:
    """Tests for PageRenderer configuration options."""

    def test__extract_title_false__keeps_h1_in_output(self, tmp_path: Path) -> None:
        """Keep H1 in output when extract_title is False."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# My Title\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache, extract_title=False)

        result = renderer.render(source_path, "guide")

        # Title should still be extracted for metadata
        # but H1 should remain in HTML
        assert "<h1" in result.html
        assert "My Title" in result.html

    def test__custom_dpi__accepted(self, tmp_path: Path) -> None:
        """Accept custom DPI setting."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache, dpi=300)

        result = renderer.render(source_path, "guide")

        assert result.title == "Guide"

    def test__include_dirs__accepted(self, tmp_path: Path) -> None:
        """Accept custom include directories."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        include_dir = tmp_path / "includes"
        include_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(
            cache,
            include_dirs=[include_dir],
        )

        result = renderer.render(source_path, "guide")

        assert result.title == "Guide"

    def test__config_file__accepted(self, tmp_path: Path) -> None:
        """Accept config file option."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nContent.")
        (source_dir / "config.iuml").write_text("skinparam backgroundColor white")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(
            cache,
            config_file="config.iuml",
        )

        result = renderer.render(source_path, "guide")

        assert result.title == "Guide"


class TestRenderResult:
    """Tests for RenderResult dataclass."""

    def test__fresh_render__has_warnings_list(self, tmp_path: Path) -> None:
        """RenderResult includes warnings list."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        result = renderer.render(source_path, "guide")

        # Fresh render should have warnings (empty list)
        assert isinstance(result.warnings, list)

    def test__cached_result__has_empty_warnings(self, tmp_path: Path) -> None:
        """Cached results have empty warnings list."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        source_path = source_dir / "guide.md"
        source_path.write_text("# Guide\n\nContent.")

        cache = FileCache(tmp_path / ".cache")
        renderer = PageRenderer(cache)

        renderer.render(source_path, "guide")
        result = renderer.render(source_path, "guide")

        assert result.from_cache is True
        assert result.warnings == []
