//! Restricted markdown rendering for inline/page comment bodies.
//!
//! Comment bodies are authored in a small CommonMark+GFM subset suited to a
//! narrow comment column. [`render_comment_body`] renders that subset to safe
//! HTML using a private `CommentBackend`, which is safe by construction: raw
//! HTML is escaped (never passed through), only a fixed set of known tags is
//! emitted, and link schemes are allow-listed.

use std::fmt::Write;

use pulldown_cmark::Alignment;

use crate::backend::RenderBackend;
use crate::{HtmlBackend, MarkdownRenderer, Pipeline, escape_html};
use rw_parser::AlertKind;

/// Render a comment `body` (markdown) to safe HTML for display.
///
/// Renders a restricted CommonMark+GFM subset: paragraphs, line breaks,
/// emphasis, lists, task lists, blockquotes, inline + fenced code, and
/// `http`/`https`/`mailto` links. Headings are demoted to paragraphs, tables
/// are flattened to text, images are dropped, GitHub alerts render as plain
/// blockquotes, raw HTML is escaped, and links with other schemes lose their
/// `href`. Blank/whitespace input renders to an empty string.
#[must_use]
pub fn render_comment_body(markdown: &str) -> String {
    MarkdownRenderer::<CommentBackend>::new()
        .render(markdown, Pipeline::new())
        .html
}

/// Allow-listed comment link schemes. Anything else (relative links,
/// `javascript:`, `data:`, `tel:`, …) renders as a bare, non-clickable `<a>`.
fn is_allowed_link_scheme(href: &str) -> bool {
    let lower = href.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Restricted [`RenderBackend`] for comment bodies (see [`render_comment_body`]).
///
/// Standalone implementation (backends do not inherit from each other). It
/// keeps the trait's HTML5 defaults for paragraphs, lists, emphasis, inline
/// code, text, breaks, rules, task lists, and `link_end` — all already safe and
/// escaping — and overrides only the restricted constructs.
///
/// Private to the crate: `render_comment_body` is the only entry point, so the
/// restricted profile (notably, wikilinks are never enabled — broken wikilinks
/// can't emit clickable `<a href="#">`) cannot be bypassed by an external
/// caller building its own `MarkdownRenderer::<CommentBackend>`.
struct CommentBackend;

impl RenderBackend for CommentBackend {
    const TITLE_AS_METADATA: bool = false;

    fn code_block(lang: Option<&str>, content: &str, out: &mut String) {
        // Fenced/indented code renders like the page backend (escapes content).
        HtmlBackend::code_block(lang, content, out);
    }

    fn blockquote_start(out: &mut String) {
        HtmlBackend::blockquote_start(out);
    }

    fn blockquote_end(out: &mut String) {
        HtmlBackend::blockquote_end(out);
    }

    fn alert_start(_kind: AlertKind, out: &mut String) {
        // GitHub alerts (`> [!NOTE]`) → plain blockquote; the full SVG callout
        // is too heavy for the narrow comment column.
        out.push_str("<blockquote>");
    }

    fn alert_end(_kind: AlertKind, out: &mut String) {
        out.push_str("</blockquote>");
    }

    fn image(_src: &str, _alt: &str, _title: &str, _out: &mut String) {
        // Images are dropped entirely. The walker pops the image scope before
        // calling this, so a no-op leaves no stray output.
    }

    fn heading_start(_level: u8, _id: &str, out: &mut String) {
        // Headings demote to a paragraph (no oversized <h1> in a thread).
        out.push_str("<p>");
    }

    fn heading_end(_level: u8, out: &mut String) {
        out.push_str("</p>");
    }

    // Tables flatten to plain text: emit no table tags, keep cell text, and add
    // a separating space after each cell so words don't run together.
    fn table_start(_out: &mut String) {}
    fn table_end(_out: &mut String) {}
    fn table_head_start(_out: &mut String) {}
    fn table_head_end(_out: &mut String) {}
    fn table_row_start(_out: &mut String) {}
    fn table_row_end(_out: &mut String) {}
    fn table_cell_start(_is_head: bool, _alignment: Option<Alignment>, _out: &mut String) {}
    fn table_cell_end(_is_head: bool, out: &mut String) {
        out.push(' ');
    }

    fn link_start(href: &str, _section_ref: Option<(&str, &str)>, out: &mut String) {
        // Always emit a balanced <a> (link_end always writes </a>), but include
        // href only for allow-listed schemes. Disallowed → bare, non-clickable.
        if is_allowed_link_scheme(href) {
            write!(out, r#"<a href="{}">"#, escape_html(href)).unwrap();
        } else {
            out.push_str("<a>");
        }
    }

    fn raw_html(html: &str, out: &mut String) {
        // Escape raw HTML to inert text — never pass it through.
        out.push_str(&escape_html(html));
    }
}

#[cfg(test)]
mod tests {
    use super::render_comment_body;

    #[test]
    fn two_paragraphs_stay_separate() {
        // The reported bug: blank-line-separated paragraphs must NOT collapse.
        let html = render_comment_body("First para.\n\nSecond para.");
        assert_eq!(html.matches("<p>").count(), 2, "got: {html}");
        assert!(html.contains("First para."));
        assert!(html.contains("Second para."));
    }

    #[test]
    fn inline_emphasis_renders() {
        let html = render_comment_body("**b** _i_ ~~s~~");
        assert!(html.contains("<strong>b</strong>"), "got: {html}");
        assert!(html.contains("<em>i</em>"), "got: {html}");
        assert!(html.contains("<s>s</s>"), "got: {html}");
    }

    #[test]
    fn lists_and_task_lists_render() {
        let ul = render_comment_body("- a\n- b");
        assert!(
            ul.contains("<ul>") && ul.contains("<li>a</li>"),
            "got: {ul}"
        );
        let ol = render_comment_body("1. a\n2. b");
        assert!(ol.contains("<ol>"), "got: {ol}");
        let tasks = render_comment_body("- [x] done\n- [ ] todo");
        assert!(tasks.contains(r#"type="checkbox" checked"#), "got: {tasks}");
    }

    #[test]
    fn inline_and_fenced_code_render() {
        assert!(render_comment_body("`x`").contains("<code>x</code>"));
        let fenced = render_comment_body("```\ncode\n```");
        assert!(fenced.contains("<pre><code>"), "got: {fenced}");
    }

    #[test]
    fn blockquote_and_alerts_render_as_blockquote() {
        assert!(render_comment_body("> quote").contains("<blockquote>"));
        let alert = render_comment_body("> [!NOTE]\n> heads up");
        assert!(alert.contains("<blockquote>"), "got: {alert}");
        assert!(!alert.contains("alert"), "alert callout leaked: {alert}");
        assert!(!alert.contains("<svg"), "svg icon leaked: {alert}");
    }

    #[test]
    fn headings_demote_to_paragraph() {
        let html = render_comment_body("# Big");
        assert!(html.contains("<p>Big</p>"), "got: {html}");
        assert!(!html.contains("<h1"), "heading leaked: {html}");
    }

    #[test]
    fn tables_flatten_to_text() {
        let html = render_comment_body("| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(!html.contains("<table"), "table leaked: {html}");
        assert!(
            html.contains('a') && html.contains('1'),
            "cell text lost: {html}"
        );
    }

    #[test]
    fn images_are_dropped() {
        let html = render_comment_body("![alt](http://example.com/i.png)");
        assert!(!html.contains("<img"), "img leaked: {html}");
    }

    #[test]
    fn allowed_links_keep_href() {
        let http = render_comment_body("[x](https://example.com)");
        assert!(
            http.contains(r#"<a href="https://example.com">x</a>"#),
            "got: {http}"
        );
        let mail = render_comment_body("[x](mailto:a@b.com)");
        assert!(mail.contains(r#"href="mailto:a@b.com""#), "got: {mail}");
    }

    #[test]
    fn javascript_link_is_neutralized_without_dangling_tag() {
        let html = render_comment_body("[x](javascript:alert(1))");
        assert!(
            html.contains("<a>x</a>"),
            "expected bare anchor, got: {html}"
        );
        assert!(!html.contains("href"), "href leaked: {html}");
        assert!(!html.contains("javascript"), "scheme leaked: {html}");
    }

    #[test]
    fn raw_html_is_escaped() {
        let script = render_comment_body("<script>alert(1)</script>");
        assert!(
            !script.contains("<script>"),
            "script passed through: {script}"
        );
        assert!(script.contains("&lt;script&gt;"), "got: {script}");
        let img = render_comment_body("<img src=x onerror=alert(1)>");
        assert!(!img.contains("<img"), "raw img passed through: {img}");
        assert!(img.contains("&lt;img"), "got: {img}");
    }

    #[test]
    fn blank_body_renders_empty() {
        assert_eq!(render_comment_body(""), "");
        // Whitespace-only input yields no markdown blocks → exactly empty (no
        // stray `<p> </p>`). Assert strictly so a future whitespace leak fails.
        assert_eq!(render_comment_body("   "), "");
    }

    #[test]
    fn status_directive_in_comment_body_is_literal() {
        // Comment bodies render a restricted subset with directives OFF; a
        // status-shaped string renders verbatim, not interpreted/stripped.
        let html = render_comment_body("Use :status[Done]{color=green} here.");
        assert!(html.contains(":status[Done]{color=green}"), "got: {html}");
        assert!(!html.contains("status-green"), "got: {html}");
    }
}
