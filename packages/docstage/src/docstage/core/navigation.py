"""Navigation tree builder.

Builds navigation trees from Site structures for UI presentation.
Navigation is a view layer over the site document hierarchy.
"""

from dataclasses import dataclass, field
from pathlib import Path
from typing import TypedDict

from docstage.core.site import Page, Site, SiteCache, SiteLoader


class NavItemDict(TypedDict, total=False):
    """Dictionary representation of a navigation item."""

    title: str
    path: str
    children: list[NavItemDict]


@dataclass
class NavItem:
    """Navigation item with children for UI tree."""

    title: str
    path: str
    children: list[NavItem] = field(default_factory=list)

    def to_dict(self) -> NavItemDict:
        """Convert to dictionary for JSON serialization."""
        result: NavItemDict = {"title": self.title, "path": self.path}
        if self.children:
            result["children"] = [child.to_dict() for child in self.children]
        return result


def build_navigation(site: Site, root_path: str | None = None) -> list[NavItem]:
    """Build navigation tree from site structure.

    Args:
        site: Site structure to build navigation from
        root_path: Optional path to start from (for subtrees)

    Returns:
        List of NavItem trees for navigation UI
    """
    if root_path:
        pages = site.get_children(root_path)
    else:
        pages = site.get_root_pages()

    return [_build_nav_item(site, page) for page in pages]


def _build_nav_item(site: Site, page: Page) -> NavItem:
    """Recursively build NavItem from page."""
    children = site.get_children(page.path)
    return NavItem(
        title=page.title,
        path=page.path,
        children=[_build_nav_item(site, child) for child in children],
    )


class NavigationBuilder:
    """Builds navigation views from site structure.

    Wraps a SiteLoader to provide navigation-specific functionality.
    Navigation is a UI view layer over the underlying site hierarchy.
    """

    def __init__(
        self,
        source_dir: Path,
        cache: SiteCache | None = None,
    ) -> None:
        """Initialize builder.

        Args:
            source_dir: Root directory containing markdown sources
            cache: Optional cache implementing SiteCache protocol
        """
        self._site_loader = SiteLoader(source_dir, cache)

    @property
    def source_dir(self) -> Path:
        """Root directory containing markdown sources."""
        return self._site_loader.source_dir

    def build_site(self, *, use_cache: bool = True) -> Site:
        """Get site structure.

        Args:
            use_cache: Whether to use cached data if available

        Returns:
            Site with all discovered documents
        """
        return self._site_loader.load(use_cache=use_cache)

    def build(self, *, use_cache: bool = True) -> list[NavItem]:
        """Build navigation tree from site structure.

        Args:
            use_cache: Whether to use cached navigation if available

        Returns:
            List of NavItem for navigation UI
        """
        site = self._site_loader.load(use_cache=use_cache)
        return build_navigation(site)

    def invalidate(self) -> None:
        """Invalidate cached site and navigation."""
        self._site_loader.invalidate()

    def get_subtree(self, path: str) -> list[NavItem] | None:
        """Get navigation subtree for a specific section.

        Args:
            path: Section path (e.g., "domain-a/subdomain")

        Returns:
            List of NavItem for the section, or None if not found
        """
        site = self._site_loader.load()

        if not path:
            return build_navigation(site)

        normalized = path if path.startswith("/") else f"/{path}"
        page = site.get_page(normalized)
        if page is None:
            return None

        return build_navigation(site, normalized)
