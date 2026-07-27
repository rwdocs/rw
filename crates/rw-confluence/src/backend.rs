//! Confluence render backend.
//!
//! This module provides [`ConfluenceBackend`] for rendering markdown to
//! Confluence XHTML storage format.

use std::fmt::Write;

use rw_renderer::{
    AlertKind, Asset, DiagramView, RenderBackend, StatusColor, TabInfo, escape_html,
};

/// Writes `<ac:image>` around one `<ri:…/>` resource, optionally sized.
///
/// `resource` is the whole resource tag's attributes — `ri:url ri:value="…"`
/// or `ri:attachment ri:filename="…"` — already escaped by the caller.
///
/// `width` is a width in pixels and never a height: Confluence scales an image
/// proportionally from a single dimension, so supplying both could only ever
/// distort it.
fn ac_image(width: Option<u32>, resource: &str, out: &mut String) {
    match width {
        Some(width) => write!(out, r#"<ac:image ac:width="{width}">"#).unwrap(),
        None => out.push_str("<ac:image>"),
    }
    write!(out, "<{resource} /></ac:image>").unwrap();
}

/// Confluence render backend.
///
/// Produces Confluence XHTML storage format with:
/// - `ac:structured-macro` for code blocks
/// - Info panel macro for blockquotes
/// - `ac:image` with `ri:url` or `ri:attachment` for images
/// - Title extraction from first H1 with level shifting
pub(crate) struct ConfluenceBackend;

impl RenderBackend for ConfluenceBackend {
    const TITLE_AS_METADATA: bool = true;

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
        // CDATA content is not escaped
        write!(
            out,
            r"<ac:plain-text-body><![CDATA[{content}]]></ac:plain-text-body>"
        )
        .unwrap();
        out.push_str("</ac:structured-macro>");
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

    fn alert_end(_kind: AlertKind, out: &mut String) {
        out.push_str("</ac:rich-text-body></ac:structured-macro>");
    }

    fn image(src: &str, _alt: &str, _title: &str, out: &mut String) {
        // Confluence doesn't use alt/title attributes in the same way
        let is_external = src.starts_with("http://") || src.starts_with("https://");
        let inner = if is_external {
            format!(r#"ri:url ri:value="{}""#, escape_html(src))
        } else {
            // Local file - assume it will be uploaded as attachment
            let filename = src.rsplit('/').next().unwrap_or(src);
            format!(r#"ri:attachment ri:filename="{}""#, escape_html(filename))
        };
        ac_image(None, &inner, out);
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
        ac_image(view.size.map(|size| size.width), &resource, out);
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
    use rw_renderer::Size;

    use super::*;

    fn diagram_markup(asset: &Asset, size: Option<Size>) -> String {
        let mut out = String::new();
        ConfluenceBackend::diagram(
            &DiagramView {
                id: Some("ignored"),
                asset,
                size,
                links: &[],
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
        ConfluenceBackend::image("./images/diagram.png", "alt", "title", &mut out);
        assert_eq!(
            out,
            r#"<ac:image><ri:attachment ri:filename="diagram.png" /></ac:image>"#
        );
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
        ConfluenceBackend::image("https://example.com/image.png", "alt", "title", &mut out);
        assert!(out.contains(r"<ac:image>"));
        assert!(out.contains(r#"ri:url ri:value="https://example.com/image.png""#));
    }

    #[test]
    fn test_local_image() {
        let mut out = String::new();
        ConfluenceBackend::image("./images/diagram.png", "alt", "title", &mut out);
        assert!(out.contains(r"<ac:image>"));
        assert!(out.contains(r#"ri:attachment ri:filename="diagram.png""#));
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

    #[test]
    fn test_alert_note() {
        let mut out = String::new();
        ConfluenceBackend::alert_start(AlertKind::Note, &mut out);
        out.push_str("<p>content</p>");
        ConfluenceBackend::alert_end(AlertKind::Note, &mut out);
        assert!(out.contains(r#"ac:name="info""#));
        assert!(out.contains("<ac:rich-text-body><p>content</p></ac:rich-text-body>"));
    }

    #[test]
    fn test_alert_tip() {
        let mut out = String::new();
        ConfluenceBackend::alert_start(AlertKind::Tip, &mut out);
        ConfluenceBackend::alert_end(AlertKind::Tip, &mut out);
        assert!(out.contains(r#"ac:name="tip""#));
    }

    #[test]
    fn test_alert_important() {
        let mut out = String::new();
        ConfluenceBackend::alert_start(AlertKind::Important, &mut out);
        ConfluenceBackend::alert_end(AlertKind::Important, &mut out);
        // Important maps to "note" macro in Confluence
        assert!(out.contains(r#"ac:name="note""#));
    }

    #[test]
    fn test_alert_warning() {
        let mut out = String::new();
        ConfluenceBackend::alert_start(AlertKind::Warning, &mut out);
        ConfluenceBackend::alert_end(AlertKind::Warning, &mut out);
        assert!(out.contains(r#"ac:name="warning""#));
    }

    #[test]
    fn test_alert_caution() {
        let mut out = String::new();
        ConfluenceBackend::alert_start(AlertKind::Caution, &mut out);
        ConfluenceBackend::alert_end(AlertKind::Caution, &mut out);
        // Caution also maps to "warning" macro in Confluence
        assert!(out.contains(r#"ac:name="warning""#));
    }

    #[test]
    fn status_open_emits_macro_prefix() {
        let mut out = String::new();
        ConfluenceBackend::status_open(StatusColor::Green, &mut out);
        assert!(out.contains(r#"ac:name="status""#), "got: {out}");
        assert!(
            out.contains(r#"<ac:parameter ac:name="colour">Green</ac:parameter>"#),
            "got: {out}"
        );
        assert!(
            out.ends_with(r#"<ac:parameter ac:name="title">"#),
            "got: {out}"
        );
    }

    #[test]
    fn status_open_renders_grey() {
        let mut out = String::new();
        ConfluenceBackend::status_open(StatusColor::Grey, &mut out);
        assert!(
            out.contains(r#"<ac:parameter ac:name="colour">Grey</ac:parameter>"#),
            "got: {out}"
        );
    }

    #[test]
    fn status_close_emits_macro_suffix() {
        let mut out = String::new();
        ConfluenceBackend::status_close(&mut out);
        assert_eq!(out, "</ac:parameter></ac:structured-macro>");
    }
}
