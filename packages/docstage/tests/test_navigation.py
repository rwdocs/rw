"""Tests for navigation tree builder."""

from pathlib import Path

import pytest
from docstage.core.navigation import NavItem, build_navigation
from docstage.core.site import SiteBuilder, SiteLoader


class TestBuildNavigation:
    """Tests for build_navigation function."""

    @pytest.fixture
    def source_dir(self, tmp_path: Path) -> Path:
        return tmp_path / "docs"

    def test__empty_site__returns_empty_list(self, source_dir: Path) -> None:
        """Return empty list when site has no pages."""
        site = SiteBuilder(source_dir).build()

        nav = build_navigation(site)

        assert nav == []

    def test__flat_site__builds_navigation(self, source_dir: Path) -> None:
        """Build navigation from flat site structure."""
        builder = SiteBuilder(source_dir)
        builder.add_page("Guide", "/guide", "guide.md")
        builder.add_page("API", "/api", "api.md")
        site = builder.build()

        nav = build_navigation(site)

        assert len(nav) == 2
        titles = [item.title for item in nav]
        assert "Guide" in titles
        assert "API" in titles

    def test__nested_site__builds_navigation_tree(self, source_dir: Path) -> None:
        """Build navigation tree from nested site structure."""
        builder = SiteBuilder(source_dir)
        parent_idx = builder.add_page("Domain A", "/domain-a", "domain-a/index.md")
        builder.add_page(
            "Setup Guide", "/domain-a/guide", "domain-a/guide.md", parent_idx
        )
        site = builder.build()

        nav = build_navigation(site)

        assert len(nav) == 1
        domain = nav[0]
        assert domain.title == "Domain A"
        assert domain.path == "/domain-a"
        assert len(domain.children) == 1
        assert domain.children[0].title == "Setup Guide"
        assert domain.children[0].path == "/domain-a/guide"

    def test__deeply_nested__builds_full_tree(self, source_dir: Path) -> None:
        """Build navigation for deeply nested structure."""
        builder = SiteBuilder(source_dir)
        idx_a = builder.add_page("A", "/a", "a/index.md")
        idx_b = builder.add_page("B", "/a/b", "a/b/index.md", idx_a)
        builder.add_page("C", "/a/b/c", "a/b/c/index.md", idx_b)
        site = builder.build()

        nav = build_navigation(site)

        assert nav[0].title == "A"
        assert nav[0].children[0].title == "B"
        assert nav[0].children[0].children[0].title == "C"

    def test__root_page__excluded_from_navigation(self, source_dir: Path) -> None:
        """Root page at '/' is excluded from navigation, showing its children."""
        builder = SiteBuilder(source_dir)
        root_idx = builder.add_page("Home", "/", "index.md")
        builder.add_page("Domains", "/domains", "domains/index.md", root_idx)
        builder.add_page("Usage", "/usage", "usage/index.md", root_idx)
        site = builder.build()

        nav = build_navigation(site)

        # Navigation should show children of root, not root itself
        assert len(nav) == 2
        titles = [item.title for item in nav]
        assert "Domains" in titles
        assert "Usage" in titles
        assert "Home" not in titles

    def test__root_page_with_file_siblings__shows_siblings(
        self, source_dir: Path
    ) -> None:
        """Siblings of root index.md are shown as navigation items.

        When root index.md exists, files at the same directory level become
        children of the root page in the Site model. Navigation shows these
        children, so filesystem siblings of index.md ARE included.
        """
        builder = SiteBuilder(source_dir)
        root_idx = builder.add_page("Home", "/", "index.md")
        builder.add_page("About", "/about", "about.md", root_idx)
        builder.add_page("Domains", "/domains", "domains/index.md", root_idx)
        site = builder.build()

        nav = build_navigation(site)

        assert len(nav) == 2
        titles = [item.title for item in nav]
        # Both the sibling file and the subdirectory should appear
        assert "About" in titles
        assert "Domains" in titles
        assert "Home" not in titles


class TestNavItem:
    """Tests for NavItem dataclass."""

    def test__creation__stores_title_and_path(self) -> None:
        """NavItem stores title and path."""
        item = NavItem(title="Guide", path="/guide")

        assert item.title == "Guide"
        assert item.path == "/guide"
        assert item.children == []

    def test__with_children__stores_children(self) -> None:
        """NavItem stores children."""
        child = NavItem(title="Child", path="/parent/child")
        item = NavItem(title="Parent", path="/parent", children=[child])

        assert len(item.children) == 1
        assert item.children[0].title == "Child"

    def test__to_dict__without_children(self) -> None:
        """Convert item without children to dict."""
        item = NavItem(title="Guide", path="/guide")

        result = item.to_dict()

        assert result == {"title": "Guide", "path": "/guide"}

    def test__to_dict__with_children(self) -> None:
        """Convert item with children to dict."""
        child = NavItem(title="Child", path="/parent/child")
        item = NavItem(title="Parent", path="/parent", children=[child])

        result = item.to_dict()

        assert result == {
            "title": "Parent",
            "path": "/parent",
            "children": [{"title": "Child", "path": "/parent/child"}],
        }


class TestBuildNavigationWithSiteLoader:
    """Integration tests for build_navigation with SiteLoader."""

    def test__root_index_with_file_siblings__shows_siblings(
        self, tmp_path: Path
    ) -> None:
        """Siblings of root index.md are shown via SiteLoader.

        Verifies full path from filesystem files to navigation includes
        files at the same level as root index.md.
        """
        source_dir = tmp_path / "docs"
        source_dir.mkdir()
        (source_dir / "index.md").write_text("# Home\n\nWelcome.")
        (source_dir / "about.md").write_text("# About\n\nAbout us.")
        domain_dir = source_dir / "domains"
        domain_dir.mkdir()
        (domain_dir / "index.md").write_text("# Domains\n\nDomain list.")

        loader = SiteLoader(source_dir)
        site = loader.load()
        nav = build_navigation(site)

        assert len(nav) == 2
        titles = [item.title for item in nav]
        assert "About" in titles
        assert "Domains" in titles
        assert "Home" not in titles
