"""Site structure for document hierarchy.

Represents the document site structure with efficient path lookups
and traversal operations. Separate from navigation which is built
from the site for UI presentation.
"""

from dataclasses import dataclass


@dataclass(frozen=True)
class Page:
    """Document page data."""

    title: str
    path: str


@dataclass(frozen=True)
class BreadcrumbItem:
    """Breadcrumb navigation item."""

    title: str
    path: str

    def to_dict(self) -> dict[str, str]:
        """Convert to dictionary for JSON serialization."""
        return {"title": self.title, "path": self.path}


class Site:
    """Document site structure with efficient path lookups.

    Stores pages in a flat list with parent/children relationships
    tracked by indices. Provides O(1) path lookups and O(d) breadcrumb
    building where d is the page depth.
    """

    __slots__ = ("_children", "_pages", "_parents", "_path_index", "_roots")

    def __init__(
        self,
        pages: list[Page],
        children: list[list[int]],
        parents: list[int | None],
        roots: list[int],
    ) -> None:
        """Initialize site structure.

        Args:
            pages: Flat list of all pages
            children: Children indices for each page
            parents: Parent index for each page (None for roots)
            roots: Indices of root pages
        """
        self._pages = pages
        self._children = children
        self._parents = parents
        self._roots = roots
        self._path_index = {page.path: i for i, page in enumerate(pages)}

    def get_page(self, path: str) -> Page | None:
        """Get page by path.

        Args:
            path: Page path (e.g., "domain/page" or "/domain/page")

        Returns:
            Page if found, None otherwise
        """
        normalized = self._normalize_path(path)
        idx = self._path_index.get(normalized)
        if idx is None:
            return None
        return self._pages[idx]

    def get_children(self, path: str) -> list[Page]:
        """Get children of a page.

        Args:
            path: Page path (e.g., "domain/page" or "/domain/page")

        Returns:
            List of child Pages, empty if not found or no children
        """
        normalized = self._normalize_path(path)
        idx = self._path_index.get(normalized)
        if idx is None:
            return []
        return [self._pages[i] for i in self._children[idx]]

    def get_breadcrumbs(self, path: str) -> list[BreadcrumbItem]:
        """Build breadcrumbs for a given path.

        Returns breadcrumbs starting with "Home" for non-root pages,
        followed by ancestor pages. The current page is not included.

        Note:
            For unknown paths, returns [Home] to provide minimal navigation
            in UI even when the page doesn't exist in the site structure.
            This differs from get_page() which returns None for unknown paths.

        Args:
            path: Page path (e.g., "domain/page" or "/domain/page")

        Returns:
            List of BreadcrumbItem for ancestor navigation
        """
        if not path:
            return []

        normalized = self._normalize_path(path)
        idx = self._path_index.get(normalized)
        if idx is None:
            return [BreadcrumbItem(title="Home", path="/")]

        # Walk up parent chain
        ancestors: list[Page] = []
        current: int | None = idx
        while current is not None:
            ancestors.append(self._pages[current])
            current = self._parents[current]

        # Reverse to root-first, exclude current page
        ancestors.reverse()
        breadcrumbs = [BreadcrumbItem(title="Home", path="/")]
        for page in ancestors[:-1]:
            breadcrumbs.append(BreadcrumbItem(title=page.title, path=page.path))

        return breadcrumbs

    def get_root_pages(self) -> list[Page]:
        """Get root-level pages."""
        return [self._pages[i] for i in self._roots]

    def _normalize_path(self, path: str) -> str:
        """Normalize path to have leading slash."""
        return path if path.startswith("/") else f"/{path}"


class SiteBuilder:
    """Builder for constructing Site instances."""

    def __init__(self) -> None:
        self._pages: list[Page] = []
        self._children: list[list[int]] = []
        self._parents: list[int | None] = []
        self._roots: list[int] = []

    def add_page(
        self,
        title: str,
        path: str,
        parent_idx: int | None = None,
    ) -> int:
        """Add a page to the site.

        Args:
            title: Page title
            path: Page path
            parent_idx: Index of parent page, None for root

        Returns:
            Index of the added page
        """
        idx = len(self._pages)
        self._pages.append(Page(title=title, path=path))
        self._children.append([])
        self._parents.append(parent_idx)

        if parent_idx is None:
            self._roots.append(idx)
        else:
            self._children[parent_idx].append(idx)

        return idx

    def build(self) -> Site:
        """Build the Site instance."""
        return Site(
            pages=self._pages,
            children=self._children,
            parents=self._parents,
            roots=self._roots,
        )
