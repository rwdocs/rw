"""Navigation tree builder.

Builds navigation trees from Site structures for UI presentation.
Navigation is a view layer over the site document hierarchy.
"""

from dataclasses import dataclass, field
from typing import TypedDict

from docstage.core.site import Page, Site


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
