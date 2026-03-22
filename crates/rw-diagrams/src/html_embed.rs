//! HTML embedding for rendered diagrams.
//!
//! This module handles embedding rendered diagrams into HTML:
//! - SVG dimension scaling based on DPI
//! - Google Fonts stripping from SVG
//! - SVG link annotation with section ref data attributes

use std::fmt::Write;
use std::sync::LazyLock;

use regex::Regex;
use rw_renderer::escape_html;
use rw_sections::Sections;

use crate::consts::STANDARD_DPI;

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
pub fn scale_svg_dimensions(svg: &str, dpi: u32) -> String {
    if dpi == STANDARD_DPI {
        return svg.to_owned();
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

/// Extract an attribute value from an SVG tag string.
///
/// Uses a space prefix to avoid matching attribute name suffixes
/// (e.g., `href=` won't match `xlink:href=`).
fn extract_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// Annotate SVG `<a>` elements with `data-section-ref` and `data-section-path`.
///
/// Scans SVG for `<a>` tags, extracts `href`, resolves against sections,
/// and injects data attributes before the closing `>`.
///
/// Returns the SVG unmodified if no annotations are needed.
#[must_use]
pub fn annotate_svg_links(svg: &str, sections: &Sections) -> String {
    if sections.is_empty() || !svg.contains("<a ") {
        return svg.to_owned();
    }

    let mut result = String::with_capacity(svg.len());
    let mut remaining = svg;
    let mut changed = false;

    while let Some(tag_start) = remaining.find("<a ") {
        let after_tag = &remaining[tag_start..];
        let Some(tag_end) = after_tag.find('>') else {
            break;
        };
        let tag = &after_tag[..=tag_end];

        let Some(href) = extract_attr(tag, "href") else {
            result.push_str(&remaining[..=tag_start + tag_end]);
            remaining = &remaining[tag_start + tag_end + 1..];
            continue;
        };

        let Some((ref_string, section_path)) = sections.resolve_ref(href) else {
            result.push_str(&remaining[..=tag_start + tag_end]);
            remaining = &remaining[tag_start + tag_end + 1..];
            continue;
        };

        changed = true;
        result.push_str(&remaining[..tag_start + tag_end]);

        write!(
            result,
            r#" data-section-ref="{}""#,
            escape_html(&ref_string)
        )
        .unwrap();
        if !section_path.is_empty() {
            write!(
                result,
                r#" data-section-path="{}""#,
                escape_html(&section_path)
            )
            .unwrap();
        }
        result.push('>');

        remaining = &remaining[tag_start + tag_end + 1..];
    }

    if changed {
        result.push_str(remaining);
        result
    } else {
        svg.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rw_sections::{Section, Sections};

    use super::*;

    fn billing_sections() -> Sections {
        Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                name: "billing".to_owned(),
            },
        )]))
    }

    #[test]
    fn annotate_svg_links_cross_section() {
        let sections = billing_sections();
        let svg = r#"<svg><a href="/domains/billing/systems/pay" target="_top" xlink:href="/domains/billing/systems/pay"><text>Pay</text></a></svg>"#;
        let result = annotate_svg_links(svg, &sections);
        assert!(
            result.contains(r#"data-section-ref="domain:default/billing""#),
            "Should have data-section-ref: {result}"
        );
        assert!(
            result.contains(r#"data-section-path="systems/pay""#),
            "Should have data-section-path: {result}"
        );
    }

    #[test]
    fn annotate_svg_links_exact_section_root() {
        let sections = billing_sections();
        let svg = r#"<svg><a href="/domains/billing" xlink:href="/domains/billing"><text>Billing</text></a></svg>"#;
        let result = annotate_svg_links(svg, &sections);
        assert!(
            result.contains(r#"data-section-ref="domain:default/billing""#),
            "Should have data-section-ref: {result}"
        );
        assert!(
            !result.contains("data-section-path"),
            "Exact root match should omit data-section-path: {result}"
        );
    }

    #[test]
    fn annotate_svg_links_no_match() {
        let sections = billing_sections();
        let svg =
            r#"<svg><a href="/other/path" xlink:href="/other/path"><text>Other</text></a></svg>"#;
        let original = svg.to_owned();
        let result = annotate_svg_links(svg, &sections);
        assert_eq!(result, original, "Non-matching link should not be modified");
    }

    #[test]
    fn annotate_svg_links_external_link() {
        let sections = billing_sections();
        let svg = r#"<svg><a href="https://example.com" xlink:href="https://example.com"><text>Ext</text></a></svg>"#;
        let original = svg.to_owned();
        let result = annotate_svg_links(svg, &sections);
        assert_eq!(result, original, "External link should not be modified");
    }

    #[test]
    fn annotate_svg_links_no_a_tags() {
        let sections = billing_sections();
        let svg = r#"<svg><rect width="100" height="50"/></svg>"#;
        let original = svg.to_owned();
        let result = annotate_svg_links(svg, &sections);
        assert_eq!(result, original, "SVG without links should not be modified");
    }

    #[test]
    fn annotate_svg_links_empty_sections() {
        let sections = Sections::default();
        let svg = r#"<svg><a href="/domains/billing" xlink:href="/domains/billing"><text>B</text></a></svg>"#;
        let original = svg.to_owned();
        let result = annotate_svg_links(svg, &sections);
        assert_eq!(
            result, original,
            "Empty sections should not modify anything"
        );
    }

    #[test]
    fn test_scale_svg_dimensions_at_192_dpi() {
        // At 192 DPI (2x retina), dimensions should be halved
        let svg = r#"<svg width="400" height="200" viewBox="0 0 400 200"></svg>"#;
        let result = scale_svg_dimensions(svg, 192);
        assert_eq!(
            result,
            r#"<svg width="200" height="100" viewBox="0 0 400 200"></svg>"#
        );
    }

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
    fn test_scale_svg_dimensions_at_144_dpi() {
        // At 144 DPI (1.5x), dimensions should be scaled to 2/3
        let svg = r#"<svg width="300" height="150"></svg>"#;
        let result = scale_svg_dimensions(svg, 144);
        // 300 * (96/144) = 200, 150 * (96/144) = 100
        assert_eq!(result, r#"<svg width="200" height="100"></svg>"#);
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
