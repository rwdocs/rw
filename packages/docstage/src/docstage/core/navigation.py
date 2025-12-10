"""Navigation tree builder from directory structure.

Builds navigation trees by scanning the filesystem. Uses index.md files
as section landing pages and extracts titles from the first H1 heading
in each document.
"""

import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Protocol, TypedDict

from docstage.core.site import Page, Site, SiteBuilder


class NavItemDict(TypedDict, total=False):
    """Dictionary representation of a navigation item."""

    title: str
    path: str
    children: list[NavItemDict]


class NavigationTreeDict(TypedDict):
    """Dictionary representation of a navigation tree."""

    items: list[NavItemDict]


class NavigationCache(Protocol):
    """Protocol for navigation tree caching."""

    def get_navigation(self) -> NavigationTreeDict | None:
        """Retrieve cached navigation tree."""
        ...

    def set_navigation(self, navigation: NavigationTreeDict) -> None:
        """Store navigation tree in cache."""
        ...

    def invalidate_navigation(self) -> None:
        """Remove cached navigation tree."""
        ...


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
    """Builds site structure and navigation from directory structure.

    Scans the source directory for markdown files and builds a Site structure.
    Uses index.md files as section landing pages. Extracts titles from the
    first H1 heading in each document, falling back to filename-based titles.
    """

    def __init__(
        self,
        source_dir: Path,
        cache: NavigationCache | None = None,
    ) -> None:
        """Initialize builder.

        Args:
            source_dir: Root directory containing markdown sources
            cache: Optional cache implementing NavigationCache protocol
        """
        self._source_dir = source_dir
        self._cache = cache
        self._site: Site | None = None

    @property
    def source_dir(self) -> Path:
        """Root directory containing markdown sources."""
        return self._source_dir

    def build_site(self, *, use_cache: bool = True) -> Site:
        """Build site structure from directory.

        Args:
            use_cache: Whether to use cached data if available

        Returns:
            Site with all discovered documents
        """
        if use_cache and self._cache is not None:
            cached = self._cache.get_navigation()
            if cached is not None:
                self._site = self._site_from_cached(cached)
                return self._site

        self._site = self._build_site_from_filesystem()

        if self._cache is not None:
            nav = build_navigation(self._site)
            self._cache.set_navigation({"items": [item.to_dict() for item in nav]})

        return self._site

    def build(self, *, use_cache: bool = True) -> list[NavItem]:
        """Build navigation tree from directory structure.

        Args:
            use_cache: Whether to use cached navigation if available

        Returns:
            List of NavItem for navigation UI
        """
        site = self.build_site(use_cache=use_cache)
        return build_navigation(site)

    def invalidate(self) -> None:
        """Invalidate cached navigation tree."""
        self._site = None
        if self._cache is not None:
            self._cache.invalidate_navigation()

    def get_subtree(self, path: str) -> list[NavItem] | None:
        """Get navigation subtree for a specific section.

        Args:
            path: Section path (e.g., "domain-a/subdomain")

        Returns:
            List of NavItem for the section, or None if not found
        """
        site = self.build_site()

        if not path:
            return build_navigation(site)

        normalized = path if path.startswith("/") else f"/{path}"
        page = site.get_page(normalized)
        if page is None:
            return None

        return build_navigation(site, normalized)

    def _build_site_from_filesystem(self) -> Site:
        """Scan filesystem and build site structure."""
        builder = SiteBuilder()

        if self._source_dir.exists():
            self._scan_directory(self._source_dir, "", builder, None)

        return builder.build()

    def _scan_directory(
        self,
        dir_path: Path,
        base_path: str,
        builder: SiteBuilder,
        parent_idx: int | None,
    ) -> list[int]:
        """Recursively scan directory for pages.

        Returns list of page indices added at this level.
        """
        indices: list[int] = []

        entries = sorted(
            dir_path.iterdir(),
            key=lambda p: (not p.is_dir(), p.name.lower()),
        )

        for entry in entries:
            if entry.name.startswith(".") or entry.name.startswith("_"):
                continue

            if entry.is_dir():
                result = self._process_directory(entry, base_path, builder, parent_idx)
                if result is not None:
                    indices.extend(result)
            elif entry.suffix == ".md" and entry.name != "index.md":
                idx = self._process_file(entry, base_path, builder, parent_idx)
                indices.append(idx)

        return indices

    def _process_directory(
        self,
        dir_path: Path,
        base_path: str,
        builder: SiteBuilder,
        parent_idx: int | None,
    ) -> list[int] | None:
        """Process a directory into page(s)."""
        dir_name = dir_path.name
        item_path = f"{base_path}/{dir_name}" if base_path else f"/{dir_name}"

        index_file = dir_path / "index.md"

        if not index_file.exists():
            # No index.md - promote children to parent level
            child_indices = self._scan_directory(
                dir_path, item_path, builder, parent_idx
            )
            return child_indices if child_indices else None

        # Create page for this directory
        title = self._extract_title(index_file) or self._title_from_name(dir_name)
        page_idx = builder.add_page(title, item_path, parent_idx)

        # Scan children with this page as parent
        self._scan_directory(dir_path, item_path, builder, page_idx)

        return [page_idx]

    def _process_file(
        self,
        file_path: Path,
        base_path: str,
        builder: SiteBuilder,
        parent_idx: int | None,
    ) -> int:
        """Process a markdown file into a page."""
        file_name = file_path.stem
        item_path = f"{base_path}/{file_name}" if base_path else f"/{file_name}"

        title = self._extract_title(file_path) or self._title_from_name(file_name)
        return builder.add_page(title, item_path, parent_idx)

    def _extract_title(self, file_path: Path) -> str | None:
        """Extract title from first H1 heading in markdown file."""
        try:
            content = file_path.read_text(encoding="utf-8")
        except OSError:
            return None

        match = re.search(r"^#\s+(.+)$", content, re.MULTILINE)
        if match:
            return match.group(1).strip()

        return None

    def _title_from_name(self, name: str) -> str:
        """Generate title from file/directory name."""
        title = name.replace("-", " ").replace("_", " ")
        return title.title()

    def _site_from_cached(self, cached: NavigationTreeDict) -> Site:
        """Reconstruct Site from cached navigation dict."""
        builder = SiteBuilder()
        self._cached_items_to_pages(cached["items"], builder, None)
        return builder.build()

    def _cached_items_to_pages(
        self,
        items: list[NavItemDict],
        builder: SiteBuilder,
        parent_idx: int | None,
    ) -> None:
        """Reconstruct pages from cached navigation items."""
        for data in items:
            idx = builder.add_page(data["title"], data["path"], parent_idx)
            children = data.get("children", [])
            if children:
                self._cached_items_to_pages(children, builder, idx)
