"""Diagram rendering with caching support.

Renders diagrams via Kroki with content-based caching to avoid redundant requests.
"""

import base64
import re
import urllib.error
import urllib.request
import zlib
from dataclasses import dataclass

from docstage.core.cache import PageCache, compute_diagram_hash

GOOGLE_FONTS_RE = re.compile(r"@import\s+url\([^)]*fonts\.googleapis\.com[^)]*\)\s*;?")

# Regex patterns for SVG dimension scaling
SVG_WIDTH_RE = re.compile(r'(<svg[^>]*\s)width="(\d+)(?:px)?"')
SVG_HEIGHT_RE = re.compile(r'(<svg[^>]*\s)height="(\d+)(?:px)?"')
STYLE_WIDTH_RE = re.compile(r"(width:\s*)(\d+)(px)")
STYLE_HEIGHT_RE = re.compile(r"(height:\s*)(\d+)(px)")

# Standard display DPI used as baseline for scaling
STANDARD_DPI = 96


@dataclass
class DiagramToRender:
    """A diagram that needs to be rendered via Kroki."""

    index: int
    source: str
    endpoint: str
    format: str
    content_hash: str


@dataclass
class RenderedDiagram:
    """A rendered diagram ready for HTML insertion."""

    index: int
    content: str
    format: str


def render_diagrams_with_cache(
    diagrams: list[tuple[int, str, str, str]],
    kroki_url: str,
    cache: PageCache,
    dpi: int = 192,
) -> list[RenderedDiagram]:
    """Render diagrams via Kroki with caching.

    SVG diagrams are scaled based on DPI before caching. This ensures
    diagrams display at their intended physical size.

    Args:
        diagrams: List of (index, source, endpoint, format) tuples
        kroki_url: Kroki server URL
        cache: FileCache for diagram caching
        dpi: DPI used for diagram rendering (for scaling SVG dimensions)

    Returns:
        List of RenderedDiagram with content ready for HTML
    """
    results: list[RenderedDiagram] = []
    to_render: list[DiagramToRender] = []

    for index, source, endpoint, fmt in diagrams:
        content_hash = compute_diagram_hash(source, endpoint, fmt, dpi)
        cached = cache.get_diagram(content_hash, fmt)

        if cached is not None:
            results.append(RenderedDiagram(index=index, content=cached, format=fmt))
        else:
            to_render.append(
                DiagramToRender(
                    index=index,
                    source=source,
                    endpoint=endpoint,
                    format=fmt,
                    content_hash=content_hash,
                ),
            )

    if to_render:
        rendered = _render_via_kroki(to_render, kroki_url, dpi)
        for diagram, content in rendered:
            cache.set_diagram(diagram.content_hash, diagram.format, content)
            results.append(
                RenderedDiagram(
                    index=diagram.index,
                    content=content,
                    format=diagram.format,
                ),
            )

    results.sort(key=lambda r: r.index)
    return results


def _render_via_kroki(
    diagrams: list[DiagramToRender],
    kroki_url: str,
    dpi: int,
) -> list[tuple[DiagramToRender, str]]:
    """Render diagrams via Kroki service.

    SVG diagrams are scaled based on DPI after rendering.

    Args:
        diagrams: Diagrams to render
        kroki_url: Kroki server URL
        dpi: DPI for scaling SVG dimensions

    Returns:
        List of (diagram, rendered_content) tuples
    """
    results: list[tuple[DiagramToRender, str]] = []
    server_url = kroki_url.rstrip("/")

    for diagram in diagrams:
        try:
            if diagram.format == "svg":
                content = _render_svg(diagram, server_url)
                content = scale_svg_dimensions(content, dpi)
            else:
                content = _render_png_data_uri(diagram, server_url)
            results.append((diagram, content))
        except (
            urllib.error.URLError,
            urllib.error.HTTPError,
            OSError,
            TimeoutError,
        ) as e:
            error_content = (
                f'<pre class="diagram-error">Diagram rendering failed: {e}</pre>'
            )
            results.append((diagram, error_content))

    return results


def _render_svg(diagram: DiagramToRender, server_url: str) -> str:
    """Render diagram as SVG.

    Args:
        diagram: Diagram to render
        server_url: Kroki server URL

    Returns:
        SVG content with Google Fonts imports stripped
    """
    encoded = _encode_source(diagram.source)
    url = f"{server_url}/{diagram.endpoint}/svg/{encoded}"

    # S310: URL is constructed from configured Kroki server (http/https only)
    with urllib.request.urlopen(url, timeout=30) as response:  # noqa: S310
        svg = response.read().decode("utf-8")

    return _strip_google_fonts(svg)


def _render_png_data_uri(diagram: DiagramToRender, server_url: str) -> str:
    """Render diagram as PNG data URI.

    Args:
        diagram: Diagram to render
        server_url: Kroki server URL

    Returns:
        Base64 data URI string
    """
    encoded = _encode_source(diagram.source)
    url = f"{server_url}/{diagram.endpoint}/png/{encoded}"

    # S310: URL is constructed from configured Kroki server (http/https only)
    with urllib.request.urlopen(url, timeout=30) as response:  # noqa: S310
        png_data = response.read()

    b64 = base64.b64encode(png_data).decode("ascii")
    return f"data:image/png;base64,{b64}"


def _encode_source(source: str) -> str:
    """Encode diagram source for Kroki URL.

    Uses deflate compression + base64 URL-safe encoding.

    Args:
        source: Diagram source code

    Returns:
        Encoded string for URL
    """
    compressed = zlib.compress(source.encode("utf-8"), level=9)
    encoded = base64.urlsafe_b64encode(compressed).decode("ascii")
    return encoded


def _strip_google_fonts(svg: str) -> str:
    """Strip Google Fonts @import from SVG.

    PlantUML embeds @import for Google Fonts when using Roboto.
    We remove this since Roboto is bundled locally.

    Args:
        svg: SVG content

    Returns:
        SVG with Google Fonts import removed
    """
    return GOOGLE_FONTS_RE.sub("", svg)


def replace_diagram_placeholders(html: str, diagrams: list[RenderedDiagram]) -> str:
    """Replace diagram placeholders with rendered content.

    Args:
        html: HTML with {{DIAGRAM_N}} placeholders
        diagrams: Rendered diagrams

    Returns:
        HTML with diagrams inserted
    """
    for diagram in diagrams:
        placeholder = f"{{{{DIAGRAM_{diagram.index}}}}}"
        content = (
            diagram.content
            if diagram.format == "svg"
            else f'<img src="{diagram.content}" alt="diagram">'
        )
        html = html.replace(placeholder, f'<figure class="diagram">{content}</figure>')

    return html


def scale_svg_dimensions(svg: str, dpi: int) -> str:
    """Scale SVG width and height based on DPI.

    Diagrams are rendered at a configured DPI (e.g., 192 for retina displays).
    This function scales the SVG dimensions down so that the diagram displays
    at its intended physical size.

    Scales both XML attributes (width="136") and inline style properties (width:136px).

    Args:
        svg: SVG content
        dpi: DPI used for rendering

    Returns:
        SVG with scaled dimensions
    """
    if dpi == STANDARD_DPI:
        return svg

    scale = STANDARD_DPI / dpi

    def scale_dim(match: re.Match[str]) -> int:
        """Scale the dimension value from group 2."""
        return round(int(match.group(2)) * scale)

    # Scale XML attributes (width="136", height="210")
    result = SVG_WIDTH_RE.sub(lambda m: f'{m.group(1)}width="{scale_dim(m)}"', svg)
    result = SVG_HEIGHT_RE.sub(lambda m: f'{m.group(1)}height="{scale_dim(m)}"', result)

    # Scale inline style properties (width:136px, height:210px)
    result = STYLE_WIDTH_RE.sub(
        lambda m: f"{m.group(1)}{scale_dim(m)}{m.group(3)}", result
    )
    result = STYLE_HEIGHT_RE.sub(
        lambda m: f"{m.group(1)}{scale_dim(m)}{m.group(3)}", result
    )

    return result
