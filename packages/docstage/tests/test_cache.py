"""Tests for file-based cache."""

import json
from pathlib import Path

from docstage.core.cache import FileCache, NullCache


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
                },
            ),
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

    def test_creates_gitignore(self, tmp_path: Path) -> None:
        """Create .gitignore in cache directory."""
        cache = FileCache(tmp_path / ".cache")

        cache.set("page", "<p>Test</p>", "Title", 1234567890.0, [])

        gitignore_path = tmp_path / ".cache" / ".gitignore"
        assert gitignore_path.exists()
        assert (
            gitignore_path.read_text() == "# Ignore everything in this directory\n*\n"
        )

    def test_writes_html_content(self, tmp_path: Path) -> None:
        """Write HTML content to pages directory."""
        cache = FileCache(tmp_path / ".cache")

        cache.set(
            "page",
            "<article><h1>Title</h1></article>",
            "Title",
            1234567890.0,
            [],
        )

        html_path = tmp_path / ".cache" / "pages" / "page.html"
        assert html_path.read_text() == "<article><h1>Title</h1></article>"

    def test_writes_metadata_json(self, tmp_path: Path) -> None:
        """Write metadata as JSON to meta directory."""
        from docstage import __version__

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
            "build_version": __version__,
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


class TestFileCacheSite:
    """Tests for site cache methods."""

    def test_get_site_returns_none_when_missing(self, tmp_path: Path) -> None:
        """Return None when site cache doesn't exist."""
        cache = FileCache(tmp_path / ".cache")

        result = cache.get_site()

        assert result is None

    def test_set_and_get_site(self, tmp_path: Path) -> None:
        """Store and retrieve site structure."""
        from docstage.core.site import SiteBuilder

        source_dir = tmp_path / "docs"
        cache = FileCache(tmp_path / ".cache")
        builder = SiteBuilder(source_dir)
        parent_idx = builder.add_page("Domain A", "/domain-a", "domain-a/index.md")
        builder.add_page("Guide", "/domain-a/guide", "domain-a/guide.md", parent_idx)
        site = builder.build()

        cache.set_site(site)
        result = cache.get_site()

        assert result is not None
        assert result.source_dir == source_dir
        assert result.get_page("/domain-a") is not None
        assert result.get_page("/domain-a").title == "Domain A"
        assert result.get_page("/domain-a").source_path == Path("domain-a/index.md")
        children = result.get_children("/domain-a")
        assert len(children) == 1
        assert children[0].title == "Guide"
        assert children[0].source_path == Path("domain-a/guide.md")

    def test_invalidate_site(self, tmp_path: Path) -> None:
        """Remove cached site structure."""
        from docstage.core.site import SiteBuilder

        source_dir = tmp_path / "docs"
        cache = FileCache(tmp_path / ".cache")
        site = SiteBuilder(source_dir).build()
        cache.set_site(site)

        cache.invalidate_site()

        assert cache.get_site() is None

    def test_invalidate_site_when_missing(self, tmp_path: Path) -> None:
        """Do nothing when site cache doesn't exist."""
        cache = FileCache(tmp_path / ".cache")

        cache.invalidate_site()  # Should not raise

    def test_get_site_returns_none_on_invalid_json(self, tmp_path: Path) -> None:
        """Return None when site file contains invalid JSON."""
        cache = FileCache(tmp_path / ".cache")
        cache_dir = tmp_path / ".cache"
        cache_dir.mkdir(parents=True)
        site_path = cache_dir / "site.json"
        site_path.write_text("not valid json {", encoding="utf-8")

        result = cache.get_site()

        assert result is None


class TestFileCacheProperty:
    """Tests for FileCache properties."""

    def test_cache_dir_property(self, tmp_path: Path) -> None:
        """Return the cache directory path."""
        cache_path = tmp_path / ".cache"
        cache = FileCache(cache_path)

        assert cache.cache_dir == cache_path

    def test_diagrams_dir_property(self, tmp_path: Path) -> None:
        """Return the diagrams cache directory path."""
        cache_path = tmp_path / ".cache"
        cache = FileCache(cache_path)

        assert cache.diagrams_dir == cache_path / "diagrams"


class TestFileCacheReadMetaEdgeCases:
    """Tests for _read_meta edge cases."""

    def _setup_html_file(self, tmp_path: Path) -> None:
        """Helper to create HTML file so meta reading is reached."""
        pages_dir = tmp_path / ".cache" / "pages"
        pages_dir.mkdir(parents=True)
        (pages_dir / "page.html").write_text("<p>Test</p>")

    def test_returns_none_for_invalid_json(self, tmp_path: Path) -> None:
        """Return None when meta file contains invalid JSON."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        (meta_dir / "page.json").write_text("not valid json", encoding="utf-8")
        self._setup_html_file(tmp_path)

        result = cache.get("page", 1234567890.0)

        assert result is None

    def test_returns_none_for_non_dict_json(self, tmp_path: Path) -> None:
        """Return None when meta file contains non-dict JSON."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        (meta_dir / "page.json").write_text('"just a string"', encoding="utf-8")
        self._setup_html_file(tmp_path)

        result = cache.get("page", 1234567890.0)

        assert result is None

    def test_returns_none_when_source_mtime_missing(self, tmp_path: Path) -> None:
        """Return None when meta file is missing source_mtime."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        (meta_dir / "page.json").write_text(
            json.dumps({"title": "Test", "toc": []}), encoding="utf-8"
        )
        self._setup_html_file(tmp_path)

        result = cache.get("page", 1234567890.0)

        assert result is None

    def test_returns_none_when_toc_missing(self, tmp_path: Path) -> None:
        """Return None when meta file is missing toc."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        (meta_dir / "page.json").write_text(
            json.dumps({"title": "Test", "source_mtime": 1234567890.0}),
            encoding="utf-8",
        )
        self._setup_html_file(tmp_path)

        result = cache.get("page", 1234567890.0)

        assert result is None


class TestFileCacheVersionValidation:
    """Tests for build version validation in cache."""

    def test_returns_none_when_build_version_missing(self, tmp_path: Path) -> None:
        """Return None when cached meta is missing build_version (old cache)."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        meta_file = meta_dir / "page.json"
        # Old cache format without build_version
        meta_file.write_text(
            json.dumps(
                {
                    "title": "Test",
                    "source_mtime": 1234567890.0,
                    "toc": [],
                }
            ),
            encoding="utf-8",
        )

        pages_dir = tmp_path / ".cache" / "pages"
        pages_dir.mkdir(parents=True)
        html_file = pages_dir / "page.html"
        html_file.write_text("<p>Test</p>")

        result = cache.get("page", 1234567890.0)

        assert result is None

    def test_returns_none_when_build_version_mismatches(self, tmp_path: Path) -> None:
        """Return None when cached build_version doesn't match current version."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        meta_file = meta_dir / "page.json"
        meta_file.write_text(
            json.dumps(
                {
                    "title": "Test",
                    "source_mtime": 1234567890.0,
                    "toc": [],
                    "build_version": "0.0.1.dev1+gdeadbeef",  # Different version
                }
            ),
            encoding="utf-8",
        )

        pages_dir = tmp_path / ".cache" / "pages"
        pages_dir.mkdir(parents=True)
        html_file = pages_dir / "page.html"
        html_file.write_text("<p>Test</p>")

        result = cache.get("page", 1234567890.0)

        assert result is None

    def test_returns_entry_when_build_version_matches(self, tmp_path: Path) -> None:
        """Return CacheEntry when build_version matches current version."""
        from docstage import __version__

        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        meta_file = meta_dir / "page.json"
        meta_file.write_text(
            json.dumps(
                {
                    "title": "Test",
                    "source_mtime": 1234567890.0,
                    "toc": [],
                    "build_version": __version__,
                }
            ),
            encoding="utf-8",
        )

        pages_dir = tmp_path / ".cache" / "pages"
        pages_dir.mkdir(parents=True)
        html_file = pages_dir / "page.html"
        html_file.write_text("<p>Test</p>")

        result = cache.get("page", 1234567890.0)

        assert result is not None
        assert result.html == "<p>Test</p>"
        assert result.meta["build_version"] == __version__

    def test_set_includes_build_version(self, tmp_path: Path) -> None:
        """Verify set() stores build_version in metadata."""
        from docstage import __version__

        cache = FileCache(tmp_path / ".cache")

        cache.set("page", "<p>Test</p>", "Title", 1234567890.0, [])

        meta_path = tmp_path / ".cache" / "meta" / "page.json"
        meta = json.loads(meta_path.read_text())
        assert meta["build_version"] == __version__


class TestNullCache:
    """Tests for NullCache (no-op cache for disabled caching)."""

    def test_get_always_returns_none(self) -> None:
        """NullCache.get() always returns None (cache miss)."""
        cache = NullCache()

        result = cache.get("domain/page", 1234567890.0)

        assert result is None

    def test_set_is_noop(self) -> None:
        """NullCache.set() does nothing but doesn't raise."""
        cache = NullCache()

        # Should not raise
        cache.set(
            "domain/page",
            "<p>Test</p>",
            "Test Title",
            1234567890.0,
            [{"level": 2, "title": "Heading", "id": "heading"}],
        )

        # Still returns None after set
        result = cache.get("domain/page", 1234567890.0)
        assert result is None

    def test_invalidate_is_noop(self) -> None:
        """NullCache.invalidate() does nothing but doesn't raise."""
        cache = NullCache()

        # Should not raise
        cache.invalidate("domain/page")

    def test_clear_is_noop(self) -> None:
        """NullCache.clear() does nothing but doesn't raise."""
        cache = NullCache()

        # Should not raise
        cache.clear()

    def test_get_site_always_returns_none(self) -> None:
        """NullCache.get_site() always returns None."""
        cache = NullCache()

        result = cache.get_site()

        assert result is None

    def test_set_site_is_noop(self, tmp_path: Path) -> None:
        """NullCache.set_site() does nothing but doesn't raise."""
        from docstage.core.site import SiteBuilder

        cache = NullCache()
        source_dir = tmp_path / "docs"
        site = SiteBuilder(source_dir).build()

        # Should not raise
        cache.set_site(site)

        # Still returns None after set
        result = cache.get_site()
        assert result is None

    def test_invalidate_site_is_noop(self) -> None:
        """NullCache.invalidate_site() does nothing but doesn't raise."""
        cache = NullCache()

        # Should not raise
        cache.invalidate_site()

    def test_diagrams_dir_returns_none(self) -> None:
        """NullCache.diagrams_dir returns None (caching disabled)."""
        cache = NullCache()

        assert cache.diagrams_dir is None
