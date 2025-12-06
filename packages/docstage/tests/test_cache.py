"""Tests for file-based cache."""

import json
from pathlib import Path

from docstage.core.cache import FileCache


class TestFileCacheGet:
    """Tests for FileCache.get()."""

    def test_returns_none_for_missing_entry(self, tmp_path: Path) -> None:
        """Return None when cache entry doesn't exist."""
        cache = FileCache(tmp_path / ".cache")

        result = cache.get("domain/page", 1234567890.0)

        assert result is None

    def test_returns_none_when_html_missing(self, tmp_path: Path) -> None:
        """Return None when HTML file is missing but meta exists."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta" / "domain"
        meta_dir.mkdir(parents=True)
        meta_file = meta_dir / "page.json"
        meta_file.write_text(
            json.dumps(
                {
                    "title": "Test Page",
                    "source_mtime": 1234567890.0,
                    "toc": [],
                }
            )
        )

        result = cache.get("domain/page", 1234567890.0)

        assert result is None

    def test_returns_none_when_meta_missing(self, tmp_path: Path) -> None:
        """Return None when meta file is missing but HTML exists."""
        cache = FileCache(tmp_path / ".cache")
        pages_dir = tmp_path / ".cache" / "pages" / "domain"
        pages_dir.mkdir(parents=True)
        html_file = pages_dir / "page.html"
        html_file.write_text("<p>Test</p>")

        result = cache.get("domain/page", 1234567890.0)

        assert result is None

    def test_returns_none_when_mtime_differs(self, tmp_path: Path) -> None:
        """Return None when source mtime doesn't match cached mtime."""
        cache = FileCache(tmp_path / ".cache")
        cache.set("domain/page", "<p>Test</p>", "Test Page", 1234567890.0, [])

        result = cache.get("domain/page", 9999999999.0)

        assert result is None

    def test_returns_entry_when_valid(self, tmp_path: Path) -> None:
        """Return CacheEntry when cache is valid."""
        cache = FileCache(tmp_path / ".cache")
        cache.set(
            "domain/page",
            "<p>Test</p>",
            "Test Page",
            1234567890.0,
            [
                {"level": 2, "title": "Section", "id": "section"},
            ],
        )

        result = cache.get("domain/page", 1234567890.0)

        assert result is not None
        assert result.html == "<p>Test</p>"
        assert result.meta["title"] == "Test Page"
        assert result.meta["source_mtime"] == 1234567890.0
        assert result.meta["toc"] == [{"level": 2, "title": "Section", "id": "section"}]

    def test_returns_entry_with_none_title(self, tmp_path: Path) -> None:
        """Return CacheEntry when title is None."""
        cache = FileCache(tmp_path / ".cache")
        cache.set("domain/page", "<p>Test</p>", None, 1234567890.0, [])

        result = cache.get("domain/page", 1234567890.0)

        assert result is not None
        assert result.meta["title"] is None


class TestFileCacheSet:
    """Tests for FileCache.set()."""

    def test_creates_directories(self, tmp_path: Path) -> None:
        """Create parent directories if they don't exist."""
        cache = FileCache(tmp_path / ".cache")

        cache.set("domain/subdomain/page", "<p>Test</p>", "Title", 1234567890.0, [])

        html_path = tmp_path / ".cache" / "pages" / "domain" / "subdomain" / "page.html"
        meta_path = tmp_path / ".cache" / "meta" / "domain" / "subdomain" / "page.json"
        assert html_path.exists()
        assert meta_path.exists()

    def test_writes_html_content(self, tmp_path: Path) -> None:
        """Write HTML content to pages directory."""
        cache = FileCache(tmp_path / ".cache")

        cache.set(
            "page", "<article><h1>Title</h1></article>", "Title", 1234567890.0, []
        )

        html_path = tmp_path / ".cache" / "pages" / "page.html"
        assert html_path.read_text() == "<article><h1>Title</h1></article>"

    def test_writes_metadata_json(self, tmp_path: Path) -> None:
        """Write metadata as JSON to meta directory."""
        cache = FileCache(tmp_path / ".cache")

        cache.set(
            "page",
            "<p>Test</p>",
            "Test Title",
            1234567890.123,
            [
                {"level": 2, "title": "Heading", "id": "heading"},
            ],
        )

        meta_path = tmp_path / ".cache" / "meta" / "page.json"
        meta = json.loads(meta_path.read_text())
        assert meta == {
            "title": "Test Title",
            "source_mtime": 1234567890.123,
            "toc": [{"level": 2, "title": "Heading", "id": "heading"}],
        }


class TestFileCacheInvalidate:
    """Tests for FileCache.invalidate()."""

    def test_removes_html_and_meta(self, tmp_path: Path) -> None:
        """Remove both HTML and meta files."""
        cache = FileCache(tmp_path / ".cache")
        cache.set("domain/page", "<p>Test</p>", "Title", 1234567890.0, [])

        cache.invalidate("domain/page")

        html_path = tmp_path / ".cache" / "pages" / "domain" / "page.html"
        meta_path = tmp_path / ".cache" / "meta" / "domain" / "page.json"
        assert not html_path.exists()
        assert not meta_path.exists()

    def test_handles_nonexistent_entry(self, tmp_path: Path) -> None:
        """Do nothing when entry doesn't exist."""
        cache = FileCache(tmp_path / ".cache")

        cache.invalidate("nonexistent/page")  # Should not raise


class TestFileCacheClear:
    """Tests for FileCache.clear()."""

    def test_removes_all_entries(self, tmp_path: Path) -> None:
        """Remove all cached pages and metadata."""
        cache = FileCache(tmp_path / ".cache")
        cache.set("domain-a/page1", "<p>1</p>", "Page 1", 1.0, [])
        cache.set("domain-b/page2", "<p>2</p>", "Page 2", 2.0, [])

        cache.clear()

        pages_dir = tmp_path / ".cache" / "pages"
        meta_dir = tmp_path / ".cache" / "meta"
        assert not pages_dir.exists()
        assert not meta_dir.exists()

    def test_handles_empty_cache(self, tmp_path: Path) -> None:
        """Do nothing when cache is already empty."""
        cache = FileCache(tmp_path / ".cache")

        cache.clear()  # Should not raise


class TestFileCacheNavigation:
    """Tests for navigation cache methods."""

    def test_get_navigation_returns_none_when_missing(self, tmp_path: Path) -> None:
        """Return None when navigation cache doesn't exist."""
        cache = FileCache(tmp_path / ".cache")

        result = cache.get_navigation()

        assert result is None

    def test_set_and_get_navigation(self, tmp_path: Path) -> None:
        """Store and retrieve navigation tree."""
        cache = FileCache(tmp_path / ".cache")
        nav = {
            "items": [
                {"title": "Domain A", "path": "/domain-a", "children": []},
            ]
        }

        cache.set_navigation(nav)
        result = cache.get_navigation()

        assert result == nav

    def test_invalidate_navigation(self, tmp_path: Path) -> None:
        """Remove cached navigation tree."""
        cache = FileCache(tmp_path / ".cache")
        cache.set_navigation({"items": []})

        cache.invalidate_navigation()

        assert cache.get_navigation() is None

    def test_invalidate_navigation_when_missing(self, tmp_path: Path) -> None:
        """Do nothing when navigation cache doesn't exist."""
        cache = FileCache(tmp_path / ".cache")

        cache.invalidate_navigation()  # Should not raise
