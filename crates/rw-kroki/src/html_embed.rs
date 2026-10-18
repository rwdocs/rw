//! Post-processing for SVG that came back from Kroki:
//! - SVG dimension scaling based on DPI
//! - Google Fonts stripping from SVG

use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::consts::STANDARD_DPI;
use crate::scale::to_display_f64;

static GOOGLE_FONTS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@import\s+url\([^)]*fonts\.googleapis\.com[^)]*\)\s*;?").unwrap()
});

/// Regex to match SVG width attribute with pixel value (integer or decimal).
static SVG_WIDTH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<svg[^>]*\s)width="(\d+(?:\.\d+)?)(?:px)?""#).unwrap());

/// Regex to match SVG height attribute with pixel value (integer or decimal).
static SVG_HEIGHT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<svg[^>]*\s)height="(\d+(?:\.\d+)?)(?:px)?""#).unwrap());

/// Regex to match the style attribute of the `<svg>` tag.
static SVG_STYLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<svg[^>]*\sstyle=")([^"]*)""#).unwrap());

/// Regex to match `width`/`height` declarations in a style value (e.g., `width:136px`).
///
/// Anchored to a declaration start so `max-width` and `stroke-width` stay put.
static STYLE_DIMENSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"((?:^|;)\s*(?:width|height)\s*:\s*)(\d+(?:\.\d+)?)px").unwrap());

/// Scale SVG width and height rendered at `dpi` down to display size.
///
/// A diagram rendered at 192 DPI has its dimensions halved to display at its
/// intended size; at 96 DPI they are unchanged.
///
/// Scales both XML attributes (`width="136"`) and inline style properties (`width:136px`).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn scale_svg_dimensions(svg: &str, dpi: u32) -> String {
    if dpi == STANDARD_DPI {
        return svg.to_owned();
    }

    let scale_dim = |caps: &Captures| {
        let value: f64 = caps[2].parse().unwrap_or(0.0);
        to_display_f64(value, dpi).round() as u32
    };

    // Scale XML attributes (width="136", height="210")
    let result = SVG_WIDTH_RE.replace(svg, |caps: &Captures| {
        format!(r#"{}width="{}""#, &caps[1], scale_dim(caps))
    });
    let result = SVG_HEIGHT_RE.replace(&result, |caps: &Captures| {
        format!(r#"{}height="{}""#, &caps[1], scale_dim(caps))
    });

    // Scale inline style properties (width:136px, height:210px)
    let result = SVG_STYLE_RE.replace(&result, |caps: &Captures| {
        let style = STYLE_DIMENSION_RE.replace_all(&caps[2], |caps: &Captures| {
            format!("{}{}px", &caps[1], scale_dim(caps))
        });
        format!(r#"{}{style}""#, &caps[1])
    });

    result.into_owned()
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
}
