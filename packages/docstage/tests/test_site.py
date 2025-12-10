"""Tests for Site class."""

from pathlib import Path

import pytest
from docstage.core.site import (
    BreadcrumbItem,
    Page,
    Site,
    SiteBuilder,
    SiteLoader,
)


class TestSite:
    """Tests for Site class."""

    def test__get_page__returns_page(self) -> None:
        """Get page by path."""
        builder = SiteBuilder()
        builder.add_page("Guide", "/guide")
        site = builder.build()

        page = site.get_page("/guide")

        assert page is not None
        assert page.title == "Guide"
        assert page.path == "/guide"

    def test__get_page__not_found__returns_none(self) -> None:
        """Return None when page not found."""
        site = SiteBuilder().build()

        page = site.get_page("/nonexistent")

        assert page is None

    def test__get_page__normalizes_path(self) -> None:
        """Normalize path without leading slash."""
        builder = SiteBuilder()
        builder.add_page("Guide", "/guide")
        site = builder.build()

        page = site.get_page("guide")

        assert page is not None
        assert page.title == "Guide"

    def test__get_children__returns_children(self) -> None:
        """Get children of a page."""
        builder = SiteBuilder()
        parent_idx = builder.add_page("Parent", "/parent")
        builder.add_page("Child", "/parent/child", parent_idx)
        site = builder.build()

        children = site.get_children("/parent")

        assert len(children) == 1
        assert children[0].title == "Child"

    def test__get_children__not_found__returns_empty(self) -> None:
        """Return empty list when page not found."""
        site = SiteBuilder().build()

        children = site.get_children("/nonexistent")

        assert children == []

    def test__get_children__no_children__returns_empty(self) -> None:
        """Return empty list when page has no children."""
        builder = SiteBuilder()
        builder.add_page("Guide", "/guide")
        site = builder.build()

        children = site.get_children("/guide")

        assert children == []

    def test__get_breadcrumbs__empty_path__returns_empty(self) -> None:
        """Return empty list for empty path."""
        site = SiteBuilder().build()

        breadcrumbs = site.get_breadcrumbs("")

        assert breadcrumbs == []

    def test__get_breadcrumbs__root_page__returns_home(self) -> None:
        """Return Home for root-level page."""
        builder = SiteBuilder()
        builder.add_page("Guide", "/guide")
        site = builder.build()

        breadcrumbs = site.get_breadcrumbs("/guide")

        assert len(breadcrumbs) == 1
        assert breadcrumbs[0].title == "Home"
        assert breadcrumbs[0].path == "/"

    def test__get_breadcrumbs__nested_page__returns_ancestors(self) -> None:
        """Return Home and ancestors for nested page."""
        builder = SiteBuilder()
        parent_idx = builder.add_page("Parent", "/parent")
        builder.add_page("Child", "/parent/child", parent_idx)
        site = builder.build()

        breadcrumbs = site.get_breadcrumbs("/parent/child")

        assert len(breadcrumbs) == 2
        assert breadcrumbs[0].title == "Home"
        assert breadcrumbs[1].title == "Parent"
        assert breadcrumbs[1].path == "/parent"

    def test__get_breadcrumbs__not_found__returns_home(self) -> None:
        """Return just Home when page not found."""
        site = SiteBuilder().build()

        breadcrumbs = site.get_breadcrumbs("/nonexistent")

        assert len(breadcrumbs) == 1
        assert breadcrumbs[0].title == "Home"

    def test__get_root_pages__returns_roots(self) -> None:
        """Get root-level pages."""
        builder = SiteBuilder()
        builder.add_page("A", "/a")
        builder.add_page("B", "/b")
        site = builder.build()

        roots = site.get_root_pages()

        assert len(roots) == 2
        assert roots[0].title == "A"
        assert roots[1].title == "B"


class TestSiteBuilder:
    """Tests for SiteBuilder class."""

    def test__add_page__returns_index(self) -> None:
        """Add page returns its index."""
        builder = SiteBuilder()

        idx = builder.add_page("Guide", "/guide")

        assert idx == 0

    def test__add_page__increments_index(self) -> None:
        """Each page gets a unique index."""
        builder = SiteBuilder()

        idx1 = builder.add_page("A", "/a")
        idx2 = builder.add_page("B", "/b")

        assert idx1 == 0
        assert idx2 == 1

    def test__add_page__with_parent__links_child(self) -> None:
        """Child page is linked to parent."""
        builder = SiteBuilder()
        parent_idx = builder.add_page("Parent", "/parent")
        builder.add_page("Child", "/parent/child", parent_idx)
        site = builder.build()

        children = site.get_children("/parent")

        assert len(children) == 1
        assert children[0].path == "/parent/child"

    def test__build__creates_site(self) -> None:
        """Build creates Site instance."""
        builder = SiteBuilder()
        builder.add_page("Guide", "/guide")

        site = builder.build()

        assert site.get_page("/guide") is not None


class TestPage:
    """Tests for Page dataclass."""

    def test__creation__stores_values(self) -> None:
        """Page stores title and path."""
        page = Page(title="Guide", path="/guide")

        assert page.title == "Guide"
        assert page.path == "/guide"

    def test__frozen__immutable(self) -> None:
        """Page is frozen/immutable."""
        page = Page(title="Guide", path="/guide")

        with pytest.raises(AttributeError):
            page.title = "New Title"  # type: ignore[misc]


class TestBreadcrumbItem:
    """Tests for BreadcrumbItem dataclass."""

    def test__creation__stores_values(self) -> None:
        """BreadcrumbItem stores title and path."""
        item = BreadcrumbItem(title="Home", path="/")

        assert item.title == "Home"
        assert item.path == "/"

    def test__to_dict__returns_dict(self) -> None:
        """Convert to dictionary."""
        item = BreadcrumbItem(title="Home", path="/")

        result = item.to_dict()

        assert result == {"title": "Home", "path": "/"}


class TestSiteLoader:
    """Tests for SiteLoader class."""

    def test__load__missing_dir__returns_empty_site(self, tmp_path: Path) -> None:
        """Return empty site when source directory doesn't exist."""
        loader = SiteLoader(tmp_path / "nonexistent")

        site = loader.load()

        assert site.get_root_pages() == []

    def test__load__empty_dir__returns_empty_site(self, tmp_path: Path) -> None:
        """Return empty site when source directory is empty."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()

        loader = SiteLoader(source_dir)

        site = loader.load()

        assert site.get_root_pages() == []

    def test__load__flat_structure__builds_site(self, tmp_path: Path) -> None:
        """Build site from flat directory with markdown files."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# User Guide\n\nContent.")
        (source_dir / "api.md").write_text("# API Reference\n\nDocs.")

        loader = SiteLoader(source_dir)

        site = loader.load()

        assert len(site.get_root_pages()) == 2
        assert site.get_page("/guide") is not None
        assert site.get_page("/api") is not None

    def test__load__nested_structure__builds_site(self, tmp_path: Path) -> None:
        """Build site from nested directory structure."""
        source_dir = tmp_path / "docs"
        domain_dir = source_dir / "domain-a"
        domain_dir.mkdir(parents=True)
        (domain_dir / "index.md").write_text("# Domain A\n\nOverview.")
        (domain_dir / "guide.md").write_text("# Setup Guide\n\nSteps.")

        loader = SiteLoader(source_dir)

        site = loader.load()

        domain = site.get_page("/domain-a")
        assert domain is not None
        assert domain.title == "Domain A"

        children = site.get_children("/domain-a")
        assert len(children) == 1
        assert children[0].title == "Setup Guide"

    def test__load__extracts_title_from_h1(self, tmp_path: Path) -> None:
        """Extract title from first H1 heading."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# My Custom Title\n\nContent.")

        loader = SiteLoader(source_dir)

        site = loader.load()

        page = site.get_page("/guide")
        assert page is not None
        assert page.title == "My Custom Title"

    def test__load__falls_back_to_filename(self, tmp_path: Path) -> None:
        """Fall back to filename when no H1 heading."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "setup-guide.md").write_text("Content without heading.")

        loader = SiteLoader(source_dir)

        site = loader.load()

        page = site.get_page("/setup-guide")
        assert page is not None
        assert page.title == "Setup Guide"

    def test__load__skips_hidden_files(self, tmp_path: Path) -> None:
        """Skip files starting with dot."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / ".hidden.md").write_text("# Hidden")
        (source_dir / "visible.md").write_text("# Visible")

        loader = SiteLoader(source_dir)

        site = loader.load()

        assert site.get_page("/.hidden") is None
        assert site.get_page("/visible") is not None

    def test__load__skips_underscore_files(self, tmp_path: Path) -> None:
        """Skip files starting with underscore."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "_partial.md").write_text("# Partial")
        (source_dir / "main.md").write_text("# Main")

        loader = SiteLoader(source_dir)

        site = loader.load()

        assert site.get_page("/_partial") is None
        assert site.get_page("/main") is not None

    def test__load__directory_without_index__promotes_children(
        self,
        tmp_path: Path,
    ) -> None:
        """Promote children to parent level when directory has no index.md."""
        source_dir = tmp_path / "docs"
        no_index_dir = source_dir / "no-index"
        no_index_dir.mkdir(parents=True)
        (no_index_dir / "child.md").write_text("# Child Page")

        loader = SiteLoader(source_dir)

        site = loader.load()

        # Child should be at root level (promoted)
        roots = site.get_root_pages()
        assert len(roots) == 1
        assert roots[0].path == "/no-index/child"

    def test__load__caches_site_instance(self, tmp_path: Path) -> None:
        """Site instance is cached and reused on subsequent calls."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide")

        loader = SiteLoader(source_dir)

        site1 = loader.load()
        site2 = loader.load()

        assert site1 is site2

    def test__invalidate__clears_cached_site(self, tmp_path: Path) -> None:
        """Invalidate clears cached Site instance."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide")

        loader = SiteLoader(source_dir)

        site1 = loader.load()
        loader.invalidate()
        (source_dir / "new.md").write_text("# New")
        site2 = loader.load()

        assert site1 is not site2
        assert site2.get_page("/new") is not None

    def test__source_dir__returns_path(self, tmp_path: Path) -> None:
        """Return source directory from property."""
        source_dir = tmp_path / "docs"
        loader = SiteLoader(source_dir)

        assert loader.source_dir == source_dir


class TestSiteLoaderWithCache:
    """Tests for SiteLoader with cache."""

    def test__load__uses_cached_site(self, tmp_path: Path) -> None:
        """Use cached site on subsequent loads."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide")

        cache = _MockSiteCache()
        loader = SiteLoader(source_dir, cache)

        loader.load()
        # Modify file after first load
        (source_dir / "new.md").write_text("# New")
        site = loader.load()

        # Should return cached version (no /new page)
        assert site.get_page("/new") is None

    def test__load__use_cache_false__bypasses_cache(self, tmp_path: Path) -> None:
        """Bypass cache when use_cache=False."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide")

        cache = _MockSiteCache()
        loader = SiteLoader(source_dir, cache)

        loader.load()
        (source_dir / "new.md").write_text("# New")
        site = loader.load(use_cache=False)

        assert site.get_page("/new") is not None

    def test__invalidate__invalidates_cache(self, tmp_path: Path) -> None:
        """Invalidate clears external cache."""
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "guide.md").write_text("# Guide")

        cache = _MockSiteCache()
        loader = SiteLoader(source_dir, cache)

        loader.load()
        (source_dir / "new.md").write_text("# New")
        loader.invalidate()
        site = loader.load()

        assert site.get_page("/new") is not None


class _MockSiteCache:
    """Mock implementation of SiteCache protocol."""

    def __init__(self) -> None:
        self._site: Site | None = None

    def get_site(self) -> Site | None:
        return self._site

    def set_site(self, site: Site) -> None:
        self._site = site

    def invalidate_site(self) -> None:
        self._site = None
