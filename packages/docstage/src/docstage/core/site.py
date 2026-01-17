"""Site structure for document hierarchy.

Provides the core document site structure with efficient path lookups
and traversal operations. Includes SiteLoader for building sites from
the filesystem.
"""

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

from docstage.core.types import URLPath


@dataclass(frozen=True)
class Page:
    """Document page data."""

    title: str
    path: URLPath
    source_path: Path  # Relative path to source .md file (e.g., Path("guide.md"))


@dataclass(frozen=True)
class BreadcrumbItem:
    """Breadcrumb navigation item."""

    title: str
    path: URLPath

    def to_dict(self) -> dict[str, str]:
        """Convert to dictionary for JSON serialization."""
        return {"title": self.title, "path": self.path}


class Site:
    """Document site structure with efficient path lookups.

    Stores pages in a flat list with parent/children relationships
    tracked by indices. Provides O(1) URL path and source path lookups,
    and O(d) breadcrumb building where d is the page depth.
    """

    __slots__ = (
        "_children",
        "_pages",
        "_parents",
        "_path_index",
        "_roots",
        "_source_dir",
        "_source_path_index",
    )

    def __init__(
        self,
        source_dir: Path,
        pages: list[Page],
        children: list[list[int]],
        parents: list[int | None],
        roots: list[int],
    ) -> None:
        """Initialize site structure.

        Args:
            source_dir: Root directory containing markdown sources
            pages: Flat list of all pages
            children: Children indices for each page
            parents: Parent index for each page (None for roots)
            roots: Indices of root pages
        """
        self._source_dir = source_dir
        self._pages = pages
        self._children = children
        self._parents = parents
        self._roots = roots
        self._path_index = {page.path: i for i, page in enumerate(pages)}
        self._source_path_index = {page.source_path: i for i, page in enumerate(pages)}

    @property
    def source_dir(self) -> Path:
        """Root directory containing markdown sources."""
        return self._source_dir

    def get_page(self, path: URLPath) -> Page | None:
        """Get page by path.

        Args:
            path: Page path (e.g., URLPath("domain/page") or URLPath("/domain/page"))

        Returns:
            Page if found, None otherwise
        """
        normalized = self._normalize_path(path)
        idx = self._path_index.get(normalized)
        if idx is None:
            return None
        return self._pages[idx]

    def get_children(self, path: URLPath) -> list[Page]:
        """Get children of a page.

        Args:
            path: Page path (e.g., URLPath("domain/page") or URLPath("/domain/page"))

        Returns:
            List of child Pages, empty if not found or no children
        """
        normalized = self._normalize_path(path)
        idx = self._path_index.get(normalized)
        if idx is None:
            return []
        return [self._pages[i] for i in self._children[idx]]

    def get_breadcrumbs(self, path: URLPath) -> list[BreadcrumbItem]:
        """Build breadcrumbs for a given path.

        Returns breadcrumbs starting with "Home" for non-root pages,
        followed by ancestor pages. The current page is not included.

        Note:
            For unknown paths, returns [Home] to provide minimal navigation
            in UI even when the page doesn't exist in the site structure.
            This differs from get_page() which returns None for unknown paths.

        Args:
            path: Page path (e.g., URLPath("domain/page") or URLPath("/domain/page"))

        Returns:
            List of BreadcrumbItem for ancestor navigation
        """
        if not path:
            return []

        normalized = self._normalize_path(path)
        idx = self._path_index.get(normalized)
        if idx is None:
            return [BreadcrumbItem(title="Home", path=URLPath("/"))]

        # Walk up parent chain
        ancestors: list[Page] = []
        current: int | None = idx
        while current is not None:
            ancestors.append(self._pages[current])
            current = self._parents[current]

        # Reverse to root-first, exclude current page and root index.md
        # (Home breadcrumb already represents "/" so root page would be duplicate)
        ancestors.reverse()
        breadcrumbs = [BreadcrumbItem(title="Home", path=URLPath("/"))]
        for page in ancestors[:-1]:
            if page.path != "/":
                breadcrumbs.append(BreadcrumbItem(title=page.title, path=page.path))

        return breadcrumbs

    def get_root_pages(self) -> list[Page]:
        """Get root-level pages."""
        return [self._pages[i] for i in self._roots]

    def resolve_source_path(self, path: URLPath) -> Path | None:
        """Resolve URL path to absolute source file path.

        Args:
            path: URL path (e.g., "domain/page" or "/domain/page")

        Returns:
            Absolute path to source markdown file, or None if page not found
        """
        page = self.get_page(path)
        if page is None:
            return None
        return self._source_dir / page.source_path

    def get_page_by_source(self, source_path: Path) -> Page | None:
        """Get page by source file path.

        Args:
            source_path: Relative path to source file (e.g., Path("guide.md"))

        Returns:
            Page if found, None otherwise
        """
        idx = self._source_path_index.get(source_path)
        if idx is None:
            return None
        return self._pages[idx]

    def _normalize_path(self, path: URLPath) -> URLPath:
        """Normalize path to have leading slash."""
        return URLPath(f"/{path.lstrip('/')}")


class SiteBuilder:
    """Builder for constructing Site instances."""

    def __init__(self, source_dir: Path) -> None:
        self._source_dir = source_dir
        self._pages: list[Page] = []
        self._children: list[list[int]] = []
        self._parents: list[int | None] = []
        self._roots: list[int] = []

    def add_page(
        self,
        title: str,
        path: URLPath,
        source_path: Path,
        parent_idx: int | None = None,
    ) -> int:
        """Add a page to the site.

        Args:
            title: Page title
            path: URL path (e.g., "/guide")
            source_path: Relative path to source file (e.g., Path("guide.md"))
            parent_idx: Index of parent page, None for root

        Returns:
            Index of the added page
        """
        idx = len(self._pages)
        self._pages.append(Page(title=title, path=path, source_path=source_path))
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
            source_dir=self._source_dir,
            pages=self._pages,
            children=self._children,
            parents=self._parents,
            roots=self._roots,
        )


class SiteCache(Protocol):
    """Protocol for site data caching."""

    def get_site(self) -> Site | None:
        """Retrieve cached site."""
        ...

    def set_site(self, site: Site) -> None:
        """Store site in cache."""
        ...

    def invalidate_site(self) -> None:
        """Remove cached site."""
        ...


class SiteLoader:
    """Loads site structure from filesystem.

    Scans a source directory for markdown files and builds a Site structure.
    Uses index.md files as section landing pages. Extracts titles from the
    first H1 heading in each document, falling back to filename-based titles.
    """

    def __init__(
        self,
        source_dir: Path,
        cache: SiteCache | None = None,
    ) -> None:
        """Initialize loader.

        Args:
            source_dir: Root directory containing markdown sources
            cache: Optional cache implementing SiteCache protocol
        """
        self._source_dir = source_dir
        self._cache = cache
        self._site: Site | None = None

    def load(self, *, use_cache: bool = True) -> Site:
        """Load site structure from directory.

        Args:
            use_cache: Whether to use cached data if available

        Returns:
            Site with all discovered documents
        """
        # Return in-memory cached Site if available
        if use_cache and self._site is not None:
            return self._site

        if use_cache and self._cache is not None:
            cached = self._cache.get_site()
            if cached is not None:
                self._site = cached
                return self._site

        self._site = self._load_from_filesystem()

        if self._cache is not None:
            self._cache.set_site(self._site)

        return self._site

    def invalidate(self) -> None:
        """Invalidate cached site."""
        self._site = None
        if self._cache is not None:
            self._cache.invalidate_site()

    def _load_from_filesystem(self) -> Site:
        """Scan filesystem and build site structure."""
        builder = SiteBuilder(self._source_dir)

        if self._source_dir.exists():
            # Handle root index.md specially
            root_index = self._source_dir / "index.md"
            root_idx: int | None = None
            if root_index.exists():
                title = self._extract_title(root_index) or "Home"
                source_path = root_index.relative_to(self._source_dir)
                root_idx = builder.add_page(title, URLPath("/"), source_path, None)

            self._scan_directory(self._source_dir, "", builder, root_idx)

        return builder.build()

    def _scan_directory(
        self,
        dir_path: Path,
        base_path: str,
        builder: SiteBuilder,
        parent_idx: int | None,
    ) -> list[int]:
        """Recursively scan directory and add pages to builder.

        Args:
            dir_path: Directory to scan
            base_path: URL path prefix for items in this directory
            builder: SiteBuilder to add pages to
            parent_idx: Index of parent page in builder

        Returns:
            List of page indices added at this directory level
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
        source_path = index_file.relative_to(self._source_dir)
        page_idx = builder.add_page(title, URLPath(item_path), source_path, parent_idx)

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
        source_path = file_path.relative_to(self._source_dir)
        return builder.add_page(title, URLPath(item_path), source_path, parent_idx)

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
