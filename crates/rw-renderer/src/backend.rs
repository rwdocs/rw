//! Render backend trait for format-specific rendering.
//!
//! [`RenderBackend`] abstracts the differences between HTML and Confluence
//! output formats, letting [`MarkdownRenderer`](crate::MarkdownRenderer)
//! handle both with the same event-walking logic.

use std::borrow::Cow;
use std::fmt::Write;

use pulldown_cmark::{Alignment, BlockQuoteKind};

use crate::escape_html;

/// Alert variant for GitHub-style blockquotes (`> [!NOTE]`, `> [!TIP]`, etc.).
///
/// Converted from [`pulldown_cmark::BlockQuoteKind`] when the parser
/// encounters a blockquote with an alert marker. Passed to
/// [`RenderBackend::alert_start`] and [`RenderBackend::alert_end`] so
/// backends can render format-appropriate alert markup.
///
/// # Examples
///
/// Markdown alerts render as styled callout boxes:
///
/// ```
/// use rw_renderer::{MarkdownRenderer, HtmlBackend};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .render_markdown("> [!WARNING]\n> Do not delete this file.");
///
/// assert!(result.html.contains(r#"class="alert alert-warning""#));
/// assert!(result.html.contains("Do not delete this file."));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// Informational note — highlights something the reader should be aware of.
    Note,
    /// Helpful advice — suggests a better approach or useful trick.
    Tip,
    /// Critical information — something the reader must not overlook.
    Important,
    /// Potential issue — something that could go wrong.
    Warning,
    /// Dangerous action — something that could cause data loss or security issues.
    Caution,
}

impl From<BlockQuoteKind> for AlertKind {
    fn from(kind: BlockQuoteKind) -> Self {
        match kind {
            BlockQuoteKind::Note => AlertKind::Note,
            BlockQuoteKind::Tip => AlertKind::Tip,
            BlockQuoteKind::Important => AlertKind::Important,
            BlockQuoteKind::Warning => AlertKind::Warning,
            BlockQuoteKind::Caution => AlertKind::Caution,
        }
    }
}

/// Format-specific rendering operations.
///
/// [`MarkdownRenderer`](crate::MarkdownRenderer) delegates all output to this
/// trait, keeping only event walking and state management.
///
/// This crate ships [`HtmlBackend`](crate::HtmlBackend); other backends
/// (e.g., Confluence XHTML, plain text) can be implemented downstream.
/// All methods have default implementations that produce HTML5 output,
/// so backends only need to override the methods that differ.
///
/// # Design: static methods
///
/// All methods are static (no `&self` receiver) for zero-cost static dispatch.
/// Backend configuration that varies per call site (e.g., a base path for link
/// resolution) is passed via method parameters. If a future backend needs
/// per-instance state, the trait signature will need to change.
pub trait RenderBackend {
    /// Controls first-H1 handling and heading level adjustment.
    ///
    /// - `true` (Confluence): first H1 is extracted as page title and
    ///   suppressed from output; all subsequent headings shift up one level
    ///   (H2 → H1, H3 → H2, etc.).
    /// - `false` (HTML): first H1 renders normally, no level shifting.
    const TITLE_AS_METADATA: bool;

    /// Writes a fenced code block to `out`.
    ///
    /// `lang` is the language identifier from the fence info string (e.g.,
    /// `"rust"`, `"python"`), or `None` for plain code blocks.
    fn code_block(lang: Option<&str>, content: &str, out: &mut String);

    /// Writes the opening tag for a blockquote.
    fn blockquote_start(out: &mut String);

    /// Writes the closing tag for a blockquote.
    fn blockquote_end(out: &mut String);

    /// Writes the opening markup for a GitHub-style alert.
    fn alert_start(kind: AlertKind, out: &mut String);

    /// Writes the closing markup for a GitHub-style alert.
    fn alert_end(kind: AlertKind, out: &mut String);

    /// Writes an image element. `title` is empty when no title attribute is present.
    fn image(src: &str, alt: &str, title: &str, out: &mut String);

    /// Transforms a link URL before it is written to output.
    ///
    /// The default implementation returns the URL unchanged.
    /// [`HtmlBackend`](crate::HtmlBackend) overrides this to resolve relative
    /// `.md` links against `base_path`.
    #[must_use]
    fn transform_link<'a>(url: &'a str, _base_path: Option<&str>) -> Cow<'a, str> {
        Cow::Borrowed(url)
    }

    /// Writes a hard line break. Default: `<br>`.
    fn hard_break(out: &mut String) {
        out.push_str("<br>");
    }

    /// Writes a horizontal rule. Default: `<hr>`.
    fn horizontal_rule(out: &mut String) {
        out.push_str("<hr>");
    }

    /// Writes a task-list checkbox. Default: HTML `<input type="checkbox">`.
    fn task_list_marker(checked: bool, out: &mut String) {
        if checked {
            out.push_str(r#"<input type="checkbox" checked disabled> "#);
        } else {
            out.push_str(r#"<input type="checkbox" disabled> "#);
        }
    }

    /// Writes the opening tag for a paragraph.
    fn paragraph_start(out: &mut String) {
        out.push_str("<p>");
    }

    /// Writes the closing tag for a paragraph.
    fn paragraph_end(out: &mut String) {
        out.push_str("</p>");
    }

    /// Writes a heading opening tag with ID.
    ///
    /// Called after the heading text has been fully collected (from `end_tag`),
    /// so the ID is already computed. The heading content follows via
    /// subsequent `text()`, `inline_code()`, etc. calls, then [`heading_end`](Self::heading_end).
    fn heading_start(level: u8, id: &str, out: &mut String) {
        write!(out, r#"<h{level} id="{id}">"#).unwrap();
    }

    /// Writes a heading closing tag.
    fn heading_end(level: u8, out: &mut String) {
        write!(out, "</h{level}>").unwrap();
    }

    /// Writes the opening tag for a list.
    fn list_start(ordered: bool, start: Option<u64>, out: &mut String) {
        match (ordered, start) {
            (true, Some(1) | None) => out.push_str("<ol>"),
            (true, Some(n)) => write!(out, r#"<ol start="{n}">"#).unwrap(),
            (false, _) => out.push_str("<ul>"),
        }
    }

    /// Writes the closing tag for a list.
    fn list_end(ordered: bool, out: &mut String) {
        out.push_str(if ordered { "</ol>" } else { "</ul>" });
    }

    /// Writes the opening tag for a list item.
    fn list_item_start(out: &mut String) {
        out.push_str("<li>");
    }

    /// Writes the closing tag for a list item.
    fn list_item_end(out: &mut String) {
        out.push_str("</li>");
    }

    /// Writes the opening tag for a definition list.
    fn definition_list_start(out: &mut String) {
        out.push_str("<dl>");
    }

    /// Writes the closing tag for a definition list.
    fn definition_list_end(out: &mut String) {
        out.push_str("</dl>");
    }

    /// Writes the opening tag for a definition title.
    fn definition_title_start(out: &mut String) {
        out.push_str("<dt>");
    }

    /// Writes the closing tag for a definition title.
    fn definition_title_end(out: &mut String) {
        out.push_str("</dt>");
    }

    /// Writes the opening tag for a definition detail.
    fn definition_detail_start(out: &mut String) {
        out.push_str("<dd>");
    }

    /// Writes the closing tag for a definition detail.
    fn definition_detail_end(out: &mut String) {
        out.push_str("</dd>");
    }

    /// Writes the opening tag for a table.
    fn table_start(out: &mut String) {
        out.push_str("<table>");
    }

    /// Writes the closing tags for a table (including `</tbody>`).
    ///
    /// The default combines `</tbody>` and `</table>` because `<tbody>` is
    /// opened implicitly by [`table_head_end`](Self::table_head_end). Backends
    /// that override `table_head_end` should keep this in sync.
    fn table_end(out: &mut String) {
        out.push_str("</tbody></table>");
    }

    /// Writes the opening tags for the table header row.
    fn table_head_start(out: &mut String) {
        out.push_str("<thead><tr>");
    }

    /// Writes the closing tags for the table header, starting the body.
    fn table_head_end(out: &mut String) {
        out.push_str("</tr></thead><tbody>");
    }

    /// Writes the opening tag for a table row.
    fn table_row_start(out: &mut String) {
        out.push_str("<tr>");
    }

    /// Writes the closing tag for a table row.
    fn table_row_end(out: &mut String) {
        out.push_str("</tr>");
    }

    /// Writes the opening tag for a table cell.
    ///
    /// `alignment` is `None` for default alignment.
    fn table_cell_start(is_head: bool, alignment: Option<Alignment>, out: &mut String) {
        let tag = if is_head { "th" } else { "td" };
        let align = match alignment {
            Some(Alignment::Left) => r#" style="text-align:left""#,
            Some(Alignment::Center) => r#" style="text-align:center""#,
            Some(Alignment::Right) => r#" style="text-align:right""#,
            Some(Alignment::None) | None => "",
        };
        write!(out, "<{tag}{align}>").unwrap();
    }

    /// Writes the closing tag for a table cell.
    fn table_cell_end(is_head: bool, out: &mut String) {
        out.push_str(if is_head { "</th>" } else { "</td>" });
    }

    /// Writes a soft line break.
    fn soft_break(out: &mut String) {
        out.push('\n');
    }

    /// Writes the opening tag for emphasis.
    fn emphasis_start(out: &mut String) {
        out.push_str("<em>");
    }

    /// Writes the closing tag for emphasis.
    fn emphasis_end(out: &mut String) {
        out.push_str("</em>");
    }

    /// Writes the opening tag for strong emphasis.
    fn strong_start(out: &mut String) {
        out.push_str("<strong>");
    }

    /// Writes the closing tag for strong emphasis.
    fn strong_end(out: &mut String) {
        out.push_str("</strong>");
    }

    /// Writes the opening tag for strikethrough text.
    fn strikethrough_start(out: &mut String) {
        out.push_str("<s>");
    }

    /// Writes the closing tag for strikethrough text.
    fn strikethrough_end(out: &mut String) {
        out.push_str("</s>");
    }

    /// Writes the opening tag for superscript.
    fn superscript_start(out: &mut String) {
        out.push_str("<sup>");
    }

    /// Writes the closing tag for superscript.
    fn superscript_end(out: &mut String) {
        out.push_str("</sup>");
    }

    /// Writes the opening tag for subscript.
    fn subscript_start(out: &mut String) {
        out.push_str("<sub>");
    }

    /// Writes the closing tag for subscript.
    fn subscript_end(out: &mut String) {
        out.push_str("</sub>");
    }

    /// Writes inline code.
    fn inline_code(code: &str, out: &mut String) {
        write!(out, "<code>{}</code>", escape_html(code)).unwrap();
    }

    /// Writes the opening tag for a link.
    ///
    /// `section_ref` contains `(ref_string, section_path)` for cross-section
    /// links. The default adds `data-section-ref` and `data-section-path` HTML
    /// attributes; non-HTML backends can ignore this parameter.
    fn link_start(href: &str, section_ref: Option<(&str, &str)>, out: &mut String) {
        write!(out, r#"<a href="{}""#, escape_html(href)).unwrap();
        if let Some((ref_string, section_path)) = section_ref {
            write!(out, r#" data-section-ref="{}""#, escape_html(ref_string)).unwrap();
            if !section_path.is_empty() {
                write!(out, r#" data-section-path="{}""#, escape_html(section_path)).unwrap();
            }
        }
        out.push('>');
    }

    /// Writes the closing tag for a link.
    fn link_end(out: &mut String) {
        out.push_str("</a>");
    }

    /// Writes the opening tag for a broken link (unresolved wikilink).
    ///
    /// The default renders an HTML anchor with `class="rw-broken-link"`.
    /// Non-HTML backends should override this to produce appropriate output
    /// (e.g., strikethrough text, a warning marker, or plain text).
    fn broken_link_start(out: &mut String) {
        out.push_str(r##"<a href="#" class="rw-broken-link">"##);
    }

    /// Writes a text node. Default: HTML-escapes the text.
    fn text(text: &str, out: &mut String) {
        out.push_str(&escape_html(text));
    }

    /// Writes raw HTML content. Default: passes through unchanged.
    fn raw_html(html: &str, out: &mut String) {
        out.push_str(html);
    }
}
