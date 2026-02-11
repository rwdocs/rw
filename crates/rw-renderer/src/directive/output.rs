//! Directive output types.
//!
//! Defines the output variants that directive handlers can return.

/// Output from directive processing.
///
/// Directives can produce three types of output:
///
/// - [`Html`](Self::Html): HTML that passes through pulldown-cmark unchanged
/// - [`Markdown`](Self::Markdown): Markdown that needs recursive processing (for `::include`)
/// - [`Skip`](Self::Skip): Pass through the directive unchanged
///
/// # Example
///
/// ```
/// use rw_renderer::directive::DirectiveOutput;
///
/// // Return HTML
/// let output = DirectiveOutput::html("<kbd>Ctrl+C</kbd>");
///
/// // Return markdown for recursive processing
/// let output = DirectiveOutput::markdown("# Included Content\n\nSome text.");
///
/// // Skip handling (pass through unchanged)
/// let output = DirectiveOutput::Skip;
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirectiveOutput {
    /// HTML that passes through pulldown-cmark unchanged.
    Html(String),
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
        assert!(matches!(output, DirectiveOutput::Html(_)));
    }
}
