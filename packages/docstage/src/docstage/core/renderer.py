"""Markdown rendering with caching.

Wraps the Rust core converter with file-based caching and mtime tracking.
"""

from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

from docstage_core import HtmlConvertResult, MarkdownConverter

from docstage.core.cache import CacheEntry, FileCache


class TocEntryProtocol(Protocol):
    """Protocol for table of contents entries.

    Compatible with both Rust TocEntry and Python _CachedTocEntry.
    """

    @property
    def level(self) -> int: ...

    @property
    def title(self) -> str: ...

    @property
    def id(self) -> str: ...


@dataclass
class RenderResult:
    """Result of rendering a markdown document."""

    html: str
    title: str | None
    toc: list[TocEntryProtocol]
    source_path: Path
    from_cache: bool


class PageRenderer:
    """Renders markdown documents with caching.

    Uses the Rust core for actual conversion and FileCache for persistence.
    Cache invalidation is based on source file mtime.
    """

    def __init__(
        self,
        source_dir: Path,
        cache: FileCache,
        *,
        extract_title: bool = True,
    ) -> None:
        """Initialize renderer.

        Args:
            source_dir: Root directory containing markdown sources
            cache: FileCache instance for caching rendered content
            extract_title: Whether to extract title from first H1
        """
        self._source_dir = source_dir
        self._cache = cache
        self._extract_title = extract_title
        self._converter = MarkdownConverter(
            gfm=True,
            extract_title=extract_title,
        )

    @property
    def source_dir(self) -> Path:
        """Root directory containing markdown sources."""
        return self._source_dir

    def render(self, path: str) -> RenderResult:
        """Render a markdown document.

        Args:
            path: Document path relative to source_dir (without .md extension)
                  e.g., "domain-a/subdomain/guide"

        Returns:
            RenderResult with HTML, title, and ToC

        Raises:
            FileNotFoundError: If source markdown file doesn't exist
        """
        source_path = self._resolve_source_path(path)
        if not source_path.exists():
            raise FileNotFoundError(f"Source file not found: {source_path}")

        source_mtime = source_path.stat().st_mtime

        cached = self._cache.get(path, source_mtime)
        if cached is not None:
            return self._from_cache(cached, source_path)

        result = self._render_fresh(source_path)
        self._cache.set(
            path,
            result.html,
            result.title,
            source_mtime,
            [{"level": e.level, "title": e.title, "id": e.id} for e in result.toc],
        )

        return RenderResult(
            html=result.html,
            title=result.title,
            toc=list(result.toc),
            source_path=source_path,
            from_cache=False,
        )

    def invalidate(self, path: str) -> None:
        """Invalidate cached content for a path.

        Args:
            path: Document path to invalidate
        """
        self._cache.invalidate(path)

    def _resolve_source_path(self, path: str) -> Path:
        """Resolve document path to source file.

        Handles index.md convention for directories.

        Args:
            path: Document path (e.g., "domain-a/guide")

        Returns:
            Path to source markdown file
        """
        source_path = self._source_dir / f"{path}.md"

        # If path.md doesn't exist, check for path/index.md
        if not source_path.exists():
            index_path = self._source_dir / path / "index.md"
            if index_path.exists():
                return index_path

        return source_path

    def _render_fresh(self, source_path: Path) -> HtmlConvertResult:
        """Render markdown from source file.

        Args:
            source_path: Path to markdown file

        Returns:
            HtmlConvertResult from Rust converter
        """
        markdown_text = source_path.read_text(encoding="utf-8")
        return self._converter.convert_html(markdown_text)

    def _from_cache(self, cached: CacheEntry, source_path: Path) -> RenderResult:
        """Create RenderResult from cache entry.

        Args:
            cached: Cache entry with HTML and metadata
            source_path: Source file path

        Returns:
            RenderResult reconstructed from cache
        """
        toc_entries: list[TocEntryProtocol] = [
            _CachedTocEntry(
                level=int(entry["level"]),
                title=str(entry["title"]),
                id=str(entry["id"]),
            )
            for entry in cached.meta["toc"]
        ]

        return RenderResult(
            html=cached.html,
            title=cached.meta["title"],
            toc=toc_entries,
            source_path=source_path,
            from_cache=True,
        )


@dataclass
class _CachedTocEntry:
    """Reconstructed TocEntry from cache.

    Mimics the TocEntry interface from docstage_core.
    """

    level: int
    title: str
    id: str
