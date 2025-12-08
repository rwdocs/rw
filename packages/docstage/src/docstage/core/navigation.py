"""Navigation tree builder from directory structure.

Builds navigation trees by scanning the filesystem. Uses index.md files
as section landing pages and extracts titles from the first H1 heading
in each document.
"""

import re
from dataclasses import dataclass, field
from pathlib import Path

from docstage.core.cache import FileCache


@dataclass
class NavItem:
    """Navigation tree item."""

    title: str
    path: str
    children: list["NavItem"] = field(default_factory=list)

    def to_dict(self) -> dict:
        """Convert to dictionary for JSON serialization."""
        result: dict = {
            "title": self.title,
            "path": self.path,
        }
        if self.children:
            result["children"] = [child.to_dict() for child in self.children]
        return result


@dataclass
class NavigationTree:
    """Complete navigation tree."""

    items: list[NavItem]

    def to_dict(self) -> dict:
        """Convert to dictionary for JSON serialization."""
        return {
            "items": [item.to_dict() for item in self.items],
        }


class NavigationBuilder:
    """Builds navigation trees from directory structure.

    Scans the source directory for markdown files and builds a tree structure.
    Uses index.md files as section landing pages. Extracts titles from the
    first H1 heading in each document, falling back to filename-based titles.
    """

    def __init__(
        self,
        source_dir: Path,
        cache: FileCache | None = None,
    ) -> None:
        """Initialize builder.

        Args:
            source_dir: Root directory containing markdown sources
            cache: Optional FileCache for caching navigation tree
        """
        self._source_dir = source_dir
        self._cache = cache

    @property
    def source_dir(self) -> Path:
        """Root directory containing markdown sources."""
        return self._source_dir

    def build(self, *, use_cache: bool = True) -> NavigationTree:
        """Build navigation tree from directory structure.

        Args:
            use_cache: Whether to use cached navigation if available

        Returns:
            NavigationTree with all discovered documents
        """
        if use_cache and self._cache is not None:
            cached = self._cache.get_navigation()
            if cached is not None:
                return self._from_cached(cached)

        tree = self._build_from_filesystem()

        if self._cache is not None:
            self._cache.set_navigation(tree.to_dict())

        return tree

    def invalidate(self) -> None:
        """Invalidate cached navigation tree."""
        if self._cache is not None:
            self._cache.invalidate_navigation()

    def get_subtree(self, path: str) -> NavigationTree | None:
        """Get navigation subtree for a specific section.

        Args:
            path: Section path (e.g., "domain-a/subdomain")

        Returns:
            NavigationTree for the section, or None if not found
        """
        tree = self.build()
        parts = path.strip("/").split("/") if path else []

        items = tree.items
        for part in parts:
            found = None
            for item in items:
                item_last_part = (
                    item.path.strip("/").split("/")[-1] if item.path else ""
                )
                if item_last_part == part:
                    found = item
                    break
            if found is None:
                return None
            items = found.children

        return NavigationTree(items=items)

    def _build_from_filesystem(self) -> NavigationTree:
        """Scan filesystem and build navigation tree.

        Returns:
            NavigationTree with all discovered documents
        """
        if not self._source_dir.exists():
            return NavigationTree(items=[])

        items = self._scan_directory(self._source_dir, "")
        return NavigationTree(items=items)

    def _scan_directory(self, dir_path: Path, base_path: str) -> list[NavItem]:
        """Recursively scan directory for navigation items.

        Args:
            dir_path: Directory to scan
            base_path: URL path prefix for items in this directory

        Returns:
            List of NavItem for this directory level
        """
        items: list[NavItem] = []

        entries = sorted(
            dir_path.iterdir(), key=lambda p: (not p.is_dir(), p.name.lower())
        )

        for entry in entries:
            if entry.name.startswith(".") or entry.name.startswith("_"):
                continue

            if entry.is_dir():
                result = self._process_directory(entry, base_path)
                if result is None:
                    continue
                if isinstance(result, list):
                    items.extend(result)
                else:
                    items.append(result)
            elif entry.suffix == ".md" and entry.name != "index.md":
                item = self._process_file(entry, base_path)
                items.append(item)

        return items

    def _process_directory(
        self, dir_path: Path, base_path: str
    ) -> NavItem | list[NavItem] | None:
        """Process a directory into navigation item(s).

        Args:
            dir_path: Directory to process
            base_path: URL path prefix

        Returns:
            NavItem for the directory if it has index.md,
            list of child NavItems if directory has no index.md (promoted children),
            or None if empty
        """
        dir_name = dir_path.name
        item_path = f"{base_path}/{dir_name}" if base_path else f"/{dir_name}"

        index_file = dir_path / "index.md"
        children = self._scan_directory(dir_path, item_path)

        if not index_file.exists():
            # No index.md - promote children to parent level, skip this directory
            return children if children else None

        title = self._extract_title(index_file) or self._title_from_name(dir_name)
        return NavItem(title=title, path=item_path, children=children)

    def _process_file(self, file_path: Path, base_path: str) -> NavItem:
        """Process a markdown file into a navigation item.

        Args:
            file_path: Markdown file to process
            base_path: URL path prefix

        Returns:
            NavItem for the file
        """
        file_name = file_path.stem
        item_path = f"{base_path}/{file_name}" if base_path else f"/{file_name}"

        title = self._extract_title(file_path) or self._title_from_name(file_name)

        return NavItem(title=title, path=item_path)

    def _extract_title(self, file_path: Path) -> str | None:
        """Extract title from first H1 heading in markdown file.

        Args:
            file_path: Path to markdown file

        Returns:
            Title string, or None if no H1 found
        """
        try:
            content = file_path.read_text(encoding="utf-8")
        except OSError:
            return None

        # Match first H1 heading (# Title)
        match = re.search(r"^#\s+(.+)$", content, re.MULTILINE)
        if match:
            return match.group(1).strip()

        return None

    def _title_from_name(self, name: str) -> str:
        """Generate title from file/directory name.

        Converts kebab-case and snake_case to Title Case.

        Args:
            name: File or directory name (without extension)

        Returns:
            Human-readable title
        """
        # Replace hyphens and underscores with spaces
        title = name.replace("-", " ").replace("_", " ")
        # Title case
        return title.title()

    def _from_cached(self, cached: dict) -> NavigationTree:
        """Reconstruct NavigationTree from cached dict.

        Args:
            cached: Cached navigation dictionary

        Returns:
            NavigationTree instance
        """
        items = [self._dict_to_nav_item(item) for item in cached.get("items", [])]
        return NavigationTree(items=items)

    def _dict_to_nav_item(self, data: dict) -> NavItem:
        """Reconstruct NavItem from dictionary.

        Args:
            data: Dictionary with title, path, and optional children

        Returns:
            NavItem instance
        """
        children = [self._dict_to_nav_item(child) for child in data.get("children", [])]
        return NavItem(
            title=data["title"],
            path=data["path"],
            children=children,
        )
