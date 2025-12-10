"""Tests for navigation tree builder."""

from pathlib import Path

import pytest
from docstage.core.navigation import NavItem, build_navigation
from docstage.core.site import SiteBuilder


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

    def test__with_root_path__returns_subtree(self, source_dir: Path) -> None:
        """Return subtree when root_path is specified."""
        builder = SiteBuilder(source_dir)
        parent_idx = builder.add_page("Domain A", "/domain-a", "domain-a/index.md")
        builder.add_page("Guide", "/domain-a/guide", "domain-a/guide.md", parent_idx)
        builder.add_page("Other", "/other", "other.md")
        site = builder.build()

        nav = build_navigation(site, "/domain-a")

        assert len(nav) == 1
        assert nav[0].title == "Guide"

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
