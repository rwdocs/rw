//! HTML backend for markdown rendering.
//!
//! [`HtmlBackend`] produces semantic HTML5 suitable for the RW web viewer.
//! It resolves relative `.md` links to clean URL paths and renders
//! GitHub-style alerts with SVG icons from the [Octicons] set.
//!
//! [Octicons]: https://primer.style/octicons/

use std::fmt::Write;

use crate::status::StatusColor;
use crate::tabs::TabInfo;

use base64::prelude::{BASE64_STANDARD, Engine};
use rw_diagrams::{Asset, DiagramContent, Size};

use crate::backend::RenderBackend;
use crate::diagram::{DiagramView, splice_link_attrs, write_diagram_id_attr};
use crate::util::escape_into;
use rw_parser::AlertKind;

// SVG icons for alerts (GitHub Octicons-style, 16x16)
const SVG_INFO: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M0 8a8 8 0 1 1 16 0A8 8 0 0 1 0 8Zm8-6.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13ZM6.5 7.75A.75.75 0 0 1 7.25 7h1a.75.75 0 0 1 .75.75v2.75h.25a.75.75 0 0 1 0 1.5h-2a.75.75 0 0 1 0-1.5h.25v-2h-.25a.75.75 0 0 1-.75-.75ZM8 6a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"></path></svg>"#;
const SVG_LIGHTBULB: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M8 1.5c-2.363 0-4 1.69-4 3.75 0 .984.424 1.625.984 2.304l.214.253c.223.264.47.556.673.848.284.411.537.896.621 1.49a.75.75 0 0 1-1.484.211c-.04-.282-.163-.547-.37-.847a8.456 8.456 0 0 0-.542-.68c-.084-.1-.173-.205-.268-.32C3.201 7.75 2.5 6.766 2.5 5.25 2.5 2.31 4.863 0 8 0s5.5 2.31 5.5 5.25c0 1.516-.701 2.5-1.328 3.259-.095.115-.184.22-.268.319-.207.245-.383.453-.541.681-.208.3-.33.565-.37.847a.751.751 0 0 1-1.485-.212c.084-.593.337-1.078.621-1.489.203-.292.45-.584.673-.848.075-.088.147-.173.213-.253.561-.679.985-1.32.985-2.304 0-2.06-1.637-3.75-4-3.75ZM5.75 12h4.5a.75.75 0 0 1 0 1.5h-4.5a.75.75 0 0 1 0-1.5ZM6 15.25a.75.75 0 0 1 .75-.75h2.5a.75.75 0 0 1 0 1.5h-2.5a.75.75 0 0 1-.75-.75Z"></path></svg>"#;
const SVG_REPORT: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M0 1.75C0 .784.784 0 1.75 0h12.5C15.216 0 16 .784 16 1.75v9.5A1.75 1.75 0 0 1 14.25 13H8.06l-2.573 2.573A1.458 1.458 0 0 1 3 14.543V13H1.75A1.75 1.75 0 0 1 0 11.25Zm1.75-.25a.25.25 0 0 0-.25.25v9.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h6.5a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25Zm7 2.25v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 9a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"></path></svg>"#;
const SVG_ALERT: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M6.457 1.047c.659-1.234 2.427-1.234 3.086 0l6.082 11.378A1.75 1.75 0 0 1 14.082 15H1.918a1.75 1.75 0 0 1-1.543-2.575Zm1.763.707a.25.25 0 0 0-.44 0L1.698 13.132a.25.25 0 0 0 .22.368h12.164a.25.25 0 0 0 .22-.368Zm.53 3.996v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 11a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"></path></svg>"#;
const SVG_STOP: &str = r#"<svg class="alert-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M4.47.22A.749.749 0 0 1 5 0h6c.199 0 .389.079.53.22l4.25 4.25c.141.14.22.331.22.53v6a.749.749 0 0 1-.22.53l-4.25 4.25A.749.749 0 0 1 11 16H5a.749.749 0 0 1-.53-.22L.22 11.53A.749.749 0 0 1 0 11V5c0-.199.079-.389.22-.53Zm.84 1.28L1.5 5.31v5.38l3.81 3.81h5.38l3.81-3.81V5.31L10.69 1.5ZM8 4a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"></path></svg>"#;

/// [`RenderBackend`] implementation that produces semantic HTML5.
///
/// Code blocks use `<pre><code>` with a `language-*` class for syntax
/// highlighting, and images use `<img>`.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new().render("Some *emphasis*.");
///
/// assert_eq!(result.html, "<p>Some <em>emphasis</em>.</p>");
/// ```
pub struct HtmlBackend;

/// Writes ` width="…" height="…"` for a diagram whose display size is known.
///
/// An unknown size writes nothing: the `<img>` still renders, without an
/// aspect-ratio box.
fn write_size_attrs(size: Option<Size>, out: &mut String) {
    if let Some(Size { width, height }) = size {
        write!(out, r#" width="{width}" height="{height}""#).unwrap();
    }
}

/// Close a diagram `<img>` with its `alt`, generic when the author gave none.
fn write_img_alt(alt: Option<&str>, out: &mut String) {
    out.push_str(r#" alt=""#);
    escape_into(alt.unwrap_or("diagram"), out);
    out.push_str(r#"">"#);
}

impl RenderBackend for HtmlBackend {
    /// Renders a status badge as a colored pill span.
    ///
    /// `color` is a closed enum ([`StatusColor`]), so it cannot inject markup
    /// into the class attribute.
    fn status_open(color: StatusColor, out: &mut String) {
        write!(out, r#"<span class="status status-{color}">"#).unwrap();
    }

    fn status_close(out: &mut String) {
        out.push_str("</span>");
    }

    fn code_block(lang: Option<&str>, content: &str, out: &mut String) {
        if let Some(lang) = lang {
            out.push_str(r#"<pre><code class="language-"#);
            escape_into(lang, out);
            out.push_str(r#"">"#);
            escape_into(content, out);
            out.push_str("</code></pre>");
        } else {
            out.push_str("<pre><code>");
            escape_into(content, out);
            out.push_str("</code></pre>");
        }
    }

    fn alert_start(kind: AlertKind, out: &mut String) {
        let (class, icon, title) = match kind {
            AlertKind::Note => ("note", SVG_INFO, "Note"),
            AlertKind::Tip => ("tip", SVG_LIGHTBULB, "Tip"),
            AlertKind::Important => ("important", SVG_REPORT, "Important"),
            AlertKind::Warning => ("warning", SVG_ALERT, "Warning"),
            AlertKind::Caution => ("caution", SVG_STOP, "Caution"),
        };
        write!(
            out,
            r#"<div class="alert alert-{class}"><div class="alert-title">{icon}{title}</div><div class="alert-content">"#
        )
        .unwrap();
    }

    fn alert_end(out: &mut String) {
        out.push_str("</div></div>");
    }

    fn image(src: &str, alt: &str, title: &str, out: &mut String) {
        out.push_str(r#"<img src=""#);
        escape_into(src, out);
        out.push('"');
        if !title.is_empty() {
            out.push_str(r#" title=""#);
            escape_into(title, out);
            out.push('"');
        }
        out.push_str(r#" alt=""#);
        escape_into(alt, out);
        out.push_str(r#"">"#);
    }

    /// Renders a diagram as a `<figure class="diagram">`.
    ///
    /// An inline SVG is wrapped in `<rw-diagram>`, which the viewer upgrades
    /// into a shadow root: generators emit ids unique only within one SVG, so
    /// a document-wide `url(#clip1)` would paint one diagram with another's
    /// clip paths.
    ///
    /// The author's alt text labels the figure around an inline SVG, or is the
    /// `<img>`'s `alt`. No `role="img"`: it would hide the SVG's links from
    /// assistive technology.
    fn diagram(view: &DiagramView<'_>, out: &mut String) {
        out.push_str(r#"<figure class="diagram""#);
        write_diagram_id_attr(view.id, out);
        if let (Asset::Inline(DiagramContent::Svg(_)), Some(alt)) = (view.asset, view.alt) {
            out.push_str(r#" aria-label=""#);
            escape_into(alt, out);
            out.push('"');
        }
        out.push('>');
        match view.asset {
            Asset::Inline(DiagramContent::Svg(svg)) => {
                out.push_str("<rw-diagram>");
                splice_link_attrs(svg, view.links, out);
                out.push_str("</rw-diagram>");
            }
            Asset::Inline(DiagramContent::Png(bytes)) => {
                out.push_str(r#"<img src="data:image/png;base64,"#);
                BASE64_STANDARD.encode_string(bytes, out);
                out.push('"');
                write_size_attrs(view.size, out);
                write_img_alt(view.alt, out);
            }
            Asset::Reference(name) => {
                out.push_str(r#"<img src=""#);
                escape_into(name, out);
                out.push('"');
                write_size_attrs(view.size, out);
                write_img_alt(view.alt, out);
            }
        }
        out.push_str("</figure>");
    }

    fn table_start(out: &mut String) {
        out.push_str(
            r#"<div class="table-wrap" role="group" tabindex="0" aria-label="Table"><table>"#,
        );
    }

    fn table_end(out: &mut String) {
        out.push_str("</tbody></table></div>");
    }

    fn tabs_open(group_id: usize, tabs: &[TabInfo], out: &mut String) {
        let _ = write!(out, r#"<div class="tabs" id="tabs-{group_id}">"#);
        out.push_str(r#"<div class="tabs-buttons" role="tablist">"#);
        for tab in tabs {
            let selected = tab.is_first;
            let _ = write!(
                out,
                r#"<button role="tab" id="tab-{group_id}-{0}" aria-controls="panel-{group_id}-{0}" aria-selected="{selected}" tabindex="{tabindex}">"#,
                tab.id,
                selected = selected,
                tabindex = if selected { "0" } else { "-1" },
            );
            escape_into(&tab.label, out);
            out.push_str("</button>");
        }
        out.push_str("</div>");
    }

    fn tab_panel_open(group_id: usize, tab: &TabInfo, out: &mut String) {
        let hidden = if tab.is_first { "" } else { " hidden" };
        let _ = write!(
            out,
            r#"<div role="tabpanel" id="panel-{group_id}-{}" aria-labelledby="tab-{group_id}-{}"{hidden}>"#,
            tab.id, tab.id
        );
    }

    fn tab_panel_close(out: &mut String) {
        out.push_str("</div>");
    }

    fn tabs_close(out: &mut String) {
        out.push_str("</div>");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_block_with_language() {
        let mut out = String::new();
        HtmlBackend::code_block(Some("rust"), "fn main() {}", &mut out);
        assert_eq!(
            out,
            r#"<pre><code class="language-rust">fn main() {}</code></pre>"#
        );
    }

    #[test]
    fn test_code_block_without_language() {
        let mut out = String::new();
        HtmlBackend::code_block(None, "plain code", &mut out);
        assert_eq!(out, "<pre><code>plain code</code></pre>");
    }

    #[test]
    fn test_blockquote() {
        let mut out = String::new();
        HtmlBackend::blockquote_start(&mut out);
        out.push_str("content");
        HtmlBackend::blockquote_end(&mut out);
        assert_eq!(out, "<blockquote>content</blockquote>");
    }

    #[test]
    fn test_image() {
        let mut out = String::new();
        HtmlBackend::image("image.png", "Alt text", "", &mut out);
        assert_eq!(out, r#"<img src="image.png" alt="Alt text">"#);
    }

    #[test]
    fn test_image_with_title() {
        let mut out = String::new();
        HtmlBackend::image("image.png", "Alt text", "Image title", &mut out);
        assert_eq!(
            out,
            r#"<img src="image.png" title="Image title" alt="Alt text">"#
        );
    }

    #[test]
    fn test_alerts() {
        for (kind, class, label) in [
            (AlertKind::Note, "alert-note", "Note"),
            (AlertKind::Tip, "alert-tip", "Tip"),
            (AlertKind::Important, "alert-important", "Important"),
            (AlertKind::Warning, "alert-warning", "Warning"),
            (AlertKind::Caution, "alert-caution", "Caution"),
        ] {
            let mut out = String::new();
            HtmlBackend::alert_start(kind, &mut out);
            out.push_str("<p>content</p>");
            HtmlBackend::alert_end(&mut out);
            assert!(
                out.contains(&format!(r#"class="alert {class}""#)),
                "{label}: {out}"
            );
            assert!(out.contains(r#"<svg class="alert-icon""#), "{label}: {out}");
            assert!(out.contains(label), "{label}: {out}");
            assert!(out.contains("<p>content</p>"), "{label}: {out}");
        }
    }

    /// The three diagram figure shapes are served to browsers with CSS and JS
    /// bound to their exact markup, so every assertion below is byte-exact.
    mod diagram {
        use crate::DiagramLink;

        use super::*;

        fn render(view: &DiagramView<'_>) -> String {
            let mut out = String::new();
            HtmlBackend::diagram(view, &mut out);
            out
        }

        fn svg(id: Option<&str>, source: &str, links: &[DiagramLink]) -> String {
            render(&DiagramView {
                id,
                asset: &Asset::Inline(DiagramContent::Svg(source.to_owned())),
                size: None,
                links,
                alt: None,
            })
        }

        fn png(id: Option<&str>, bytes: &[u8], size: Option<Size>) -> String {
            render(&DiagramView {
                id,
                asset: &Asset::Inline(DiagramContent::Png(bytes.to_vec())),
                size,
                links: &[],
                alt: None,
            })
        }

        fn reference(id: Option<&str>, name: &str, size: Option<Size>) -> String {
            render(&DiagramView {
                id,
                asset: &Asset::Reference(name.to_owned()),
                size,
                links: &[],
                alt: None,
            })
        }

        #[test]
        fn an_inline_svg_is_wrapped_in_rw_diagram() {
            assert_eq!(
                svg(None, "<svg><g/></svg>", &[]),
                r#"<figure class="diagram"><rw-diagram><svg><g/></svg></rw-diagram></figure>"#
            );
        }

        #[test]
        fn an_inline_svg_carries_its_diagram_id() {
            assert_eq!(
                svg(Some("diagram-2"), "<svg/>", &[]),
                r#"<figure class="diagram" data-diagram-id="diagram-2"><rw-diagram><svg/></rw-diagram></figure>"#
            );
        }

        #[test]
        fn a_diagram_id_is_attribute_escaped() {
            assert_eq!(
                svg(Some(r#"a"b"#), "<svg/>", &[]),
                r#"<figure class="diagram" data-diagram-id="a&quot;b"><rw-diagram><svg/></rw-diagram></figure>"#
            );
        }

        #[test]
        fn resolved_links_are_spliced_into_the_svg() {
            let links = [DiagramLink {
                href: "/domains/billing/api".to_owned(),
                section_ref: "domain:default/billing".to_owned(),
                section_path: "api".to_owned(),
            }];
            assert_eq!(
                svg(
                    None,
                    r#"<svg><a href="/domains/billing/api">x</a></svg>"#,
                    &links
                ),
                r#"<figure class="diagram"><rw-diagram><svg><a href="/domains/billing/api" data-section-ref="domain:default/billing" data-section-path="api">x</a></svg></rw-diagram></figure>"#
            );
        }

        #[test]
        fn an_svg_with_no_resolved_links_is_untouched() {
            assert_eq!(
                svg(None, r#"<svg><a href="/x">x</a></svg>"#, &[]),
                r#"<figure class="diagram"><rw-diagram><svg><a href="/x">x</a></svg></rw-diagram></figure>"#
            );
        }

        #[test]
        fn inline_png_bytes_become_a_sized_data_uri() {
            assert_eq!(
                png(
                    Some("d1"),
                    b"PNG",
                    Some(Size {
                        width: 200,
                        height: 100
                    })
                ),
                concat!(
                    r#"<figure class="diagram" data-diagram-id="d1">"#,
                    r#"<img src="data:image/png;base64,UE5H" width="200" height="100" alt="diagram">"#,
                    "</figure>",
                )
            );
        }

        /// A PNG whose header would not parse has no size to report. It renders
        /// unsized rather than not at all.
        #[test]
        fn an_unsized_png_renders_without_dimension_attributes() {
            assert_eq!(
                png(None, b"PNG", None),
                r#"<figure class="diagram"><img src="data:image/png;base64,UE5H" alt="diagram"></figure>"#
            );
        }

        #[test]
        fn a_reference_points_at_the_written_name() {
            assert_eq!(
                reference(
                    Some("d1"),
                    "diagram_abc123.png",
                    Some(Size {
                        width: 640,
                        height: 480
                    })
                ),
                concat!(
                    r#"<figure class="diagram" data-diagram-id="d1">"#,
                    r#"<img src="diagram_abc123.png" width="640" height="480" alt="diagram">"#,
                    "</figure>",
                )
            );
        }

        #[test]
        fn an_unsized_reference_renders_without_dimension_attributes() {
            assert_eq!(
                reference(None, "diagram_abc123.png", None),
                r#"<figure class="diagram"><img src="diagram_abc123.png" alt="diagram"></figure>"#
            );
        }

        #[test]
        fn a_reference_name_is_attribute_escaped() {
            assert_eq!(
                reference(None, r#"a"b.png"#, None),
                r#"<figure class="diagram"><img src="a&quot;b.png" alt="diagram"></figure>"#
            );
        }

        #[test]
        fn alt_text_labels_an_inline_svg_figure() {
            assert_eq!(
                render(&DiagramView {
                    id: Some("d1"),
                    asset: &Asset::Inline(DiagramContent::Svg("<svg/>".to_owned())),
                    size: None,
                    links: &[],
                    alt: Some(r#"Pay "now" & <later>"#),
                }),
                concat!(
                    r#"<figure class="diagram" data-diagram-id="d1" aria-label="Pay &quot;now&quot; &amp; &lt;later&gt;">"#,
                    "<rw-diagram><svg/></rw-diagram></figure>",
                )
            );
        }

        /// The figure stays unlabelled, or a screen reader would announce the
        /// text twice.
        #[test]
        fn alt_text_is_the_img_alt_for_both_raster_shapes() {
            for asset in [
                Asset::Inline(DiagramContent::Png(b"PNG".to_vec())),
                Asset::Reference("d.png".to_owned()),
            ] {
                let out = render(&DiagramView {
                    id: None,
                    asset: &asset,
                    size: None,
                    links: &[],
                    alt: Some(r#"Pay "now""#),
                });
                assert!(
                    out.starts_with(r#"<figure class="diagram"><img src=""#)
                        && out.ends_with(r#" alt="Pay &quot;now&quot;"></figure>"#),
                    "{out}"
                );
            }
        }

        #[test]
        fn an_error_figure_carries_the_escaped_message() {
            let mut out = String::new();
            HtmlBackend::diagram_error(Some("d1"), "syntax error at <line 3>", &mut out);
            assert_eq!(
                out,
                concat!(
                    r#"<figure class="diagram diagram-error" data-diagram-id="d1">"#,
                    "<pre>Diagram rendering failed: syntax error at &lt;line 3&gt;</pre>",
                    "</figure>",
                )
            );
        }

        #[test]
        fn an_error_figure_without_an_id_omits_the_attribute() {
            let mut out = String::new();
            HtmlBackend::diagram_error(None, "boom", &mut out);
            assert_eq!(
                out,
                r#"<figure class="diagram diagram-error"><pre>Diagram rendering failed: boom</pre></figure>"#
            );
        }
    }
}
