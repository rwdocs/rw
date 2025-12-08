"""Diagram rendering with caching support.

Renders diagrams via Kroki with content-based caching to avoid redundant requests.
"""

import base64
import re
import urllib.error
import urllib.request
import zlib
from dataclasses import dataclass

from docstage.core.cache import FileCache, compute_diagram_hash

GOOGLE_FONTS_RE = re.compile(r"@import\s+url\([^)]*fonts\.googleapis\.com[^)]*\)\s*;?")


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
    cache: FileCache,
) -> list[RenderedDiagram]:
    """Render diagrams via Kroki with caching.

    Args:
        diagrams: List of (index, source, endpoint, format) tuples
        kroki_url: Kroki server URL
        cache: FileCache for diagram caching

    Returns:
        List of RenderedDiagram with content ready for HTML
    """
    results: list[RenderedDiagram] = []
    to_render: list[DiagramToRender] = []

    for index, source, endpoint, fmt in diagrams:
        content_hash = compute_diagram_hash(source, endpoint, fmt)
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
                )
            )

    if to_render:
        rendered = _render_via_kroki(to_render, kroki_url)
        for diagram, content in rendered:
            cache.set_diagram(diagram.content_hash, diagram.format, content)
            results.append(
                RenderedDiagram(index=diagram.index, content=content, format=diagram.format)
            )

    results.sort(key=lambda r: r.index)
    return results


def _render_via_kroki(
    diagrams: list[DiagramToRender],
    kroki_url: str,
) -> list[tuple[DiagramToRender, str]]:
    """Render diagrams via Kroki service.

    Args:
        diagrams: Diagrams to render
        kroki_url: Kroki server URL

    Returns:
        List of (diagram, rendered_content) tuples
    """
    results: list[tuple[DiagramToRender, str]] = []
    server_url = kroki_url.rstrip("/")

    for diagram in diagrams:
        try:
            if diagram.format == "svg":
                content = _render_svg(diagram, server_url)
            else:
                content = _render_png_data_uri(diagram, server_url)
            results.append((diagram, content))
        except (urllib.error.URLError, urllib.error.HTTPError, OSError, TimeoutError) as e:
            error_content = f'<pre class="diagram-error">Diagram rendering failed: {e}</pre>'
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

    with urllib.request.urlopen(url, timeout=30) as response:
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

    with urllib.request.urlopen(url, timeout=30) as response:
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

        if diagram.format == "svg":
            figure = f'<figure class="diagram">{diagram.content}</figure>'
        else:
            figure = f'<figure class="diagram"><img src="{diagram.content}" alt="diagram"></figure>'

        html = html.replace(placeholder, figure)

    return html
