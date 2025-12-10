"""Tests for navigation tree builder."""

from docstage.core.navigation import NavItem, build_navigation
from docstage.core.site import SiteBuilder


class TestBuildNavigation:
    """Tests for build_navigation function."""

    def test__empty_site__returns_empty_list(self) -> None:
        """Return empty list when site has no pages."""
        site = SiteBuilder().build()

        nav = build_navigation(site)

        assert nav == []

    def test__flat_site__builds_navigation(self) -> None:
        """Build navigation from flat site structure."""
        builder = SiteBuilder()
        builder.add_page("Guide", "/guide")
        builder.add_page("API", "/api")
        site = builder.build()

        nav = build_navigation(site)

        assert len(nav) == 2
        titles = [item.title for item in nav]
        assert "Guide" in titles
        assert "API" in titles

    def test__nested_site__builds_navigation_tree(self) -> None:
        """Build navigation tree from nested site structure."""
        builder = SiteBuilder()
        parent_idx = builder.add_page("Domain A", "/domain-a")
        builder.add_page("Setup Guide", "/domain-a/guide", parent_idx)
        site = builder.build()

        nav = build_navigation(site)

        assert len(nav) == 1
        domain = nav[0]
        assert domain.title == "Domain A"
        assert domain.path == "/domain-a"
        assert len(domain.children) == 1
        assert domain.children[0].title == "Setup Guide"
        assert domain.children[0].path == "/domain-a/guide"

    def test__with_root_path__returns_subtree(self) -> None:
        """Return subtree when root_path is specified."""
        builder = SiteBuilder()
        parent_idx = builder.add_page("Domain A", "/domain-a")
        builder.add_page("Guide", "/domain-a/guide", parent_idx)
        builder.add_page("Other", "/other")
        site = builder.build()

        nav = build_navigation(site, "/domain-a")

        assert len(nav) == 1
        assert nav[0].title == "Guide"

    def test__deeply_nested__builds_full_tree(self) -> None:
        """Build navigation for deeply nested structure."""
        builder = SiteBuilder()
        idx_a = builder.add_page("A", "/a")
        idx_b = builder.add_page("B", "/a/b", idx_a)
        builder.add_page("C", "/a/b/c", idx_b)
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
