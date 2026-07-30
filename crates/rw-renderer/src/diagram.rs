//! What a backend is handed to render one diagram.

use std::iter::from_fn;

use rw_diagrams::{Asset, Size};
use rw_sections::Sections;

use crate::link::{resolve_section_href, write_section_attrs};
use crate::util::escape_into;

/// One resolved diagram, ready for a backend to turn into markup.
pub struct DiagramView<'a> {
    /// The id for the figure, when this backend emits ids at all (see
    /// [`RenderBackend::DIAGRAM_IDS`](crate::RenderBackend::DIAGRAM_IDS)).
    /// Either the writer's `{#id}` or `diagram-<n>` by position among the
    /// page's diagrams.
    pub id: Option<&'a str>,
    /// The rendered bytes, or the name a caller wrote them under.
    pub asset: &'a Asset,
    /// Display size in CSS pixels, oversampling already divided out. `None`
    /// when the content sizes itself (an SVG carries its own width/height) or
    /// the provider could not determine one.
    pub size: Option<Size>,
    /// Links inside an inline SVG that resolved to a site section, in document
    /// order. Empty for every other asset shape.
    pub links: &'a [DiagramLink],
}

/// One `<a href>` inside a diagram that resolved to a site section.
///
/// Lives here rather than in `rw-diagrams` because no provider produces or
/// consumes one: the renderer resolves them against [`Sections`], exactly as it
/// already does for prose links, and the backend decides what markup they
/// become.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramLink {
    /// The href as it appears in the SVG, identifying which `<a>` to annotate.
    pub href: String,
    /// Canonical `kind:namespace/name`. A `String` rather than a typed
    /// `Section` because this is both what the markup carries and what
    /// [`RenderResult::section_refs`](crate::RenderResult::section_refs)
    /// collects — a typed field would make every backend re-derive the same
    /// string.
    pub section_ref: String,
    /// Path within that section; empty for the section root.
    pub section_path: String,
}

/// Extract the `href` attribute value from an SVG tag string.
///
/// The space prefix avoids matching attribute name suffixes (`xlink:href=`).
fn extract_href(tag: &str) -> Option<&str> {
    const NEEDLE: &str = " href=\"";
    let start = tag.find(NEEDLE)? + NEEDLE.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// Walk `<a …>` tags, yielding the byte offset of each tag's closing `>` and
/// its `href` attribute (`None` when the tag carries none).
///
/// Scanning and splicing must agree on exactly which tags exist, so both
/// [`scan_svg_links`] and [`splice_link_attrs`] walk through here. A tag with
/// no closing `>` ends the walk, leaving the rest of the string untouched.
fn a_tags(svg: &str) -> impl Iterator<Item = (usize, Option<&str>)> {
    let mut pos = 0;
    from_fn(move || {
        let tag_start = svg[pos..].find("<a ")? + pos;
        let close = svg[tag_start..].find('>')? + tag_start;
        let href = extract_href(&svg[tag_start..=close]);
        pos = close + 1;
        Some((close, href))
    })
}

/// Find the `<a>` links in an SVG that point at a section of this site, in
/// document order.
///
/// Only site-absolute hrefs (`/…`) that [`Sections::find`] resolves are
/// reported; everything else — external URLs, fragments, unknown paths — is
/// left alone.
pub(crate) fn scan_svg_links(svg: &str, sections: &Sections) -> Vec<DiagramLink> {
    if sections.is_empty() || !svg.contains("<a ") {
        return Vec::new();
    }

    let mut links = Vec::new();
    for (_, href) in a_tags(svg) {
        let Some(href) = href else { continue };
        let Some((section_ref, section_path)) = resolve_section_href(sections, href) else {
            continue;
        };
        links.push(DiagramLink {
            href: href.to_owned(),
            section_ref,
            section_path,
        });
    }
    links
}

/// Append `svg` to `out` with `data-section-ref` (and `data-section-path`, when
/// non-empty) written onto every `<a>` tag [`scan_svg_links`] resolved.
///
/// Appends the input verbatim when nothing matched, rather than a rebuilt copy
/// of it. Writes into the caller's buffer — the diagram markup it is part of —
/// so an annotated SVG is copied once instead of into an owned `String` and then
/// again out of it.
pub(crate) fn splice_link_attrs(svg: &str, links: &[DiagramLink], out: &mut String) {
    if links.is_empty() {
        out.push_str(svg);
        return;
    }

    out.reserve(svg.len());
    let start = out.len();
    let mut copied = 0;
    let mut changed = false;

    for (close, href) in a_tags(svg) {
        let Some(href) = href else { continue };
        let Some(link) = links.iter().find(|l| l.href == href) else {
            continue;
        };

        changed = true;
        // Stop short of the `>`; the next copied run starts with it.
        out.push_str(&svg[copied..close]);
        copied = close;

        write_section_attrs(&link.section_ref, &link.section_path, out);
    }

    if changed {
        out.push_str(&svg[copied..]);
    } else {
        // Every iteration either skipped before writing or set `changed`, so
        // there is no half-spliced prefix to undo here.
        debug_assert_eq!(out.len(), start, "an unmatched splice wrote to the buffer");
        out.push_str(svg);
    }
}

/// Append the optional `data-diagram-id` attribute (leading space included), or
/// nothing when there is no id. The value is HTML-attribute-escaped.
pub(crate) fn write_diagram_id_attr(id: Option<&str>, out: &mut String) {
    let Some(id) = id else { return };
    out.push_str(r#" data-diagram-id=""#);
    escape_into(id, out);
    out.push('"');
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rw_sections::{Namespace, Section, Sections};

    use super::{DiagramLink, scan_svg_links, splice_link_attrs, write_diagram_id_attr};

    /// Owned-return wrapper over [`splice_link_attrs`], for readable assertions.
    fn spliced(svg: &str, links: &[DiagramLink]) -> String {
        let mut out = String::new();
        splice_link_attrs(svg, links, &mut out);
        out
    }

    /// Owned-return wrapper over [`write_diagram_id_attr`].
    fn diagram_id_attr(id: Option<&str>) -> String {
        let mut out = String::new();
        write_diagram_id_attr(id, &mut out);
        out
    }

    fn billing_sections() -> Sections {
        Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )]))
    }

    /// Scan then splice, the way a caller does across the two phases.
    fn annotate(svg: &str, sections: &Sections) -> String {
        spliced(svg, &scan_svg_links(svg, sections))
    }

    #[test]
    fn annotates_a_cross_section_link() {
        let sections = billing_sections();
        let svg = r#"<svg><a href="/domains/billing/systems/pay" target="_top" xlink:href="/domains/billing/systems/pay"><text>Pay</text></a></svg>"#;
        assert_eq!(
            annotate(svg, &sections),
            r#"<svg><a href="/domains/billing/systems/pay" target="_top" xlink:href="/domains/billing/systems/pay" data-section-ref="domain:default/billing" data-section-path="systems/pay"><text>Pay</text></a></svg>"#
        );
    }

    #[test]
    fn scan_collects_one_link_per_resolving_tag() {
        let sections = billing_sections();
        // Two links to the same section, plus an external one that resolves to
        // nothing: the scan reports the two, in document order.
        let svg = r#"<svg><a href="/domains/billing/api">x</a><a href="/domains/billing/other">y</a><a href="https://ext.example">z</a></svg>"#;
        assert_eq!(
            scan_svg_links(svg, &sections),
            [
                DiagramLink {
                    href: "/domains/billing/api".to_owned(),
                    section_ref: "domain:default/billing".to_owned(),
                    section_path: "api".to_owned(),
                },
                DiagramLink {
                    href: "/domains/billing/other".to_owned(),
                    section_ref: "domain:default/billing".to_owned(),
                    section_path: "other".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn an_exact_section_root_gets_no_section_path() {
        let sections = billing_sections();
        let svg = r#"<svg><a href="/domains/billing" xlink:href="/domains/billing"><text>Billing</text></a></svg>"#;
        assert_eq!(
            annotate(svg, &sections),
            r#"<svg><a href="/domains/billing" xlink:href="/domains/billing" data-section-ref="domain:default/billing"><text>Billing</text></a></svg>"#
        );
    }

    #[test]
    fn a_path_outside_every_section_is_left_alone() {
        let sections = billing_sections();
        let svg =
            r#"<svg><a href="/other/path" xlink:href="/other/path"><text>Other</text></a></svg>"#;
        assert_eq!(scan_svg_links(svg, &sections), []);
        assert_eq!(annotate(svg, &sections), svg);
    }

    /// The leading-slash filter is what makes a link site-absolute, and it is
    /// load-bearing on its own: [`Sections::find`] strips a leading slash before
    /// matching, so it resolves the relative spelling just as happily. Only the
    /// filter tells the two apart.
    #[test]
    fn a_relative_href_is_left_alone_though_the_same_path_would_resolve() {
        let sections = billing_sections();
        let svg = r#"<svg><a href="domains/billing/api">x</a></svg>"#;
        assert_eq!(scan_svg_links(svg, &sections), []);
        assert_eq!(annotate(svg, &sections), svg);

        let absolute = r#"<svg><a href="/domains/billing/api">x</a></svg>"#;
        assert_eq!(
            scan_svg_links(absolute, &sections).len(),
            1,
            "the same path with a leading slash must still resolve, or this \
             test would pass for the wrong reason",
        );
    }

    #[test]
    fn an_external_link_is_left_alone() {
        let sections = billing_sections();
        let svg = r#"<svg><a href="https://example.com" xlink:href="https://example.com"><text>Ext</text></a></svg>"#;
        assert_eq!(scan_svg_links(svg, &sections), []);
        assert_eq!(annotate(svg, &sections), svg);
    }

    #[test]
    fn an_svg_with_no_a_tags_is_left_alone() {
        let sections = billing_sections();
        let svg = r#"<svg><rect width="100" height="50"/></svg>"#;
        assert_eq!(scan_svg_links(svg, &sections), []);
        assert_eq!(annotate(svg, &sections), svg);
    }

    #[test]
    fn empty_sections_resolve_nothing() {
        let sections = Sections::default();
        let svg = r#"<svg><a href="/domains/billing" xlink:href="/domains/billing"><text>B</text></a></svg>"#;
        assert_eq!(scan_svg_links(svg, &sections), []);
        assert_eq!(annotate(svg, &sections), svg);
    }

    #[test]
    fn splicing_escapes_the_attribute_values() {
        let svg = r#"<svg><a href="/x">x</a></svg>"#;
        let links = [DiagramLink {
            href: "/x".to_owned(),
            section_ref: r#"domain:default/a"b"#.to_owned(),
            section_path: "p<q".to_owned(),
        }];
        assert_eq!(
            spliced(svg, &links),
            r#"<svg><a href="/x" data-section-ref="domain:default/a&quot;b" data-section-path="p&lt;q">x</a></svg>"#
        );
    }

    #[test]
    fn splicing_no_links_returns_the_input() {
        let svg = r#"<svg><a href="/x">x</a></svg>"#;
        assert_eq!(spliced(svg, &[]), svg);
    }

    /// Both functions append: the diagram markup around them is already in the
    /// caller's buffer, so writing from position zero would overwrite it.
    #[test]
    fn both_writers_append_to_what_the_caller_already_holds() {
        let mut out = String::from("<figure");
        write_diagram_id_attr(Some("flow-1"), &mut out);
        out.push('>');
        splice_link_attrs(
            r#"<svg><a href="/x">x</a></svg>"#,
            &[DiagramLink {
                href: "/x".to_owned(),
                section_ref: "domain:default/billing".to_owned(),
                section_path: String::new(),
            }],
            &mut out,
        );
        out.push_str("</figure>");

        assert_eq!(
            out,
            r#"<figure data-diagram-id="flow-1"><svg><a href="/x" data-section-ref="domain:default/billing">x</a></svg></figure>"#
        );
    }

    /// An SVG whose `<a>` tags all fail to match must not leave a half-spliced
    /// prefix behind in the caller's buffer.
    #[test]
    fn a_splice_that_matches_nothing_leaves_the_prefix_alone() {
        let mut out = String::from("before:");
        splice_link_attrs(
            r#"<svg><a href="/unmatched">x</a></svg>"#,
            &[DiagramLink {
                href: "/other".to_owned(),
                section_ref: "domain:default/billing".to_owned(),
                section_path: String::new(),
            }],
            &mut out,
        );
        assert_eq!(out, r#"before:<svg><a href="/unmatched">x</a></svg>"#);
    }

    #[test]
    fn diagram_id_attr_has_a_leading_space() {
        assert_eq!(
            diagram_id_attr(Some("flow-1")),
            r#" data-diagram-id="flow-1""#
        );
    }

    #[test]
    fn diagram_id_attr_escapes_the_id() {
        assert_eq!(
            diagram_id_attr(Some(r#"a"b<c"#)),
            r#" data-diagram-id="a&quot;b&lt;c""#
        );
    }

    #[test]
    fn diagram_id_attr_is_empty_without_an_id() {
        assert_eq!(diagram_id_attr(None), "");
    }
}
