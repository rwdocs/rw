//! HTML embedding for rendered diagrams.
//!
//! This module handles embedding rendered diagrams into HTML:
//! - SVG dimension scaling based on DPI
//! - Placeholder replacement with rendered content
//! - Error handling for failed renders

use std::sync::LazyLock;

use regex::Regex;

use docstage_renderer::escape_html;

use crate::kroki::{
    DiagramError, DiagramRequest, render_all_png_data_uri_partial, render_all_svg_partial,
};

use crate::consts::{DEFAULT_DPI, STANDARD_DPI};

static GOOGLE_FONTS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@import\s+url\([^)]*fonts\.googleapis\.com[^)]*\)\s*;?").unwrap()
});

/// Regex to match SVG width attribute with pixel value.
static SVG_WIDTH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<svg[^>]*\s)width="(\d+)(?:px)?""#).unwrap());

/// Regex to match SVG height attribute with pixel value.
static SVG_HEIGHT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<svg[^>]*\s)height="(\d+)(?:px)?""#).unwrap());

/// Regex to match width in style attribute (e.g., `width:136px`).
static STYLE_WIDTH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(width:\s*)(\d+)(px)").unwrap());

/// Regex to match height in style attribute (e.g., `height:210px`).
static STYLE_HEIGHT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(height:\s*)(\d+)(px)").unwrap());

/// Scale SVG width and height based on DPI.
///
/// Diagrams are rendered at a configured DPI (e.g., 192 for retina displays).
/// This function scales the SVG dimensions down so that the diagram displays
/// at its intended physical size. For example, a diagram rendered at 192 DPI
/// will have its dimensions halved to display correctly on standard 96 DPI displays.
///
/// Scales both XML attributes (`width="136"`) and inline style properties (`width:136px`).
///
/// The scaling factor is `STANDARD_DPI / dpi`. At 192 DPI, this is 0.5 (halved).
/// At 96 DPI, dimensions are unchanged.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[must_use]
pub fn scale_svg_dimensions(svg: &str, dpi: Option<u32>) -> String {
    let dpi = dpi.unwrap_or(DEFAULT_DPI);
    if dpi == STANDARD_DPI {
        return svg.to_string();
    }

    let scale = f64::from(STANDARD_DPI) / f64::from(dpi);

    // Helper to scale a dimension value and format the result
    let scale_dim = |caps: &regex::Captures| {
        let value: f64 = caps[2].parse().unwrap_or(0.0);
        (value * scale).round() as u32
    };

    // Scale XML attributes (width="136", height="210")
    let result = SVG_WIDTH_RE.replace(svg, |caps: &regex::Captures| {
        format!(r#"{}width="{}""#, &caps[1], scale_dim(caps))
    });
    let result = SVG_HEIGHT_RE.replace(&result, |caps: &regex::Captures| {
        format!(r#"{}height="{}""#, &caps[1], scale_dim(caps))
    });

    // Scale inline style properties (width:136px, height:210px)
    let result = STYLE_WIDTH_RE.replace_all(&result, |caps: &regex::Captures| {
        format!("{}{}{}", &caps[1], scale_dim(caps), &caps[3])
    });
    let result = STYLE_HEIGHT_RE.replace_all(&result, |caps: &regex::Captures| {
        format!("{}{}{}", &caps[1], scale_dim(caps), &caps[3])
    });

    result.into_owned()
}

/// Strip Google Fonts @import from SVG to avoid external requests.
///
/// `PlantUML` embeds `@import url('https://fonts.googleapis.com/...')` in SVG
/// when using Roboto font. We remove this since Roboto is bundled locally.
#[must_use]
pub fn strip_google_fonts_import(svg: &str) -> String {
    GOOGLE_FONTS_RE.replace_all(svg, "").to_string()
}

/// Replace diagram placeholders with rendered SVG content.
///
/// Renders diagrams via Kroki and replaces `{{DIAGRAM_N}}` placeholders.
/// On success, wraps SVG in `<figure class="diagram">`.
/// On failure, shows error in `<figure class="diagram diagram-error">`.
///
/// SVG dimensions are scaled based on DPI to display at correct physical size.
/// For example, at 192 DPI (2x retina), dimensions are halved so diagrams
/// appear at their intended size on standard displays.
pub fn replace_svg_diagrams(
    html: &mut String,
    diagrams: &[(usize, DiagramRequest)],
    kroki_url: &str,
    dpi: Option<u32>,
) {
    if diagrams.is_empty() {
        return;
    }

    let requests: Vec<_> = diagrams.iter().map(|(_, r)| r.clone()).collect();
    match render_all_svg_partial(&requests, kroki_url, 4) {
        Ok(result) => {
            for r in result.rendered {
                replace_placeholder_with_svg(html, r.index, r.svg.trim(), dpi);
            }
            replace_errors(html, &result.errors);
        }
        Err(e) => replace_all_with_error(html, diagrams, &e.to_string()),
    }
}

/// Replace diagram placeholders with rendered PNG content as data URIs.
///
/// Renders diagrams via Kroki and replaces `{{DIAGRAM_N}}` placeholders.
/// On success, wraps PNG in `<figure class="diagram"><img>`.
/// On failure, shows error in `<figure class="diagram diagram-error">`.
pub fn replace_png_diagrams(
    html: &mut String,
    diagrams: &[(usize, DiagramRequest)],
    kroki_url: &str,
) {
    if diagrams.is_empty() {
        return;
    }

    let requests: Vec<_> = diagrams.iter().map(|(_, r)| r.clone()).collect();
    match render_all_png_data_uri_partial(&requests, kroki_url, 4) {
        Ok(result) => {
            for r in result.rendered {
                replace_placeholder_with_png(html, r.index, &r.data_uri);
            }
            replace_errors(html, &result.errors);
        }
        Err(e) => replace_all_with_error(html, diagrams, &e.to_string()),
    }
}

fn replace_placeholder_with_svg(html: &mut String, index: usize, svg: &str, dpi: Option<u32>) {
    let placeholder = format!("{{{{DIAGRAM_{index}}}}}");
    let clean_svg = strip_google_fonts_import(svg);
    let scaled_svg = scale_svg_dimensions(&clean_svg, dpi);
    let figure = format!(r#"<figure class="diagram">{scaled_svg}</figure>"#);
    *html = html.replace(&placeholder, &figure);
}

fn replace_placeholder_with_png(html: &mut String, index: usize, data_uri: &str) {
    let placeholder = format!("{{{{DIAGRAM_{index}}}}}");
    let figure =
        format!(r#"<figure class="diagram"><img src="{data_uri}" alt="diagram"></figure>"#);
    *html = html.replace(&placeholder, &figure);
}

fn replace_errors(html: &mut String, errors: &[DiagramError]) {
    for e in errors {
        replace_placeholder_with_error_msg(html, e.index, &e.to_string());
    }
}

fn replace_all_with_error(
    html: &mut String,
    diagrams: &[(usize, DiagramRequest)],
    error_msg: &str,
) {
    for (idx, _) in diagrams {
        replace_placeholder_with_error_msg(html, *idx, error_msg);
    }
}

fn replace_placeholder_with_error_msg(html: &mut String, index: usize, error_msg: &str) {
    let placeholder = format!("{{{{DIAGRAM_{index}}}}}");
    let error_figure = format!(
        r#"<figure class="diagram diagram-error"><pre>Diagram rendering failed: {}</pre></figure>"#,
        escape_html(error_msg)
    );
    *html = html.replace(&placeholder, &error_figure);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_svg_dimensions_at_192_dpi() {
        // At 192 DPI (2x retina), dimensions should be halved
        let svg = r#"<svg width="400" height="200" viewBox="0 0 400 200"></svg>"#;
        let result = scale_svg_dimensions(svg, Some(192));
        assert_eq!(
            result,
            r#"<svg width="200" height="100" viewBox="0 0 400 200"></svg>"#
        );
    }

    #[test]
    fn test_scale_svg_dimensions_at_96_dpi() {
        // At 96 DPI (standard), dimensions should be unchanged
        let svg = r#"<svg width="400" height="200"></svg>"#;
        let result = scale_svg_dimensions(svg, Some(96));
        assert_eq!(result, r#"<svg width="400" height="200"></svg>"#);
    }

    #[test]
    fn test_scale_svg_dimensions_with_px_suffix() {
        // Handle width/height with "px" suffix
        let svg = r#"<svg width="400px" height="200px"></svg>"#;
        let result = scale_svg_dimensions(svg, Some(192));
        assert_eq!(result, r#"<svg width="200" height="100"></svg>"#);
    }

    #[test]
    fn test_scale_svg_dimensions_preserves_other_attributes() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200" class="diagram"></svg>"#;
        let result = scale_svg_dimensions(svg, Some(192));
        assert_eq!(
            result,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" class="diagram"></svg>"#
        );
    }

    #[test]
    fn test_scale_svg_dimensions_at_144_dpi() {
        // At 144 DPI (1.5x), dimensions should be scaled to 2/3
        let svg = r#"<svg width="300" height="150"></svg>"#;
        let result = scale_svg_dimensions(svg, Some(144));
        // 300 * (96/144) = 200, 150 * (96/144) = 100
        assert_eq!(result, r#"<svg width="200" height="100"></svg>"#);
    }

    #[test]
    fn test_scale_svg_dimensions_with_style_attribute() {
        // Handle width/height in style attribute (as Kroki returns)
        let svg = r#"<svg width="136" height="210" style="width:136px;height:210px;background:#FFFFFF;"></svg>"#;
        let result = scale_svg_dimensions(svg, Some(192));
        assert_eq!(
            result,
            r#"<svg width="68" height="105" style="width:68px;height:105px;background:#FFFFFF;"></svg>"#
        );
    }

    #[test]
    fn test_strip_google_fonts_import() {
        let svg_with_import =
            r#"<style>@import url('https://fonts.googleapis.com/css?family=Roboto');</style>"#;
        let result = strip_google_fonts_import(svg_with_import);
        assert_eq!(result, "<style></style>");
    }

    #[test]
    fn test_strip_google_fonts_import_no_change() {
        let svg_without_import = r#"<style>.diagram { fill: blue; }</style>"#;
        let result = strip_google_fonts_import(svg_without_import);
        assert_eq!(result, svg_without_import);
    }
}
