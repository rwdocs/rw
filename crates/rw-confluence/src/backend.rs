//! Confluence render backend.
//!
//! This module provides [`ConfluenceBackend`] for rendering markdown to
//! Confluence XHTML storage format.

use std::borrow::Cow;
use std::fmt::Write;

use percent_encoding::percent_decode_str;
use rw_renderer::{
    AlertKind, Asset, DiagramView, RenderBackend, StatusColor, TabInfo, escape_html, is_external,
    replace_controls_into,
};

/// The attachment a local image references while its `src` stays as written.
pub(crate) fn attachment_filename(src: &str) -> Cow<'_, str> {
    let basename = src.rsplit('/').next().unwrap_or(src);
    percent_decode_str(basename)
        .decode_utf8()
        .unwrap_or(Cow::Borrowed(basename))
}

/// Writes `<ac:image>` around one `<ri:…/>` resource, optionally sized and
/// with alt text.
///
/// `resource` is the whole resource tag's attributes — `ri:url ri:value="…"`
/// or `ri:attachment ri:filename="…"` — already escaped by the caller. `alt`
/// is raw text, escaped here.
///
/// `width` is a width in pixels and never a height: Confluence scales an image
/// proportionally from a single dimension, so supplying both could only ever
/// distort it.
fn ac_image(width: Option<u32>, alt: Option<&str>, resource: &str, out: &mut String) {
    out.push_str("<ac:image");
    if let Some(width) = width {
        write!(out, r#" ac:width="{width}""#).unwrap();
    }
    if let Some(alt) = alt {
        write!(out, r#" ac:alt="{}""#, escape_html(alt)).unwrap();
    }
    write!(out, "><{resource} /></ac:image>").unwrap();
}

/// Writes `text` as CDATA, the form a code macro body takes.
pub(crate) fn write_cdata(text: &str, out: &mut String) {
    out.push_str("<![CDATA[");
    // A literal `]]>` would end the section early.
    replace_controls_into(&text.replace("]]>", "]]]]><![CDATA[>"), out);
    out.push_str("]]>");
}

/// HTML elements that never have content. HTML lets their start tag stand
/// alone; storage format is XML and needs it self-closed.
const VOID_ELEMENTS: [&str; 13] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// The length through `>` of the void-element start tag that `tag` begins
/// with, or `None` if it does not begin with one that still needs closing.
///
/// A tag cut off before its `>` is left alone: raw HTML arrives one parser
/// event at a time, and there is no telling where the rest went.
fn open_void_tag_len(tag: &str) -> Option<usize> {
    let name_end = tag[1..]
        .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
        .map_or(tag.len(), |i| i + 1);
    let name = &tag[1..name_end];
    if !VOID_ELEMENTS
        .iter()
        .any(|void| void.eq_ignore_ascii_case(name))
    {
        return None;
    }
    let mut quote = None;
    for (i, c) in tag[name_end..].char_indices() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => {
                let end = name_end + i;
                let head = tag[..end].trim_end();
                // `-->` closes a comment, perhaps opened in an earlier event;
                // inserting a `/` would leave it open.
                let closed = head.ends_with('/') || head.ends_with("--");
                return (!closed).then_some(end + 1);
            }
            _ => {}
        }
    }
    None
}

/// Confluence render backend.
///
/// Produces Confluence XHTML storage format with:
/// - `ac:structured-macro` for code blocks
/// - Info panel macro for blockquotes
/// - `ac:image` with `ri:url` or `ri:attachment` for images
/// - Title extraction from the opening H1 with level shifting
pub(crate) struct ConfluenceBackend;

impl RenderBackend for ConfluenceBackend {
    /// Confluence storage format has no place for a per-figure id: a diagram is
    /// an `<ac:image>` macro, not markup this backend can hang an attribute on.
    const DIAGRAM_IDS: bool = false;

    fn code_block(lang: Option<&str>, content: &str, out: &mut String) {
        out.push_str(r#"<ac:structured-macro ac:name="code" ac:schema-version="1">"#);
        if let Some(lang) = lang {
            write!(
                out,
                r#"<ac:parameter ac:name="language">{}</ac:parameter>"#,
                escape_html(lang)
            )
            .unwrap();
        }
        out.push_str(r#"<ac:parameter ac:name="linenumbers">true</ac:parameter>"#);
        out.push_str("<ac:plain-text-body>");
        write_cdata(content, out);
        out.push_str("</ac:plain-text-body></ac:structured-macro>");
    }

    fn blockquote_start(out: &mut String) {
        out.push_str(
            r#"<ac:structured-macro ac:name="info" ac:schema-version="1"><ac:rich-text-body>"#,
        );
    }

    fn blockquote_end(out: &mut String) {
        out.push_str("</ac:rich-text-body></ac:structured-macro>");
    }

    fn alert_start(kind: AlertKind, out: &mut String) {
        let macro_name = match kind {
            AlertKind::Note => "info",
            AlertKind::Tip => "tip",
            AlertKind::Important => "note",
            AlertKind::Warning | AlertKind::Caution => "warning",
        };
        write!(
            out,
            r#"<ac:structured-macro ac:name="{macro_name}" ac:schema-version="1"><ac:rich-text-body>"#
        )
        .unwrap();
    }

    fn alert_end(out: &mut String) {
        out.push_str("</ac:rich-text-body></ac:structured-macro>");
    }

    /// A local image becomes an attachment reference by its basename: a
    /// bundled image's `src` already is its attachment name, and the
    /// publisher uploads any other.
    fn image(src: &str, alt: &str, _title: &str, out: &mut String) {
        let inner = if is_external(src) {
            format!(r#"ri:url ri:value="{}""#, escape_html(src))
        } else {
            format!(
                r#"ri:attachment ri:filename="{}""#,
                escape_html(&attachment_filename(src))
            )
        };
        ac_image(None, (!alt.is_empty()).then_some(alt), &inner, out);
    }

    fn raw_html(html: &str, out: &mut String) {
        let mut rest = html;
        while let Some(lt) = rest.find('<') {
            let (before, tag) = rest.split_at(lt);
            out.push_str(before);
            if let Some(len) = open_void_tag_len(tag) {
                out.push_str(tag[..len - 1].trim_end());
                out.push_str(" />");
                rest = &tag[len..];
            } else {
                out.push('<');
                rest = &tag[1..];
            }
        }
        out.push_str(rest);
    }

    /// Renders a diagram as an attachment reference.
    ///
    /// Only [`Asset::Reference`] should occur: rw-confluence writes every
    /// diagram out as an attachment before finishing the pass, so by the time
    /// markup is produced the bytes already have a filename.
    ///
    /// Inline bytes have nowhere to go in storage format. They become an error
    /// figure rather than nothing at all: writing nothing drops the diagram off
    /// the published page with no warning and no gap to notice, and the fill is
    /// still a fill, so no assertion catches it either.
    fn diagram(view: &DiagramView<'_>, out: &mut String) {
        let Asset::Reference(filename) = view.asset else {
            Self::diagram_error(
                view.id,
                "the diagram was rendered but never saved as a page attachment, \
                 so there is nothing for Confluence to reference; check that \
                 the render was given a bundle directory to write attachments into",
                out,
            );
            return;
        };
        let resource = format!(r#"ri:attachment ri:filename="{}""#, escape_html(filename));
        ac_image(view.size.map(|size| size.width), view.alt, &resource, out);
    }

    fn hard_break(out: &mut String) {
        out.push_str("<br />");
    }

    fn horizontal_rule(out: &mut String) {
        out.push_str("<hr />");
    }

    fn task_list_marker(checked: bool, out: &mut String) {
        out.push_str(if checked { "[x] " } else { "[ ] " });
    }

    /// Opens the Confluence `status` structured macro: the macro tag, the
    /// `colour` parameter, and an open `title` parameter. [`status_close`](Self::status_close)
    /// closes that `title` parameter and the macro; the label in between flows
    /// through the normal `text` path.
    fn status_open(color: StatusColor, out: &mut String) {
        // Confluence's `colour` parameter expects capitalized names.
        let confluence_color = match color {
            StatusColor::Grey => "Grey",
            StatusColor::Red => "Red",
            StatusColor::Yellow => "Yellow",
            StatusColor::Green => "Green",
            StatusColor::Blue => "Blue",
            StatusColor::Purple => "Purple",
        };
        write!(
            out,
            concat!(
                r#"<ac:structured-macro ac:name="status" ac:schema-version="1">"#,
                r#"<ac:parameter ac:name="colour">{}</ac:parameter>"#,
                r#"<ac:parameter ac:name="title">"#,
            ),
            confluence_color
        )
        .unwrap();
    }

    /// Closes the `title` parameter and the `status` macro opened by
    /// [`status_open`](Self::status_open).
    fn status_close(out: &mut String) {
        out.push_str("</ac:parameter></ac:structured-macro>");
    }

    /// Renders a tab as a bold-label section: `<p><strong>Label</strong></p>`,
    /// always visible. Confluence storage format has no tabs macro; the bar,
    /// panel close, and group close are the trait's no-ops.
    fn tab_panel_open(_group_id: usize, tab: &TabInfo, out: &mut String) {
        write!(out, "<p><strong>{}</strong></p>", escape_html(&tab.label)).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use rw_renderer::{MarkdownRenderer, Size};

    use super::*;

    fn diagram_markup(asset: &Asset, size: Option<Size>) -> String {
        diagram_markup_with_alt(asset, size, None)
    }

    fn diagram_markup_with_alt(asset: &Asset, size: Option<Size>, alt: Option<&str>) -> String {
        let mut out = String::new();
        ConfluenceBackend::diagram(
            &DiagramView {
                id: Some("ignored"),
                asset,
                size,
                links: &[],
                alt,
            },
            &mut out,
        );
        out
    }

    /// The backend formats the display width it is given; the oversampling
    /// correction happens in the provider, so this backend never applies a DPI
    /// correction of its own.
    ///
    /// Whole-tag equality: height is deliberately absent, because Confluence
    /// scales proportionally from a single dimension and emitting both could
    /// only ever distort the diagram.
    #[test]
    fn emits_the_display_width_it_is_given() {
        assert_eq!(
            diagram_markup(
                &Asset::Reference("diagram_abc123.png".to_owned()),
                Some(Size {
                    width: 200,
                    height: 100
                })
            ),
            r#"<ac:image ac:width="200"><ri:attachment ri:filename="diagram_abc123.png" /></ac:image>"#
        );
    }

    /// An unknown size drops the attribute rather than guessing one; Confluence
    /// then renders the attachment at its natural size.
    ///
    /// Whole-tag equality, so it also pins what is *not* there: the id
    /// `diagram_markup` passes in (`DIAGRAM_IDS` is false, and storage format
    /// has no attribute to carry one).
    #[test]
    fn an_unsized_diagram_gets_no_width_attribute() {
        assert_eq!(
            diagram_markup(&Asset::Reference("test.png".to_owned()), None),
            r#"<ac:image><ri:attachment ri:filename="test.png" /></ac:image>"#
        );
    }

    #[test]
    fn alt_text_becomes_an_escaped_ac_alt() {
        assert_eq!(
            diagram_markup_with_alt(
                &Asset::Reference("d.png".to_owned()),
                Some(Size {
                    width: 200,
                    height: 100
                }),
                Some(r#"Pay "now" & <later>"#),
            ),
            r#"<ac:image ac:width="200" ac:alt="Pay &quot;now&quot; &amp; &lt;later&gt;"><ri:attachment ri:filename="d.png" /></ac:image>"#
        );
    }

    /// Inline bytes cannot be referenced from storage format, so they become a
    /// visible error figure. `force_png` plus `write_attachments` make this
    /// unreachable today; the point is that a future provider which does reach
    /// it loses a diagram loudly instead of silently.
    #[test]
    fn inline_bytes_produce_an_error_figure() {
        use rw_renderer::DiagramContent;

        for asset in [
            Asset::Inline(DiagramContent::Svg("<svg/>".to_owned())),
            Asset::Inline(DiagramContent::Png(Vec::from([0x89]))),
        ] {
            let markup = diagram_markup(&asset, None);
            assert!(
                markup.contains("Diagram rendering failed:"),
                "an inline asset must not vanish: {markup:?}"
            );
            assert!(
                markup.contains("never saved as a page attachment"),
                "the message must name the situation: {markup:?}"
            );
            assert!(
                !markup.contains("<ac:image"),
                "there is no filename to reference: {markup:?}"
            );
        }
    }

    /// Both `image` and `diagram` go through the same `<ac:image>` writer, so a
    /// local image and a diagram produce the same `<ac:image>` wrapper.
    #[test]
    fn a_local_image_is_an_unsized_attachment_reference() {
        let mut out = String::new();
        ConfluenceBackend::image("./images/diagram.png", "", "title", &mut out);
        assert_eq!(
            out,
            r#"<ac:image><ri:attachment ri:filename="diagram.png" /></ac:image>"#
        );
    }

    #[test]
    fn a_local_image_carries_its_alt_text() {
        let mut out = String::new();
        ConfluenceBackend::image("a.png", r#"Pay "now""#, "", &mut out);
        assert_eq!(
            out,
            r#"<ac:image ac:alt="Pay &quot;now&quot;"><ri:attachment ri:filename="a.png" /></ac:image>"#
        );
    }

    fn image_markup(src: &str) -> String {
        let mut out = String::new();
        ConfluenceBackend::image(src, "", "", &mut out);
        out
    }

    /// The bundle names attachments as files on disk, so the escape a
    /// markdown link needs for a space must not reach the filename.
    #[test]
    fn a_local_image_names_its_attachment_by_the_decoded_basename() {
        assert_eq!(
            image_markup("images/a%20b.png"),
            r#"<ac:image><ri:attachment ri:filename="a b.png" /></ac:image>"#
        );
    }

    /// The renderer resolves no file for these, so there is nothing to attach.
    #[test]
    fn srcs_with_a_scheme_or_host_are_urls_not_attachments() {
        for src in [
            "data:image/png;base64,iVBORw0KGgo=",
            "//cdn.example.com/x.png",
            "HTTPS://example.com/x.png",
            "ftp://example.com/x.png",
            r"\\cdn.example.com/x.png",
        ] {
            assert_eq!(
                image_markup(src),
                format!(r#"<ac:image><ri:url ri:value="{src}" /></ac:image>"#),
            );
        }
    }

    fn raw(html: &str) -> String {
        let mut out = String::new();
        ConfluenceBackend::raw_html(html, &mut out);
        out
    }

    /// Storage format is XML, where an HTML void element left open swallows
    /// the rest of the page.
    #[test]
    fn raw_void_elements_are_self_closed() {
        for (html, expected) in [
            ("<br>", "<br />"),
            ("<BR>", "<BR />"),
            ("<br >", "<br />"),
            ("<wbr>", "<wbr />"),
            ("<hr>\n", "<hr />\n"),
            (
                r#"<img src="a.png" alt='x'>"#,
                r#"<img src="a.png" alt='x' />"#,
            ),
            ("a<br>b<br>c", "a<br />b<br />c"),
        ] {
            assert_eq!(raw(html), expected, "{html}");
        }
    }

    #[test]
    fn a_gt_inside_a_quoted_attribute_does_not_end_the_tag() {
        assert_eq!(
            raw(r#"<img alt="x>y" title='a>b' src="a.png">"#),
            r#"<img alt="x>y" title='a>b' src="a.png" />"#
        );
    }

    #[test]
    fn raw_html_that_needs_no_closing_is_unchanged() {
        for html in [
            "<br/>",
            "<br />",
            "<img src=\"a.png\"/>",
            "<p>text</p>",
            "<brx>",
            "<b>bold</b>",
            "</br>",
            "<!-- <br -->",
            "a < b",
            // A tag cut off at the end of the event: nothing safe to add.
            "<img src=\"a.png\"",
        ] {
            assert_eq!(raw(html), html);
        }
    }

    /// Closing these would write a `/>` onto an element that is not void.
    #[test]
    fn a_tag_whose_name_only_starts_with_a_void_name_is_unchanged() {
        for html in ["<br_x>", "<img.x>", "<br:x>", "<brü>", "<br-x>"] {
            assert_eq!(raw(html), html);
        }
    }

    /// Raw HTML reaches the backend one parser event at a time, and an inline
    /// tag that spans lines is still one event.
    #[test]
    fn an_inline_void_tag_spanning_lines_is_closed() {
        let html = MarkdownRenderer::<ConfluenceBackend>::new()
            .render("Inline <img src=\"b.png\"\ntitle=\"t\"> split.\n")
            .html;
        assert!(html.contains("title=\"t\" />"), "{html}");
    }

    #[test]
    fn test_code_block_with_language() {
        let mut out = String::new();
        ConfluenceBackend::code_block(Some("python"), "print('hello')", &mut out);
        assert!(out.contains(r#"ac:name="code""#));
        assert!(out.contains(r#"ac:name="language">python"#));
        assert!(out.contains("print('hello')"));
        assert!(out.contains("<![CDATA["));
    }

    #[test]
    fn a_code_block_replaces_control_characters() {
        let mut out = String::new();
        ConfluenceBackend::code_block(Some("py\u{1}"), "a\u{0}b\u{1B}\u{C}]]>\t\n", &mut out);
        assert_eq!(
            out,
            "<ac:structured-macro ac:name=\"code\" ac:schema-version=\"1\">\
             <ac:parameter ac:name=\"language\">py\u{FFFD}</ac:parameter>\
             <ac:parameter ac:name=\"linenumbers\">true</ac:parameter>\
             <ac:plain-text-body><![CDATA[a\u{FFFD}b\u{FFFD} ]]]]><![CDATA[>\t\n]]>\
             </ac:plain-text-body></ac:structured-macro>"
        );
    }

    #[test]
    fn a_page_with_control_characters_renders_without_them() {
        let html = MarkdownRenderer::<ConfluenceBackend>::new()
            .render(
                "## Sec\u{1}tion\n\nText\u{0}\u{C} `co\u{2}de` [li\u{3}nk](<a\u{4}b>) \
                 ![al\u{5}t](<https://x/i\u{6}.png>)\n\n    indented\u{7}\n",
            )
            .html;
        assert_eq!(
            html,
            "<h2 id=\"section\">Sec\u{FFFD}tion</h2>\
             <p>Text\u{FFFD}  <code>co\u{FFFD}de</code> <a href=\"a\u{FFFD}b\">li\u{FFFD}nk</a> \
             <ac:image ac:alt=\"al\u{FFFD}t\"><ri:url ri:value=\"https://x/i\u{FFFD}.png\" /></ac:image></p>\
             <ac:structured-macro ac:name=\"code\" ac:schema-version=\"1\">\
             <ac:parameter ac:name=\"linenumbers\">true</ac:parameter>\
             <ac:plain-text-body><![CDATA[indented\u{FFFD}\n]]></ac:plain-text-body></ac:structured-macro>"
        );
    }

    #[test]
    fn test_code_block_without_language() {
        let mut out = String::new();
        ConfluenceBackend::code_block(None, "plain code", &mut out);
        assert!(out.contains(r#"ac:name="code""#));
        assert!(!out.contains(r#"ac:name="language""#));
        assert!(out.contains("plain code"));
    }

    #[test]
    fn test_blockquote() {
        let mut out = String::new();
        ConfluenceBackend::blockquote_start(&mut out);
        out.push_str("content");
        ConfluenceBackend::blockquote_end(&mut out);
        assert!(out.contains(r#"ac:name="info""#));
        assert!(out.contains("<ac:rich-text-body>content</ac:rich-text-body>"));
    }

    #[test]
    fn test_external_image() {
        let mut out = String::new();
        ConfluenceBackend::image("https://example.com/image.png", "", "title", &mut out);
        assert!(out.contains(r"<ac:image>"));
        assert!(out.contains(r#"ri:url ri:value="https://example.com/image.png""#));
    }

    #[test]
    fn test_hard_break() {
        let mut out = String::new();
        ConfluenceBackend::hard_break(&mut out);
        assert_eq!(out, "<br />");
    }

    #[test]
    fn test_horizontal_rule() {
        let mut out = String::new();
        ConfluenceBackend::horizontal_rule(&mut out);
        assert_eq!(out, "<hr />");
    }

    #[test]
    fn test_task_list_marker() {
        let mut out = String::new();
        ConfluenceBackend::task_list_marker(false, &mut out);
        assert_eq!(out, "[ ] ");

        let mut out = String::new();
        ConfluenceBackend::task_list_marker(true, &mut out);
        assert_eq!(out, "[x] ");
    }

    /// Confluence has no "important" or "caution" panel, so those borrow the
    /// nearest of its four macros.
    #[test]
    fn alerts_map_to_confluence_panel_macros() {
        for (kind, macro_name) in [
            (AlertKind::Note, "info"),
            (AlertKind::Tip, "tip"),
            (AlertKind::Important, "note"),
            (AlertKind::Warning, "warning"),
            (AlertKind::Caution, "warning"),
        ] {
            let mut out = String::new();
            ConfluenceBackend::alert_start(kind, &mut out);
            out.push_str("<p>content</p>");
            ConfluenceBackend::alert_end(&mut out);
            assert_eq!(
                out,
                format!(
                    r#"<ac:structured-macro ac:name="{macro_name}" ac:schema-version="1"><ac:rich-text-body><p>content</p></ac:rich-text-body></ac:structured-macro>"#
                ),
                "{kind:?}"
            );
        }
    }
}
