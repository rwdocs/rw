"""Tests for file-based cache."""

import json
from pathlib import Path

from docstage.core.cache import FileCache, compute_diagram_hash


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


class TestFileCacheDiagram:
    """Tests for diagram cache methods."""

    def test_get_diagram_returns_none_when_missing(self, tmp_path: Path) -> None:
        """Return None when diagram doesn't exist in cache."""
        cache = FileCache(tmp_path / ".cache")

        result = cache.get_diagram("abc123", "svg")

        assert result is None

    def test_set_and_get_diagram_svg(self, tmp_path: Path) -> None:
        """Store and retrieve SVG diagram."""
        cache = FileCache(tmp_path / ".cache")
        svg_content = "<svg><circle r='10'/></svg>"

        cache.set_diagram("abc123", "svg", svg_content)
        result = cache.get_diagram("abc123", "svg")

        assert result == svg_content

    def test_set_and_get_diagram_png(self, tmp_path: Path) -> None:
        """Store and retrieve PNG diagram (as data URI)."""
        cache = FileCache(tmp_path / ".cache")
        png_data_uri = "data:image/png;base64,iVBORw0KGgo="

        cache.set_diagram("def456", "png", png_data_uri)
        result = cache.get_diagram("def456", "png")

        assert result == png_data_uri

    def test_diagrams_stored_in_diagrams_directory(self, tmp_path: Path) -> None:
        """Store diagrams in the diagrams subdirectory."""
        cache = FileCache(tmp_path / ".cache")

        cache.set_diagram("hash123", "svg", "<svg/>")

        diagram_path = tmp_path / ".cache" / "diagrams" / "hash123.svg"
        assert diagram_path.exists()
        assert diagram_path.read_text() == "<svg/>"


class TestComputeDiagramHash:
    """Tests for compute_diagram_hash function."""

    def test_returns_sha256_hash(self) -> None:
        """Return a SHA-256 hex digest."""
        result = compute_diagram_hash("@startuml\nA -> B\n@enduml", "plantuml", "svg")

        assert len(result) == 64  # SHA-256 produces 64 hex characters
        assert all(c in "0123456789abcdef" for c in result)

    def test_same_input_same_hash(self) -> None:
        """Return same hash for identical inputs."""
        hash1 = compute_diagram_hash("source", "plantuml", "svg")
        hash2 = compute_diagram_hash("source", "plantuml", "svg")

        assert hash1 == hash2

    def test_different_source_different_hash(self) -> None:
        """Return different hash for different source."""
        hash1 = compute_diagram_hash("source1", "plantuml", "svg")
        hash2 = compute_diagram_hash("source2", "plantuml", "svg")

        assert hash1 != hash2

    def test_different_endpoint_different_hash(self) -> None:
        """Return different hash for different endpoint."""
        hash1 = compute_diagram_hash("source", "plantuml", "svg")
        hash2 = compute_diagram_hash("source", "mermaid", "svg")

        assert hash1 != hash2

    def test_different_format_different_hash(self) -> None:
        """Return different hash for different format."""
        hash1 = compute_diagram_hash("source", "plantuml", "svg")
        hash2 = compute_diagram_hash("source", "plantuml", "png")

        assert hash1 != hash2

    def test_different_dpi_different_hash(self) -> None:
        """Return different hash for different DPI."""
        hash1 = compute_diagram_hash("source", "plantuml", "svg", dpi=96)
        hash2 = compute_diagram_hash("source", "plantuml", "svg", dpi=192)

        assert hash1 != hash2


class TestFileCacheProperty:
    """Tests for FileCache properties."""

    def test_cache_dir_property(self, tmp_path: Path) -> None:
        """Return the cache directory path."""
        cache_path = tmp_path / ".cache"
        cache = FileCache(cache_path)

        assert cache.cache_dir == cache_path


class TestFileCacheReadMetaEdgeCases:
    """Tests for _read_meta edge cases."""

    def test_returns_none_for_invalid_json(self, tmp_path: Path) -> None:
        """Return None when meta file contains invalid JSON."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        meta_file = meta_dir / "page.json"
        meta_file.write_text("not valid json", encoding="utf-8")

        # Also create HTML file so we reach the meta reading code
        pages_dir = tmp_path / ".cache" / "pages"
        pages_dir.mkdir(parents=True)
        html_file = pages_dir / "page.html"
        html_file.write_text("<p>Test</p>")

        result = cache.get("page", 1234567890.0)

        assert result is None

    def test_returns_none_for_non_dict_json(self, tmp_path: Path) -> None:
        """Return None when meta file contains non-dict JSON."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        meta_file = meta_dir / "page.json"
        meta_file.write_text('"just a string"', encoding="utf-8")

        pages_dir = tmp_path / ".cache" / "pages"
        pages_dir.mkdir(parents=True)
        html_file = pages_dir / "page.html"
        html_file.write_text("<p>Test</p>")

        result = cache.get("page", 1234567890.0)

        assert result is None

    def test_returns_none_when_source_mtime_missing(self, tmp_path: Path) -> None:
        """Return None when meta file is missing source_mtime."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        meta_file = meta_dir / "page.json"
        meta_file.write_text(json.dumps({"title": "Test", "toc": []}), encoding="utf-8")

        pages_dir = tmp_path / ".cache" / "pages"
        pages_dir.mkdir(parents=True)
        html_file = pages_dir / "page.html"
        html_file.write_text("<p>Test</p>")

        result = cache.get("page", 1234567890.0)

        assert result is None

    def test_returns_none_when_toc_missing(self, tmp_path: Path) -> None:
        """Return None when meta file is missing toc."""
        cache = FileCache(tmp_path / ".cache")
        meta_dir = tmp_path / ".cache" / "meta"
        meta_dir.mkdir(parents=True)
        meta_file = meta_dir / "page.json"
        meta_file.write_text(
            json.dumps({"title": "Test", "source_mtime": 1234567890.0}),
            encoding="utf-8",
        )

        pages_dir = tmp_path / ".cache" / "pages"
        pages_dir.mkdir(parents=True)
        html_file = pages_dir / "page.html"
        html_file.write_text("<p>Test</p>")

        result = cache.get("page", 1234567890.0)

        assert result is None
