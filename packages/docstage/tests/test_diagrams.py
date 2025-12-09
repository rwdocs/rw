"""Tests for diagram rendering."""

from docstage.core.diagrams import scale_svg_dimensions


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
