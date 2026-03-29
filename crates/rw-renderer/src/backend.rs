//! Render backend trait for format-specific rendering.
//!
//! [`RenderBackend`] abstracts the differences between HTML and Confluence
//! output formats, letting [`MarkdownRenderer`](crate::MarkdownRenderer)
//! handle both with the same event-walking logic.

use std::borrow::Cow;

use pulldown_cmark::BlockQuoteKind;

/// Alert variant for GitHub-style blockquotes (`> [!NOTE]`, `> [!TIP]`, etc.).
///
/// Converted from [`pulldown_cmark::BlockQuoteKind`] when the parser
/// encounters a blockquote with an alert marker.
///
/// # Examples
///
/// ```
/// use rw_renderer::AlertKind;
///
/// let kind = AlertKind::Warning;
/// assert_eq!(kind, AlertKind::Warning);
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
/// [`MarkdownRenderer`](crate::MarkdownRenderer) calls these methods when it
/// encounters elements that differ between output formats:
///
/// This crate ships [`HtmlBackend`](crate::HtmlBackend); other backends
/// (e.g., Confluence XHTML) can be implemented downstream.
///
/// Methods cover the elements that differ between output formats: code blocks,
/// blockquotes, alerts, images, link transformation, and line breaks.
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
}
