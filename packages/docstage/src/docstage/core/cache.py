"""File-based cache with mtime invalidation.

Cache structure:
    .cache/
    ├── pages/
    │   └── domain-a/
    │       └── subdomain/
    │           └── guide.html       # Rendered HTML
    ├── meta/
    │   └── domain-a/
    │       └── subdomain/
    │           └── guide.json       # Extracted metadata
    ├── diagrams/
    │   └── <content_hash>.svg       # Rendered SVG diagrams
    │   └── <content_hash>.png       # Rendered PNG diagrams (base64 data URI)
    └── site.json                    # Site structure
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, TypedDict

from docstage.core.types import URLPath

if TYPE_CHECKING:
    from docstage.core.site import Site


class CachedMetadata(TypedDict):
    """Cached page metadata structure."""

    title: str | None
    source_mtime: float
    toc: list[dict[str, str | int]]


@dataclass
class CacheEntry:
    """Result of cache lookup."""

    html: str
    meta: CachedMetadata


def compute_diagram_hash(source: str, endpoint: str, fmt: str, dpi: int = 192) -> str:
    """Compute a content hash for diagram caching.

    Args:
        source: Diagram source code
        endpoint: Kroki endpoint (e.g., "plantuml", "mermaid")
        fmt: Output format ("svg" or "png")
        dpi: DPI used for rendering (affects SVG scaling)

    Returns:
        SHA-256 hash of the combined inputs
    """
    content = f"{endpoint}:{fmt}:{dpi}:{source}"
    return hashlib.sha256(content.encode()).hexdigest()


class FileCache:
    """File-based cache for rendered HTML and metadata.

    Uses source file mtime for invalidation. Cache entries are considered valid
    when the cached mtime matches the current source file mtime.
    """

    _GITIGNORE_CONTENT = "# Ignore everything in this directory\n*\n"

    def __init__(self, cache_dir: Path) -> None:
        """Initialize cache with directory path.

        Args:
            cache_dir: Root directory for cache files (e.g., .cache/)
        """
        self._cache_dir = cache_dir
        self._pages_dir = cache_dir / "pages"
        self._meta_dir = cache_dir / "meta"
        self._diagrams_dir = cache_dir / "diagrams"

    @property
    def cache_dir(self) -> Path:
        """Root cache directory."""
        return self._cache_dir

    def _ensure_cache_dir(self) -> None:
        """Create cache directory with .gitignore if it doesn't exist."""
        if not self._cache_dir.exists():
            self._cache_dir.mkdir(parents=True, exist_ok=True)
            gitignore_path = self._cache_dir / ".gitignore"
            gitignore_path.write_text(self._GITIGNORE_CONTENT, encoding="utf-8")

    def get(self, path: str, source_mtime: float) -> CacheEntry | None:
        """Retrieve cached entry if valid.

        Args:
            path: Document path (e.g., "domain-a/subdomain/guide")
            source_mtime: Current mtime of source file

        Returns:
            CacheEntry if cache hit and valid, None otherwise
        """
        html_path = self._pages_dir / f"{path}.html"
        meta_path = self._meta_dir / f"{path}.json"

        if not html_path.exists() or not meta_path.exists():
            return None

        meta = self._read_meta(meta_path)
        if meta is None:
            return None

        if meta["source_mtime"] != source_mtime:
            return None

        try:
            html = html_path.read_text(encoding="utf-8")
        except OSError:
            return None

        return CacheEntry(html=html, meta=meta)

    def set(
        self,
        path: str,
        html: str,
        title: str | None,
        source_mtime: float,
        toc: list[dict[str, str | int]],
    ) -> None:
        """Store entry in cache.

        Args:
            path: Document path (e.g., "domain-a/subdomain/guide")
            html: Rendered HTML content
            title: Extracted title (or None)
            source_mtime: Source file mtime for invalidation
            toc: Table of contents entries
        """
        self._ensure_cache_dir()

        html_path = self._pages_dir / f"{path}.html"
        meta_path = self._meta_dir / f"{path}.json"

        html_path.parent.mkdir(parents=True, exist_ok=True)
        meta_path.parent.mkdir(parents=True, exist_ok=True)

        html_path.write_text(html, encoding="utf-8")

        meta: CachedMetadata = {
            "title": title,
            "source_mtime": source_mtime,
            "toc": toc,
        }
        meta_path.write_text(json.dumps(meta), encoding="utf-8")

    def invalidate(self, path: str) -> None:
        """Remove entry from cache.

        Args:
            path: Document path to invalidate
        """
        html_path = self._pages_dir / f"{path}.html"
        meta_path = self._meta_dir / f"{path}.json"

        if html_path.exists():
            html_path.unlink()
        if meta_path.exists():
            meta_path.unlink()

    def clear(self) -> None:
        """Remove all cached entries."""
        import shutil

        if self._pages_dir.exists():
            shutil.rmtree(self._pages_dir)
        if self._meta_dir.exists():
            shutil.rmtree(self._meta_dir)

    def get_site(self) -> Site | None:
        """Retrieve cached site structure.

        Returns:
            Site if exists, None otherwise
        """
        from docstage.core.site import Page, Site

        site_path = self._cache_dir / "site.json"
        if not site_path.exists():
            return None

        try:
            data = json.loads(site_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return None

        # Reconstruct Site from cached data
        source_dir = Path(data["source_dir"])
        pages = [
            Page(
                title=p["title"],
                path=URLPath(p["path"]),
                source_path=Path(p["source_path"]),
            )
            for p in data["pages"]
        ]
        return Site(
            source_dir=source_dir,
            pages=pages,
            children=data["children"],
            parents=data["parents"],
            roots=data["roots"],
        )

    def set_site(self, site: Site) -> None:
        """Store site structure in cache.

        Args:
            site: Site to cache
        """
        self._ensure_cache_dir()
        site_path = self._cache_dir / "site.json"

        # Serialize Site to JSON-compatible dict
        data = {
            "source_dir": str(site.source_dir),
            "pages": [
                {"title": p.title, "path": p.path, "source_path": str(p.source_path)}
                for p in site._pages
            ],
            "children": site._children,
            "parents": site._parents,
            "roots": site._roots,
        }
        site_path.write_text(json.dumps(data), encoding="utf-8")

    def invalidate_site(self) -> None:
        """Remove cached site structure."""
        site_path = self._cache_dir / "site.json"
        if site_path.exists():
            site_path.unlink()

    def get_diagram(self, content_hash: str, fmt: str) -> str | None:
        """Retrieve cached diagram by content hash.

        Args:
            content_hash: SHA-256 hash of diagram content
            fmt: Output format ("svg" or "png")

        Returns:
            Cached diagram content (SVG string or PNG data URI), or None if not cached
        """
        diagram_path = self._diagrams_dir / f"{content_hash}.{fmt}"
        if not diagram_path.exists():
            return None

        try:
            return diagram_path.read_text(encoding="utf-8")
        except OSError:
            return None

    def set_diagram(self, content_hash: str, fmt: str, content: str) -> None:
        """Store rendered diagram in cache.

        Args:
            content_hash: SHA-256 hash of diagram content
            fmt: Output format ("svg" or "png")
            content: Rendered diagram (SVG string or PNG data URI)
        """
        self._ensure_cache_dir()
        self._diagrams_dir.mkdir(parents=True, exist_ok=True)
        diagram_path = self._diagrams_dir / f"{content_hash}.{fmt}"
        diagram_path.write_text(content, encoding="utf-8")

    def _read_meta(self, meta_path: Path) -> CachedMetadata | None:
        """Read and validate metadata file.

        Args:
            meta_path: Path to metadata JSON file

        Returns:
            CachedMetadata if valid, None otherwise
        """
        try:
            data = json.loads(meta_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return None

        if not isinstance(data, dict) or "source_mtime" not in data or "toc" not in data:
            return None

        return CachedMetadata(
            title=data.get("title"),
            source_mtime=data["source_mtime"],
            toc=data["toc"],
        )
