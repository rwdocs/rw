//! Directive output types.
//!
//! Defines the output variants that directive handlers can return.

/// Output from directive processing.
///
/// Directives can produce four kinds of output:
///
/// - [`Html`](Self::Html): a single HTML blob passed verbatim to the backend's `raw_html`.
/// - [`Marker`](Self::Marker): a structured open-tag / body / close-tag triple. The renderer
///   emits these as three separate backend calls (`raw_html(open) + text(body) + raw_html(close)`),
///   so a stateful backend (e.g. Confluence translating `<rw-status>` markers into native macros)
///   can pattern-match the opening and closing tags independently while still seeing the body
///   as text. Use this for any directive that wraps a label.
/// - [`Markdown`](Self::Markdown): markdown that needs recursive processing (used by `::include`).
/// - [`Skip`](Self::Skip): the handler declines; the original directive syntax is preserved.
///
/// # Example
///
/// ```
/// use rw_renderer::directive::DirectiveOutput;
///
/// // Single HTML blob.
/// let _ = DirectiveOutput::html("<kbd>Ctrl+C</kbd>");
///
/// // Marker pair — backend sees three discrete events.
/// let _ = DirectiveOutput::marker(r#"<rw-status data-color="green">"#, "On Track", "</rw-status>");
///
/// // Markdown for recursive processing.
/// let _ = DirectiveOutput::markdown("# Included Content\n\nSome text.");
///
/// // Skip handling (pass through unchanged).
/// let _ = DirectiveOutput::Skip;
/// ```
#[derive(Debug, PartialEq, Eq)]
pub enum DirectiveOutput {
    /// HTML that passes through to the backend as a single `raw_html` call.
    Html(String),
    /// A marker pair: opening tag, body text, closing tag. The renderer emits these as
    /// three separate backend calls (`raw_html(open) + text(body) + raw_html(close)`).
    Marker {
        /// Opening marker — emitted verbatim via `raw_html`.
        open: String,
        /// Body text — emitted via `text` (HTML-escaped by the backend).
        body: String,
        /// Closing marker — emitted verbatim via `raw_html`.
        close: String,
    },
    /// Markdown that needs to be processed through the full pipeline.
    ///
    /// Used by `::include` to inline file contents for recursive processing.
    Markdown(String),
    /// Don't handle this directive (pass through unchanged).
    Skip,
}

impl DirectiveOutput {
    /// Create an HTML output.
    ///
    /// # Example
    ///
    /// ```
    /// use rw_renderer::directive::DirectiveOutput;
    ///
    /// let output = DirectiveOutput::html("<strong>bold</strong>");
    /// assert!(matches!(output, DirectiveOutput::Html(_)));
    /// ```
    #[must_use]
    pub fn html(s: impl Into<String>) -> Self {
        Self::Html(s.into())
    }

    /// Create a marker triple — opening tag, body text, closing tag.
    ///
    /// The renderer emits these as three separate backend calls so a stateful
    /// backend can pattern-match the opening and closing tags while still
    /// seeing the body as text.
    ///
    /// # Example
    ///
    /// ```
    /// use rw_renderer::directive::DirectiveOutput;
    ///
    /// let output = DirectiveOutput::marker(
    ///     r#"<rw-status data-color="green">"#,
    ///     "On Track",
    ///     "</rw-status>",
    /// );
    /// assert!(matches!(output, DirectiveOutput::Marker { .. }));
    /// ```
    #[must_use]
    pub fn marker(
        open: impl Into<String>,
        body: impl Into<String>,
        close: impl Into<String>,
    ) -> Self {
        Self::Marker {
            open: open.into(),
            body: body.into(),
            close: close.into(),
        }
    }

    /// Create a markdown output (for includes).
    ///
    /// The returned markdown will be processed through the full pipeline,
    /// including directive expansion, pulldown-cmark parsing, and rendering.
    ///
    /// # Example
    ///
    /// ```
    /// use rw_renderer::directive::DirectiveOutput;
    ///
    /// let output = DirectiveOutput::markdown("# Title\n\nParagraph.");
    /// assert!(matches!(output, DirectiveOutput::Markdown(_)));
    /// ```
    #[must_use]
    pub fn markdown(s: impl Into<String>) -> Self {
        Self::Markdown(s.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    #[test]
    fn test_html() {
        let output = DirectiveOutput::html("<p>test</p>");
        assert_eq!(output, DirectiveOutput::Html("<p>test</p>".to_owned()));
    }

    #[test]
    fn test_markdown() {
        let output = DirectiveOutput::markdown("# Heading");
        assert_eq!(output, DirectiveOutput::Markdown("# Heading".to_owned()));
    }

    #[test]
    fn test_skip() {
        let output = DirectiveOutput::Skip;
        assert_eq!(output, DirectiveOutput::Skip);
    }

    #[test]
    fn test_html_from_string() {
        let s = String::from("<div>content</div>");
        let output = DirectiveOutput::html(s);
        assert_matches!(output, DirectiveOutput::Html(_));
    }
}
