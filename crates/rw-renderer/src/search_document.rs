//! Search-optimized plain text backend.
//!
//! [`SearchDocumentBackend`] implements [`RenderBackend`] to produce
//! whitespace-separated tokens for search indexing (`PostgreSQL` `to_tsvector`,
//! Lunr, etc.). No HTML tags, no markdown markup — just text with spaces
//! between content boundaries.

use std::borrow::Cow;

use pulldown_cmark::Alignment;

use crate::RenderBackend;
use crate::backend::AlertKind;

/// Render backend that produces search-optimized plain text.
///
/// All structural markup is stripped. Content boundaries emit a single space
/// to prevent token merging. Inline formatting wrappers are no-ops — inner
/// text flows through via [`text()`](RenderBackend::text).
///
/// # Examples
///
/// ```
/// use rw_renderer::{MarkdownRenderer, SearchDocumentBackend};
///
/// let result = MarkdownRenderer::<SearchDocumentBackend>::new()
///     .with_title_extraction()
///     .render_markdown("# Title\n\nHello **world**.");
///
/// assert_eq!(result.title.as_deref(), Some("Title"));
/// assert_eq!(result.html.trim(), "Hello world.");
/// ```
pub struct SearchDocumentBackend;

impl RenderBackend for SearchDocumentBackend {
    const TITLE_AS_METADATA: bool = true;

    fn code_block(_lang: Option<&str>, content: &str, out: &mut String) {
        out.push_str(content);
        out.push(' ');
    }

    fn blockquote_start(_out: &mut String) {}
    fn blockquote_end(_out: &mut String) {}

    fn alert_start(_kind: AlertKind, _out: &mut String) {}
    fn alert_end(_kind: AlertKind, _out: &mut String) {}

    fn image(_src: &str, alt: &str, _title: &str, out: &mut String) {
        if !alt.is_empty() {
            out.push_str(alt);
        }
    }

    fn transform_link<'a>(url: &'a str, _base_path: Option<&str>) -> Cow<'a, str> {
        Cow::Borrowed(url)
    }

    fn hard_break(out: &mut String) {
        out.push(' ');
    }

    fn horizontal_rule(out: &mut String) {
        out.push(' ');
    }

    fn task_list_marker(_checked: bool, _out: &mut String) {}

    fn paragraph_start(_out: &mut String) {}

    fn paragraph_end(out: &mut String) {
        out.push(' ');
    }

    fn heading_start(_level: u8, _id: &str, _out: &mut String) {}

    fn heading_end(_level: u8, out: &mut String) {
        out.push(' ');
    }

    fn list_start(_ordered: bool, _start: Option<u64>, _out: &mut String) {}
    fn list_end(_ordered: bool, _out: &mut String) {}

    fn list_item_start(out: &mut String) {
        out.push(' ');
    }

    fn list_item_end(_out: &mut String) {}

    fn definition_list_start(_out: &mut String) {}
    fn definition_list_end(_out: &mut String) {}

    fn definition_title_start(out: &mut String) {
        out.push(' ');
    }

    fn definition_title_end(_out: &mut String) {}

    fn definition_detail_start(out: &mut String) {
        out.push(' ');
    }

    fn definition_detail_end(_out: &mut String) {}

    fn table_start(_out: &mut String) {}

    fn table_end(_out: &mut String) {}

    fn table_head_start(_out: &mut String) {}
    fn table_head_end(_out: &mut String) {}

    fn table_row_start(_out: &mut String) {}

    fn table_row_end(out: &mut String) {
        out.push(' ');
    }

    fn table_cell_start(_is_head: bool, _alignment: Option<Alignment>, out: &mut String) {
        out.push(' ');
    }

    fn table_cell_end(_is_head: bool, _out: &mut String) {}

    fn soft_break(out: &mut String) {
        out.push(' ');
    }

    fn emphasis_start(_out: &mut String) {}
    fn emphasis_end(_out: &mut String) {}
    fn strong_start(_out: &mut String) {}
    fn strong_end(_out: &mut String) {}
    fn strikethrough_start(_out: &mut String) {}
    fn strikethrough_end(_out: &mut String) {}
    fn superscript_start(_out: &mut String) {}
    fn superscript_end(_out: &mut String) {}
    fn subscript_start(_out: &mut String) {}
    fn subscript_end(_out: &mut String) {}

    fn inline_code(code: &str, out: &mut String) {
        out.push_str(code);
    }

    fn link_start(_href: &str, _section_ref: Option<(&str, &str)>, _out: &mut String) {}
    fn link_end(_out: &mut String) {}
    fn broken_link_start(_out: &mut String) {}

    fn text(text: &str, out: &mut String) {
        out.push_str(text);
    }

    fn raw_html(_html: &str, _out: &mut String) {}
}

#[cfg(test)]
mod tests {
    use crate::{MarkdownRenderer, SearchDocumentBackend};

    #[test]
    fn renders_plain_text() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .with_title_extraction()
            .render_markdown("# Title\n\nHello **world** and `code`.");

        assert_eq!(result.title.as_deref(), Some("Title"));
        assert_eq!(result.html.trim(), "Hello world and code.");
    }

    #[test]
    fn strips_inline_formatting() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .render_markdown("**bold** *italic* ~~strike~~ `code`");
        assert_eq!(result.html.trim(), "bold italic strike code");
    }

    #[test]
    fn link_keeps_display_text_drops_url() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .render_markdown("[Click here](https://example.com)");
        assert_eq!(result.html.trim(), "Click here");
    }

    #[test]
    fn image_outputs_alt_text() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .render_markdown("![A cute cat](cat.png)");
        assert_eq!(result.html.trim(), "A cute cat");
    }

    #[test]
    fn table_cells_separated_by_space() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .render_markdown("| A | B |\n|---|---|\n| C | D |");
        let text = result.html.trim();
        assert!(text.contains('A'));
        assert!(text.contains('B'));
        assert!(text.contains('C'));
        assert!(text.contains('D'));
        assert!(!text.contains('<'));
    }

    #[test]
    fn list_items_separated() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .render_markdown("- alpha\n- beta\n- gamma");
        let text = result.html.trim();
        assert!(text.contains("alpha"));
        assert!(text.contains("beta"));
        assert!(text.contains("gamma"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn code_block_included_as_is() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .render_markdown("```python\ndef hello():\n    pass\n```");
        assert!(result.html.contains("def hello():"));
        assert!(result.html.contains("pass"));
        assert!(!result.html.contains("<code"));
    }

    #[test]
    fn raw_html_stripped() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .render_markdown("before <b>bold</b> after");
        assert!(result.html.contains("before"));
        assert!(result.html.contains("after"));
        assert!(!result.html.contains("<b>"));
    }

    #[test]
    fn alert_content_included() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .with_gfm(true)
            .render_markdown("> [!WARNING]\n> Do not delete this file.");
        assert!(result.html.contains("Do not delete this file."));
        assert!(!result.html.contains('<'));
    }

    #[test]
    fn headings_in_body_included_title_excluded() {
        let result = MarkdownRenderer::<SearchDocumentBackend>::new()
            .with_title_extraction()
            .render_markdown("# Title\n\n## Section\n\nContent");
        assert_eq!(result.title.as_deref(), Some("Title"));
        assert!(result.html.contains("Section"));
        assert!(result.html.contains("Content"));
        assert!(!result.html.contains("Title"));
    }
}
