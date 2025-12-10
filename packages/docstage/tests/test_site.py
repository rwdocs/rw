"""Tests for Site class."""

from docstage.core.site import BreadcrumbItem, Page, Site, SiteBuilder


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

        try:
            page.title = "New Title"  # type: ignore[misc]
            assert False, "Should raise FrozenInstanceError"
        except AttributeError:
            pass


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
