//! Directive output types.
//!
//! Defines the output variants that directive handlers can return.

use super::Marker;

/// Output from directive processing.
///
/// Directives can produce four kinds of output:
///
/// - [`Html`](Self::Html): a single HTML blob passed verbatim to the backend's `raw_html`.
/// - [`Marker`](Self::Marker): a semantic [`Marker`](super::Marker) wrapping a body of text.
///   The renderer emits it as three separate backend calls
///   (`marker_open + text(body) + marker_close`), so each backend decides what the marker
///   looks like — Confluence emits a native macro where HTML emits a `<span>`. Use this for
///   any inline directive that wraps a label.
/// - [`Markdown`](Self::Markdown): markdown that needs recursive processing (used by `::include`).
/// - [`Skip`](Self::Skip): the handler declines; the original directive syntax is preserved.
///
/// # Example
///
/// ```
/// use rw_renderer::directive::{DirectiveOutput, Marker};
///
/// // Single HTML blob.
/// let _ = DirectiveOutput::html("<kbd>Ctrl+C</kbd>");
///
/// // Semantic marker — each backend renders it its own way.
/// let _ = DirectiveOutput::marker(Marker::new("status").with_attr("color", "green"), "On Track");
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
    /// A semantic marker wrapping body text. The renderer emits it as three separate
    /// backend calls (`marker_open + text(body) + marker_close`).
    Marker {
        /// The semantic marker — backends dispatch on its name and read its attributes.
        marker: Marker,
        /// Body text — emitted via `text` (HTML-escaped by the backend).
        body: String,
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

    /// Create a semantic marker wrapping body text.
    ///
    /// The renderer emits it as three separate backend calls so each backend
    /// renders the marker its own way while the body flows through `text`.
    ///
    /// # Example
    ///
    /// ```
    /// use rw_renderer::directive::{DirectiveOutput, Marker};
    ///
    /// let output = DirectiveOutput::marker(
    ///     Marker::new("status").with_attr("color", "green"),
    ///     "On Track",
    /// );
    /// assert!(matches!(output, DirectiveOutput::Marker { .. }));
    /// ```
    #[must_use]
    pub fn marker(marker: Marker, body: impl Into<String>) -> Self {
        Self::Marker {
            marker,
            body: body.into(),
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
