//! Post-processing for SVG that came back from Kroki: DPI scaling, sizing a
//! root that has only a `viewBox`, and stripping Google Fonts imports and
//! dark-mode styles.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::consts::STANDARD_DPI;

static GOOGLE_FONTS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@import\s+url\([^)]*fonts\.googleapis\.com[^)]*\)\s*;?").unwrap()
});

/// The first `<svg>` start tag — the root. The dimension regexes run inside it
/// only, so a nested `<svg>` keeps its size even when the root declares none.
static SVG_START_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<svg(?:[\s/][^>]*)?>").unwrap());

/// A `width`/`height` attribute with a pixel value (integer or decimal).
static DIMENSION_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(\s(?:width|height)=")(\d+(?:\.\d+)?)(?:px)?""#).unwrap());

/// A `prefers-color-scheme: dark` block holding plain rules, as `GoAT` emits.
static DARK_SCHEME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"@media\s*\(\s*prefers-color-scheme\s*:\s*dark\s*\)\s*\{(?:[^{}]*\{[^{}]*\})*[^{}]*\}",
    )
    .unwrap()
});

/// Any `width`/`height` attribute; `stroke-width` must not count.
static ANY_DIMENSION_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s(?:width|height)\s*=").unwrap());

static VIEW_BOX_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\sviewBox\s*=\s*["']([^"']*)["']"#).unwrap());

static STYLE_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(\sstyle=")([^"]*)""#).unwrap());

/// Regex to match `width`/`height` declarations in a style value (e.g., `width:136px`).
///
/// Anchored to a declaration start so `max-width` and `stroke-width` stay put.
static STYLE_DIMENSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"((?:^|;)\s*(?:width|height)\s*:\s*)(\d+(?:\.\d+)?)px").unwrap());

/// Scale the root `<svg>`'s width and height rendered at `dpi` down to display
/// size.
///
/// A diagram rendered at 192 DPI has its dimensions halved to display at its
/// intended size; at 96 DPI they are unchanged.
///
/// Scales both XML attributes (`width="136"`) and inline style properties
/// (`width:136px`). Decimals (`PlantUML` emits `height="98.2656"`) are scaled
/// before rounding.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn scale_svg_dimensions(svg: &str, dpi: u32) -> String {
    if dpi == STANDARD_DPI {
        return svg.to_owned();
    }
    let Some(root) = SVG_START_TAG_RE.find(svg) else {
        return svg.to_owned();
    };

    let scale_dim = |caps: &Captures| {
        let value: f64 = caps[2].parse().unwrap_or(0.0);
        (value * f64::from(STANDARD_DPI) / f64::from(dpi)).round() as u32
    };

    let tag = DIMENSION_ATTR_RE.replace_all(root.as_str(), |caps: &Captures| {
        format!(r#"{}{}""#, &caps[1], scale_dim(caps))
    });
    let tag = STYLE_ATTR_RE.replace(&tag, |caps: &Captures| {
        let style = STYLE_DIMENSION_RE.replace_all(&caps[2], |caps: &Captures| {
            format!("{}{}px", &caps[1], scale_dim(caps))
        });
        format!(r#"{}{style}""#, &caps[1])
    });

    format!("{}{tag}{}", &svg[..root.start()], &svg[root.end()..])
}

/// Give a root `<svg>` that declares only a `viewBox` the matching `width` and
/// `height`.
///
/// Without them the viewer's shrink-to-fit layout collapses the diagram to
/// nothing; D2 and Pikchr emit such roots.
pub(crate) fn size_from_view_box(svg: &str) -> String {
    let Some(root) = SVG_START_TAG_RE.find(svg) else {
        return svg.to_owned();
    };
    let tag = root.as_str();
    if ANY_DIMENSION_ATTR_RE.is_match(tag) {
        return svg.to_owned();
    }
    let Some(view_box) = VIEW_BOX_ATTR_RE.captures(tag) else {
        return svg.to_owned();
    };
    let parts: Vec<&str> = view_box[1]
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .collect();
    let [_, _, width, height] = parts[..] else {
        return svg.to_owned();
    };
    let positive = |value: &str| value.parse::<f64>().is_ok_and(|n| n > 0.0);
    if !positive(width) || !positive(height) {
        return svg.to_owned();
    }
    let insert_at = root.start() + "<svg".len();
    format!(
        r#"{} width="{width}" height="{height}"{}"#,
        &svg[..insert_at],
        &svg[insert_at..],
    )
}

/// Strip a diagram's own dark-mode styles.
///
/// The viewer darkens a diagram by inverting it, so a diagram that also
/// recolors itself for a dark system theme inverts twice, and one that
/// recolors under a light page theme vanishes into the background.
pub(crate) fn strip_dark_scheme_styles(svg: &str) -> String {
    DARK_SCHEME_RE.replace_all(svg, "").to_string()
}

/// Strip Google Fonts @import from SVG to avoid external requests.
///
/// `PlantUML` embeds `@import url('https://fonts.googleapis.com/...')` in SVG
/// when using Roboto font. We remove this since Roboto is bundled locally.
pub(crate) fn strip_google_fonts_import(svg: &str) -> String {
    GOOGLE_FONTS_RE.replace_all(svg, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_svg_dimensions_at_96_dpi() {
        // At 96 DPI (standard), dimensions should be unchanged
        let svg = r#"<svg width="400" height="200"></svg>"#;
        let result = scale_svg_dimensions(svg, 96);
        assert_eq!(result, r#"<svg width="400" height="200"></svg>"#);
    }

    #[test]
    fn test_scale_svg_dimensions_with_px_suffix() {
        // Handle width/height with "px" suffix
        let svg = r#"<svg width="400px" height="200px"></svg>"#;
        let result = scale_svg_dimensions(svg, 192);
        assert_eq!(result, r#"<svg width="200" height="100"></svg>"#);
    }

    #[test]
    fn test_scale_svg_dimensions_preserves_other_attributes() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200" class="diagram"></svg>"#;
        let result = scale_svg_dimensions(svg, 192);
        assert_eq!(
            result,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" class="diagram"></svg>"#
        );
    }

    #[test]
    fn test_scale_svg_dimensions_with_style_attribute() {
        // Handle width/height in style attribute (as Kroki returns)
        let svg = r#"<svg width="136" height="210" style="width:136px;height:210px;background:#FFFFFF;"></svg>"#;
        let result = scale_svg_dimensions(svg, 192);
        assert_eq!(
            result,
            r#"<svg width="68" height="105" style="width:68px;height:105px;background:#FFFFFF;"></svg>"#
        );
    }

    #[test]
    fn scale_svg_dimensions_scales_only_the_root_style_size() {
        let svg = concat!(
            r#"<svg width="136" height="210" style="max-width:300px;width:136px;height:210px;">"#,
            r#"<path style="stroke-width:2px;max-height:40px;"/>"#,
            "<text>width: 300px</text></svg>",
        );
        let result = scale_svg_dimensions(svg, 192);
        assert_eq!(
            result,
            concat!(
                r#"<svg width="68" height="105" style="max-width:300px;width:68px;height:105px;">"#,
                r#"<path style="stroke-width:2px;max-height:40px;"/>"#,
                "<text>width: 300px</text></svg>",
            )
        );
    }

    #[test]
    fn test_scale_svg_dimensions_with_decimal_values() {
        // PlantUML emits decimal dimensions too (`height="98.2656"`).
        let svg = r#"<svg width="1610.5" height="633" viewBox="-50 -10 1610.5 633"></svg>"#;
        let result = scale_svg_dimensions(svg, 192);
        assert_eq!(
            result,
            r#"<svg width="805" height="317" viewBox="-50 -10 1610.5 633"></svg>"#
        );
    }

    #[test]
    fn scale_svg_dimensions_leaves_an_svg_without_dimensions_untouched() {
        let svg = r#"<svg viewBox="0 0 400 200"><rect width="10" height="10"/></svg>"#;
        let result = scale_svg_dimensions(svg, 192);
        assert_eq!(
            result, svg,
            "an SVG with no <svg> width/height must be untouched"
        );
    }

    #[test]
    fn scale_svg_dimensions_leaves_a_nested_svg_alone_when_the_root_lacks_dimensions() {
        let svg = concat!(
            r#"<svg viewBox="0 0 400 200">"#,
            r#"<svg width="40" height="20" style="width:40px;height:20px;"/>"#,
            "</svg>",
        );
        assert_eq!(scale_svg_dimensions(svg, 192), svg);
    }

    #[test]
    fn scale_svg_dimensions_needs_an_svg_tag() {
        let not_svg = r#"<svgx width="400"/>"#;
        assert_eq!(scale_svg_dimensions(not_svg, 192), not_svg);
    }

    #[test]
    fn scale_svg_dimensions_scales_a_root_without_touching_a_nested_svg() {
        let svg = concat!(
            "<?xml version=\"1.0\"?>\n",
            "<svg\n  width=\"400\"\n  height=\"200\"\n  style=\"width:400px;\">",
            r#"<svg width="40" height="20" style="width:40px;"/>"#,
            "</svg>",
        );
        assert_eq!(
            scale_svg_dimensions(svg, 192),
            concat!(
                "<?xml version=\"1.0\"?>\n",
                "<svg\n  width=\"200\"\n  height=\"100\"\n  style=\"width:200px;\">",
                r#"<svg width="40" height="20" style="width:40px;"/>"#,
                "</svg>",
            )
        );
    }

    #[test]
    fn test_strip_google_fonts_import() {
        let svg_with_import =
            r"<style>@import url('https://fonts.googleapis.com/css?family=Roboto');</style>";
        let result = strip_google_fonts_import(svg_with_import);
        assert_eq!(result, "<style></style>");
    }

    #[test]
    fn test_strip_google_fonts_import_no_change() {
        let svg_without_import = r"<style>.diagram { fill: blue; }</style>";
        let result = strip_google_fonts_import(svg_without_import);
        assert_eq!(result, svg_without_import);
    }

    #[test]
    fn size_from_view_box_sizes_a_root_with_only_a_view_box() {
        let svg = concat!(
            r#"<svg xmlns='http://www.w3.org/2000/svg' class="pikchr" viewBox="0 0 292.32 76.32">"#,
            r#"<svg width="10" height="10"/></svg>"#,
        );
        assert_eq!(
            size_from_view_box(svg),
            concat!(
                r#"<svg width="292.32" height="76.32" xmlns='http://www.w3.org/2000/svg' class="pikchr" viewBox="0 0 292.32 76.32">"#,
                r#"<svg width="10" height="10"/></svg>"#,
            ),
        );
    }

    #[test]
    fn size_from_view_box_reads_a_comma_separated_view_box() {
        assert_eq!(
            size_from_view_box(r"<svg viewBox='-5,-5, 40,20'></svg>"),
            r#"<svg width="40" height="20" viewBox='-5,-5, 40,20'></svg>"#,
        );
    }

    #[test]
    fn size_from_view_box_leaves_a_declared_size_alone() {
        for svg in [
            r#"<svg width="100%" viewBox="0 0 40 20" style="max-width:40px"></svg>"#,
            r#"<svg height="20" viewBox="0 0 40 20"></svg>"#,
        ] {
            assert_eq!(size_from_view_box(svg), svg);
        }
    }

    #[test]
    fn size_from_view_box_ignores_stroke_width() {
        assert_eq!(
            size_from_view_box(r#"<svg stroke-width="1" viewBox="0 0 40 20"></svg>"#),
            r#"<svg width="40" height="20" stroke-width="1" viewBox="0 0 40 20"></svg>"#,
        );
    }

    #[test]
    fn size_from_view_box_needs_a_usable_view_box() {
        for svg in [
            "<svg></svg>",
            r#"<svg viewBox="0 0 40"></svg>"#,
            r#"<svg viewBox="0 0 0 20"></svg>"#,
            r#"<svg viewBox="0 0 wide 20"></svg>"#,
            r#"<g viewBox="0 0 40 20"></g>"#,
        ] {
            assert_eq!(size_from_view_box(svg), svg);
        }
    }

    #[test]
    fn strip_dark_scheme_styles_removes_the_whole_block() {
        let svg = concat!(
            "<style>svg { color: #000; }\n",
            "@media (prefers-color-scheme: dark) {\n",
            "  svg { color: #FFF; /* currentColor */ }\n",
            "  text { fill: #FFF; }\n",
            "}\n",
            "svg { fill: currentColor; }</style>",
        );
        assert_eq!(
            strip_dark_scheme_styles(svg),
            "<style>svg { color: #000; }\n\nsvg { fill: currentColor; }</style>",
        );
    }

    #[test]
    fn strip_dark_scheme_styles_keeps_other_media_queries() {
        let svg = "<style>@media (prefers-color-scheme: light) { svg { color: #000; } }</style>";
        assert_eq!(strip_dark_scheme_styles(svg), svg);
    }
}
