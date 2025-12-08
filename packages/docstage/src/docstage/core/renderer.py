"""Markdown rendering with caching.

Wraps the Rust core converter with file-based caching and mtime tracking.
"""

from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

from docstage_core import MarkdownConverter

from docstage.core.cache import CacheEntry, FileCache
from docstage.core.diagrams import render_diagrams_with_cache, replace_diagram_placeholders


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
    warnings: list[str]


class PageRenderer:
    """Renders markdown documents with caching.

    Uses the Rust core for actual conversion and FileCache for persistence.
    Cache invalidation is based on source file mtime.

    When kroki_url is provided, diagram code blocks (plantuml, mermaid, graphviz, etc.)
    are rendered as images via Kroki. Otherwise they appear as syntax-highlighted code.
    """

    def __init__(
        self,
        source_dir: Path,
        cache: FileCache,
        *,
        extract_title: bool = True,
        kroki_url: str | None = None,
        include_dirs: list[Path] | None = None,
        config_file: str | None = None,
        dpi: int = 192,
    ) -> None:
        """Initialize renderer.

        Args:
            source_dir: Root directory containing markdown sources
            cache: FileCache instance for caching rendered content
            extract_title: Whether to extract title from first H1
            kroki_url: Kroki server URL for diagram rendering (e.g., "https://kroki.io").
                       If None, diagrams are rendered as code blocks.
            include_dirs: Directories to search for PlantUML !include files.
                          If None, defaults to [source_dir] when kroki_url is set.
            config_file: PlantUML config file name (searched in include_dirs)
            dpi: DPI for diagram rendering (default: 192 for retina)
        """
        self._source_dir = source_dir
        self._cache = cache
        self._extract_title = extract_title
        self._kroki_url = kroki_url
        self._include_dirs = include_dirs
        self._config_file = config_file
        self._dpi = dpi

        effective_include_dirs: list[Path] = include_dirs or ([source_dir] if kroki_url else [])
        converter_include_dirs = [str(d) for d in effective_include_dirs] if effective_include_dirs else None
        self._converter = MarkdownConverter(
            gfm=True,
            extract_title=extract_title,
            include_dirs=converter_include_dirs,
            config_file=config_file,
            dpi=dpi,
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
            return _from_cache(cached, source_path)

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
            warnings=list(result.warnings),
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

    def _render_fresh(self, source_path: Path) -> "_FreshRenderResult":
        """Render markdown from source file.

        Args:
            source_path: Path to markdown file

        Returns:
            _FreshRenderResult with HTML, title, ToC, and warnings
        """
        markdown_text = source_path.read_text(encoding="utf-8")

        if self._kroki_url:
            extract_result = self._converter.extract_html_with_diagrams(markdown_text)

            if extract_result.diagrams:
                diagrams_input = [
                    (d.index, d.source, d.endpoint, d.format) for d in extract_result.diagrams
                ]
                rendered_diagrams = render_diagrams_with_cache(
                    diagrams_input, self._kroki_url, self._cache
                )
                html = replace_diagram_placeholders(extract_result.html, rendered_diagrams)
            else:
                html = extract_result.html

            return _FreshRenderResult(
                html=html,
                title=extract_result.title,
                toc=list(extract_result.toc),
                warnings=list(extract_result.warnings),
            )

        result = self._converter.convert_html(markdown_text)
        return _FreshRenderResult(
            html=result.html,
            title=result.title,
            toc=list(result.toc),
            warnings=list(result.warnings),
        )


@dataclass
class _FreshRenderResult:
    """Internal result from fresh rendering."""

    html: str
    title: str | None
    toc: list
    warnings: list[str]


def _from_cache(cached: CacheEntry, source_path: Path) -> RenderResult:
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
        warnings=[],  # Warnings are not cached
    )


@dataclass
class _CachedTocEntry:
    """Reconstructed TocEntry from cache.

    Mimics the TocEntry interface from docstage_core.
    """

    level: int
    title: str
    id: str
