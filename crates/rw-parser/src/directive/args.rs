//! Directive argument parsing.
//!
//! Parses the `[content]{key="value"}` syntax from directives.

use super::line::Escapes;
use crate::attrs::parse_attrs;

/// Parsed arguments from directive syntax.
///
/// Represents the content and attributes extracted from a directive:
/// `:name[content]{key="value"}`. The braces hold the same block as a fence's
/// [`Attrs`](crate::Attrs), less its `#id`.
///
/// # Example
///
/// ```
/// use rw_parser::DirectiveArgs;
///
/// let args = DirectiveArgs::parse("hello", r#"#my-id .foo .bar lang="en""#);
/// assert_eq!(args.content(), "hello");
/// assert_eq!(args.get("lang"), Some("en"));
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DirectiveArgs {
    /// Content from brackets: `[content]` (empty string if not provided).
    pub(crate) content: String,
    /// Key-value attributes: `{key="value"}`, in source order.
    pub(crate) attrs: Vec<(String, String)>,
}

impl DirectiveArgs {
    /// Parse the `[content]` text and the `{...}` attributes string, braces
    /// excluded.
    #[must_use]
    pub fn parse(content: &str, attrs_str: &str) -> Self {
        Self::parse_escaped(content, attrs_str, Escapes::default())
    }

    /// [`parse`](Self::parse) for attributes whose `escapes` are content.
    pub(crate) fn parse_escaped(content: &str, attrs_str: &str, escapes: Escapes<'_>) -> Self {
        Self {
            content: content.to_owned(),
            attrs: parse_attrs(attrs_str, escapes).map,
        }
    }

    /// Get the content from brackets: `[content]`.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get an attribute value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_args() {
        let args = DirectiveArgs::parse("", "");
        assert_eq!(args, DirectiveArgs::default());
    }

    #[test]
    fn test_content_only() {
        let args = DirectiveArgs::parse("hello world", "");
        assert_eq!(args.content, "hello world");
        assert_eq!(args.attrs, []);
    }

    #[test]
    fn test_id_is_dropped() {
        let args = DirectiveArgs::parse("", "#my-id k=v");
        assert_eq!(args.get("k"), Some("v"));
        assert_eq!(args.attrs.len(), 1);
    }

    #[test]
    fn test_get_nonexistent() {
        let args = DirectiveArgs::parse("", "foo=bar");
        assert_eq!(args.get("baz"), None);
    }
}
