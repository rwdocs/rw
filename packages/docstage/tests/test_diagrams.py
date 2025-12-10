"""Tests for diagram rendering."""

from pathlib import Path
from unittest.mock import MagicMock, patch

from docstage.core.cache import FileCache
from docstage.core.diagrams import (
    DiagramToRender,
    RenderedDiagram,
    _encode_source,
    _strip_google_fonts,
    render_diagrams_with_cache,
    replace_diagram_placeholders,
    scale_svg_dimensions,
)


class TestScaleSvgDimensions:
    """Tests for scale_svg_dimensions()."""

    def test__at_192_dpi__halves_dimensions(self) -> None:
        """At 192 DPI (2x retina), dimensions should be halved."""
        svg = '<svg width="400" height="200" viewBox="0 0 400 200"></svg>'

        result = scale_svg_dimensions(svg, 192)

        assert result == '<svg width="200" height="100" viewBox="0 0 400 200"></svg>'

    def test__at_96_dpi__unchanged(self) -> None:
        """At 96 DPI (standard), dimensions should be unchanged."""
        svg = '<svg width="400" height="200"></svg>'

        result = scale_svg_dimensions(svg, 96)

        assert result == '<svg width="400" height="200"></svg>'

    def test__with_px_suffix__strips_suffix(self) -> None:
        """Handle width/height with 'px' suffix."""
        svg = '<svg width="400px" height="200px"></svg>'

        result = scale_svg_dimensions(svg, 192)

        assert result == '<svg width="200" height="100"></svg>'

    def test__with_other_attributes__preserves_them(self) -> None:
        """Preserve other SVG attributes when scaling."""
        svg = '<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200" class="diagram"></svg>'

        result = scale_svg_dimensions(svg, 192)

        assert (
            result
            == '<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" class="diagram"></svg>'
        )

    def test__at_144_dpi__scales_to_two_thirds(self) -> None:
        """At 144 DPI (1.5x), dimensions should be scaled to 2/3."""
        svg = '<svg width="300" height="150"></svg>'

        result = scale_svg_dimensions(svg, 144)

        # 300 * (96/144) = 200, 150 * (96/144) = 100
        assert result == '<svg width="200" height="100"></svg>'

    def test__with_style_attribute__scales_both(self) -> None:
        """Scale both XML attributes and inline style properties."""
        svg = '<svg width="136" height="210" style="width:136px;height:210px;background:#FFFFFF;"></svg>'

        result = scale_svg_dimensions(svg, 192)

        assert (
            result
            == '<svg width="68" height="105" style="width:68px;height:105px;background:#FFFFFF;"></svg>'
        )


class TestStripGoogleFonts:
    """Tests for _strip_google_fonts()."""

    def test__with_google_fonts__strips_import(self) -> None:
        """Remove Google Fonts @import from SVG."""
        svg = "<style>@import url(https://fonts.googleapis.com/css?family=Roboto);.text{fill:#000;}</style>"

        result = _strip_google_fonts(svg)

        assert result == "<style>.text{fill:#000;}</style>"

    def test__without_google_fonts__unchanged(self) -> None:
        """Leave SVG unchanged if no Google Fonts import."""
        svg = '<svg width="100"><style>.text{fill:#000;}</style></svg>'

        result = _strip_google_fonts(svg)

        assert result == svg


class TestEncodeSource:
    """Tests for _encode_source()."""

    def test__encodes_source__returns_base64(self) -> None:
        """Encode diagram source using deflate + base64."""
        source = "@startuml\nAlice -> Bob\n@enduml"

        result = _encode_source(source)

        # Should be URL-safe base64
        assert isinstance(result, str)
        assert "/" not in result  # URL-safe
        assert "+" not in result  # URL-safe


class TestReplaceDiagramPlaceholders:
    """Tests for replace_diagram_placeholders()."""

    def test__svg_format__wraps_in_figure(self) -> None:
        """SVG diagrams are wrapped in figure element."""
        html = "<p>Before</p>{{DIAGRAM_0}}<p>After</p>"
        diagrams = [RenderedDiagram(index=0, content="<svg></svg>", format="svg")]

        result = replace_diagram_placeholders(html, diagrams)

        assert (
            result
            == '<p>Before</p><figure class="diagram"><svg></svg></figure><p>After</p>'
        )

    def test__png_format__creates_img_tag(self) -> None:
        """PNG diagrams create img tag with data URI."""
        html = "{{DIAGRAM_0}}"
        diagrams = [
            RenderedDiagram(index=0, content="data:image/png;base64,abc", format="png")
        ]

        result = replace_diagram_placeholders(html, diagrams)

        assert (
            result
            == '<figure class="diagram"><img src="data:image/png;base64,abc" alt="diagram"></figure>'
        )

    def test__multiple_diagrams__replaces_all(self) -> None:
        """Replace multiple diagram placeholders."""
        html = "{{DIAGRAM_0}} text {{DIAGRAM_1}}"
        diagrams = [
            RenderedDiagram(index=0, content="<svg>A</svg>", format="svg"),
            RenderedDiagram(index=1, content="<svg>B</svg>", format="svg"),
        ]

        result = replace_diagram_placeholders(html, diagrams)

        assert '<figure class="diagram"><svg>A</svg></figure>' in result
        assert '<figure class="diagram"><svg>B</svg></figure>' in result


class TestRenderDiagramsWithCache:
    """Tests for render_diagrams_with_cache()."""

    def test__cache_hit__returns_cached(self, tmp_path: Path) -> None:
        """Return cached diagram without calling Kroki."""
        cache = FileCache(tmp_path / ".cache")
        # Pre-populate cache
        from docstage.core.cache import compute_diagram_hash

        source = "@startuml\nA -> B\n@enduml"
        content_hash = compute_diagram_hash(source, "plantuml", "svg", 192)
        cache.set_diagram(content_hash, "svg", "<svg>cached</svg>")

        diagrams = [(0, source, "plantuml", "svg")]

        result = render_diagrams_with_cache(diagrams, "https://kroki.io", cache, 192)

        assert len(result) == 1
        assert result[0].content == "<svg>cached</svg>"
        assert result[0].format == "svg"

    def test__cache_miss__calls_kroki(self, tmp_path: Path) -> None:
        """Render via Kroki and cache result on cache miss."""
        cache = FileCache(tmp_path / ".cache")
        diagrams = [(0, "@startuml\nA\n@enduml", "plantuml", "svg")]

        # Mock the HTTP request
        mock_response = MagicMock()
        mock_response.read.return_value = b"<svg>rendered</svg>"
        mock_response.__enter__ = MagicMock(return_value=mock_response)
        mock_response.__exit__ = MagicMock(return_value=False)

        with patch("urllib.request.urlopen", return_value=mock_response):
            result = render_diagrams_with_cache(diagrams, "https://kroki.io", cache, 96)

        assert len(result) == 1
        assert "<svg>rendered</svg>" in result[0].content

    def test__png_format__returns_data_uri(self, tmp_path: Path) -> None:
        """PNG diagrams are returned as base64 data URIs."""
        cache = FileCache(tmp_path / ".cache")
        diagrams = [(0, "@startuml\nA\n@enduml", "plantuml", "png")]

        # Mock PNG response
        mock_response = MagicMock()
        mock_response.read.return_value = b"\x89PNG\r\n"  # PNG header
        mock_response.__enter__ = MagicMock(return_value=mock_response)
        mock_response.__exit__ = MagicMock(return_value=False)

        with patch("urllib.request.urlopen", return_value=mock_response):
            result = render_diagrams_with_cache(
                diagrams, "https://kroki.io", cache, 192
            )

        assert len(result) == 1
        assert result[0].content.startswith("data:image/png;base64,")
        assert result[0].format == "png"

    def test__kroki_error__returns_error_html(self, tmp_path: Path) -> None:
        """Return error HTML when Kroki request fails."""
        import urllib.error

        cache = FileCache(tmp_path / ".cache")
        diagrams = [(0, "invalid", "plantuml", "svg")]

        with patch(
            "urllib.request.urlopen",
            side_effect=urllib.error.URLError("Connection refused"),
        ):
            result = render_diagrams_with_cache(
                diagrams, "https://kroki.io", cache, 192
            )

        assert len(result) == 1
        assert "diagram-error" in result[0].content
        assert "Connection refused" in result[0].content

    def test__multiple_diagrams__returns_sorted(self, tmp_path: Path) -> None:
        """Return diagrams sorted by index."""
        cache = FileCache(tmp_path / ".cache")

        # Pre-cache diagram 1 but not diagram 0
        from docstage.core.cache import compute_diagram_hash

        source1 = "B"
        hash1 = compute_diagram_hash(source1, "mermaid", "svg", 192)
        cache.set_diagram(hash1, "svg", "<svg>B</svg>")

        diagrams = [
            (0, "A", "mermaid", "svg"),
            (1, source1, "mermaid", "svg"),
        ]

        mock_response = MagicMock()
        mock_response.read.return_value = b"<svg>A</svg>"
        mock_response.__enter__ = MagicMock(return_value=mock_response)
        mock_response.__exit__ = MagicMock(return_value=False)

        with patch("urllib.request.urlopen", return_value=mock_response):
            result = render_diagrams_with_cache(
                diagrams, "https://kroki.io", cache, 192
            )

        assert len(result) == 2
        assert result[0].index == 0
        assert result[1].index == 1


class TestDiagramDataclasses:
    """Tests for diagram dataclasses."""

    def test__diagram_to_render__fields(self) -> None:
        """DiagramToRender has correct fields."""
        diagram = DiagramToRender(
            index=0,
            source="test",
            endpoint="plantuml",
            format="svg",
            content_hash="abc123",
        )

        assert diagram.index == 0
        assert diagram.source == "test"
        assert diagram.endpoint == "plantuml"
        assert diagram.format == "svg"
        assert diagram.content_hash == "abc123"

    def test__rendered_diagram__fields(self) -> None:
        """RenderedDiagram has correct fields."""
        diagram = RenderedDiagram(index=0, content="<svg></svg>", format="svg")

        assert diagram.index == 0
        assert diagram.content == "<svg></svg>"
        assert diagram.format == "svg"
