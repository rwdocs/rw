//! Render backend trait for format-specific rendering.
//!
//! [`RenderBackend`] abstracts the differences between HTML and Confluence
//! output formats, letting [`MarkdownRenderer`](crate::MarkdownRenderer)
//! handle both with the same event-walking logic.

use std::fmt::Write;

use crate::diagram::{DiagramView, write_diagram_id_attr};
use crate::link::write_section_attrs;
use crate::status::StatusColor;
use crate::tabs::TabInfo;

use crate::escape_into;
use rw_parser::{AlertKind, Alignment};

/// Format-specific rendering operations.
///
/// [`MarkdownRenderer`](crate::MarkdownRenderer) delegates all output to this
/// trait, keeping only event walking and state management.
///
/// Most markup methods default to HTML5; a backend overrides only the methods
/// that differ. [`image`](Self::image), [`code_block`](Self::code_block),
/// [`diagram`](Self::diagram) and the alert methods have no default: a wrong
/// default is worse than a compile error.
///
/// All methods are static; per-call settings arrive as parameters.
pub trait RenderBackend {
    /// Whether the parser tokenizes rw directive syntax (`:name`, `::name`,
    /// `:::name`) for this backend. Defaults to `true`.
    ///
    /// Set it `false` for untrusted user text, where `:status[Done]` must
    /// render as typed.
    const TOKENIZE_DIRECTIVES: bool = true;

    /// Whether this backend puts a `data-diagram-id` on each diagram.
    ///
    /// When `false` the renderer does not resolve diagram ids, so no
    /// duplicate-id warnings are raised.
    const DIAGRAM_IDS: bool = true;

    /// Writes a fenced code block to `out`.
    ///
    /// `lang` is the language identifier from the fence info string (e.g.,
    /// `"rust"`, `"python"`), or `None` for plain code blocks.
    fn code_block(lang: Option<&str>, content: &str, out: &mut String);

    /// Writes the non-empty `{alt=…}` of the fence that
    /// [`code_block`](Self::code_block) writes next. Default: nothing.
    fn code_block_alt(_alt: &str, _out: &mut String) {}

    /// Writes one resolved diagram.
    fn diagram(view: &DiagramView<'_>, out: &mut String);

    /// Writes a diagram that could not be rendered.
    fn diagram_error(id: Option<&str>, message: &str, out: &mut String) {
        out.push_str(r#"<figure class="diagram diagram-error""#);
        write_diagram_id_attr(id, out);
        out.push_str("><pre>Diagram rendering failed: ");
        escape_into(message, out);
        out.push_str("</pre></figure>");
    }

    /// Writes the opening tag for a blockquote.
    fn blockquote_start(out: &mut String) {
        out.push_str("<blockquote>");
    }

    /// Writes the closing tag for a blockquote.
    fn blockquote_end(out: &mut String) {
        out.push_str("</blockquote>");
    }

    /// Writes the opening markup for a GitHub-style alert.
    fn alert_start(kind: AlertKind, out: &mut String);

    /// Writes the closing markup for a GitHub-style alert.
    fn alert_end(out: &mut String);

    /// Writes an image element. `title` is empty when no title attribute is present.
    fn image(src: &str, alt: &str, title: &str, out: &mut String);

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
    /// Called once the heading's content is rendered, so `id` is known. The
    /// content follows, then [`heading_end`](Self::heading_end).
    fn heading_start(level: u8, id: &str, out: &mut String) {
        write!(out, r#"<h{level} id="{id}">"#).unwrap();
    }

    /// Writes a heading closing tag.
    fn heading_end(level: u8, out: &mut String) {
        write!(out, "</h{level}>").unwrap();
    }

    /// Writes the opening tag for a list: `start` is `Some(n)` for an ordered
    /// list starting at `n`, `None` for an unordered one.
    fn list_start(start: Option<u64>, out: &mut String) {
        match start {
            Some(1) => out.push_str("<ol>"),
            Some(n) => write!(out, r#"<ol start="{n}">"#).unwrap(),
            None => out.push_str("<ul>"),
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
    /// `<tbody>` is opened by [`table_head_end`](Self::table_head_end); keep
    /// the two in sync.
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
        out.push_str("<code>");
        escape_into(code, out);
        out.push_str("</code>");
    }

    /// Writes the opening tag for a link.
    ///
    /// `section_ref` contains `(ref_string, section_path)` for cross-section
    /// links. The default adds `data-section-ref` and `data-section-path` HTML
    /// attributes; non-HTML backends can ignore this parameter.
    fn link_start(href: &str, section_ref: Option<(&str, &str)>, out: &mut String) {
        out.push_str(r#"<a href=""#);
        escape_into(href, out);
        out.push('"');
        if let Some((ref_string, section_path)) = section_ref {
            write_section_attrs(ref_string, section_path, out);
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
    fn broken_link_start(out: &mut String) {
        out.push_str(r##"<a href="#" class="rw-broken-link">"##);
    }

    /// Writes a text node. Default: HTML-escapes the text.
    fn text(text: &str, out: &mut String) {
        escape_into(text, out);
    }

    /// Writes raw HTML content. Default: passes through unchanged.
    fn raw_html(html: &str, out: &mut String) {
        out.push_str(html);
    }

    /// Opens a status badge wrapper; the walker writes the label via `text`.
    /// Default is a no-op, leaving the bare label.
    fn status_open(_color: StatusColor, _out: &mut String) {}

    /// Closes the status badge wrapper opened by [`status_open`](Self::status_open).
    fn status_close(_out: &mut String) {}

    /// Opens a tab group's bar. Called when the group closes, since the bar
    /// needs every tab's label, but lands before the panels. Default no-op.
    fn tabs_open(_group_id: usize, _tabs: &[TabInfo], _out: &mut String) {}

    /// Opens one tab panel, emitted inline at the panel's position. Default no-op.
    fn tab_panel_open(_group_id: usize, _tab: &TabInfo, _out: &mut String) {}

    /// Closes a tab panel opened by [`tab_panel_open`](Self::tab_panel_open).
    fn tab_panel_close(_out: &mut String) {}

    /// Closes the tab-group container opened (conceptually) by the bar. Default no-op.
    fn tabs_close(_out: &mut String) {}
}
