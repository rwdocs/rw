//! Directive output types.
//!
//! Defines the output variants that directive handlers can return.

use super::Part;

/// Output from directive processing.
///
/// Directives can produce three kinds of output:
///
/// - [`Html`](Self::Html): a single HTML blob passed verbatim to the backend's `raw_html`.
/// - [`Deferred`](Self::Deferred): literal HTML interleaved with holes the handler fills after
///   the walk, via [`fills`](super::ContainerDirective::fills). Use this when the content depends
///   on material the walk has not reached yet — a tab bar needs every tab's label but is emitted
///   before the first tab.
/// - [`Skip`](Self::Skip): the handler declines; the original directive syntax is preserved.
///
/// # Example
///
/// ```
/// use rw_renderer::directive::{DirectiveOutput, Part};
///
/// // Single HTML blob.
/// let _ = DirectiveOutput::html("<kbd>Ctrl+C</kbd>");
///
/// // Literal HTML plus a hole filled after the walk.
/// let _ = DirectiveOutput::deferred(vec![Part::Hole(0), Part::Html("<p>body</p>".into())]);
///
/// // Skip handling (pass through unchanged).
/// let _ = DirectiveOutput::Skip;
/// ```
#[derive(Debug, PartialEq, Eq)]
pub enum DirectiveOutput {
    /// HTML that passes through to the backend as a single `raw_html` call.
    Html(String),
    /// Literal HTML interleaved with holes to be filled after the walk.
    ///
    /// Used by directives whose opening content depends on material that has
    /// not been walked yet — a tab bar needs every tab's label, but is emitted
    /// before the first tab.
    Deferred(Vec<Part>),
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

    /// Create a deferred output: literal HTML interleaved with holes.
    ///
    /// Each [`Part::Hole`] reserves a position in the output; the handler
    /// supplies its content afterwards from
    /// [`fills`](super::ContainerDirective::fills), keyed by the same value.
    ///
    /// # Example
    ///
    /// ```
    /// use rw_renderer::directive::{DirectiveOutput, Part};
    ///
    /// let output = DirectiveOutput::deferred(vec![Part::Hole(0)]);
    /// assert!(matches!(output, DirectiveOutput::Deferred(_)));
    /// ```
    #[must_use]
    pub fn deferred(parts: impl Into<Vec<Part>>) -> Self {
        Self::Deferred(parts.into())
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
