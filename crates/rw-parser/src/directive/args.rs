//! Directive argument parsing.
//!
//! Parses the `[content]{key="value"}` syntax from directives.

use super::line::Escapes;

/// Parsed arguments from directive syntax.
///
/// Represents the content and attributes extracted from a directive:
/// `:name[content]{key="value"}`. Any other token in the braces — `#id`,
/// `.class`, a bare word — is skipped whole.
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
    ///
    /// A `Vec` rather than a `HashMap`. Directives carry a handful of
    /// attributes at most, so hashing earns nothing at this size, iteration
    /// order is the author's instead of `RandomState`'s, and the struct is 24
    /// bytes smaller. Write through [`set`](Self::set) to preserve
    /// last-write-wins.
    pub(crate) attrs: Vec<(String, String)>,
}

impl DirectiveArgs {
    /// Parse content and attributes string into structured arguments.
    ///
    /// # Arguments
    ///
    /// * `content` - The content from brackets `[content]`
    /// * `attrs_str` - The attributes string from braces `{...}` (without braces)
    #[must_use]
    pub fn parse(content: &str, attrs_str: &str) -> Self {
        Self::parse_escaped(content, attrs_str, Escapes::default())
    }

    /// [`parse`](Self::parse) for attributes whose `escapes` are content.
    pub(crate) fn parse_escaped(content: &str, attrs_str: &str, escapes: Escapes<'_>) -> Self {
        let mut args = Self {
            content: content.to_owned(),
            attrs: Vec::new(),
        };

        let mut remaining = attrs_str.trim_start();
        while !remaining.is_empty() {
            let at = attrs_str.len() - remaining.len();
            if let Some((key, value, rest)) = parse_key_value(remaining, escapes.skip(at)) {
                args.set(key, value);
                remaining = rest;
            } else {
                // Skipping less than the whole token would let a bare word
                // become part of the next key: `{draft color=green}`.
                let end = remaining
                    .find(char::is_whitespace)
                    .unwrap_or(remaining.len());
                remaining = &remaining[end..];
            }
            remaining = remaining.trim_start();
        }

        args
    }

    /// Set an attribute, replacing any existing value for `key`.
    ///
    /// Reproduces `HashMap::insert`: a repeated key overwrites in place rather
    /// than appending a second entry, so the last occurrence wins.
    fn set(&mut self, key: &str, value: &str) {
        if let Some(entry) = self.attrs.iter_mut().find(|(k, _)| k == key) {
            value.clone_into(&mut entry.1);
        } else {
            self.attrs.push((key.to_owned(), value.to_owned()));
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

/// Parse a key-value pair from the attributes string.
///
/// Supports: `key="value"`, `key='value'`, `key=value`
fn parse_key_value<'s>(s: &'s str, escapes: Escapes<'_>) -> Option<(&'s str, &'s str, &'s str)> {
    let key_end = s.find(|c: char| c == '=' || c.is_whitespace())?;
    let key = &s[..key_end];
    let after_eq = s[key_end..].strip_prefix('=')?;

    if key.is_empty() || key.starts_with('#') || key.starts_with('.') {
        return None;
    }

    let value_at = key_end + 1;
    let quote = after_eq
        .chars()
        .next()
        .filter(|&c| (c == '"' || c == '\'') && !escapes.contains(value_at));
    if let Some(quote) = quote {
        let end_quote = after_eq
            .match_indices(quote)
            .map(|(i, _)| i)
            .skip(1)
            .find(|&i| !escapes.contains(value_at + i))?;
        Some((key, &after_eq[1..end_quote], &after_eq[end_quote + 1..]))
    } else {
        // Unquoted value (until whitespace)
        let end = after_eq.find(char::is_whitespace).unwrap_or(after_eq.len());
        let value = &after_eq[..end];
        let rest = &after_eq[end..];
        Some((key, value, rest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(args: &DirectiveArgs) -> Vec<(&str, &str)> {
        args.attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    #[test]
    fn test_empty_args() {
        let args = DirectiveArgs::parse("", "");
        assert_eq!(args, DirectiveArgs::default());
    }

    #[test]
    fn test_content_only() {
        let args = DirectiveArgs::parse("hello world", "");
        assert_eq!(args.content, "hello world");
        assert_eq!(attrs(&args), []);
    }

    #[test]
    fn test_double_quoted_value() {
        let args = DirectiveArgs::parse("", r#"lang="en""#);
        assert_eq!(args.get("lang"), Some("en"));
    }

    #[test]
    fn test_single_quoted_value() {
        let args = DirectiveArgs::parse("", "title='Hello World'");
        assert_eq!(args.get("title"), Some("Hello World"));
    }

    #[test]
    fn test_unquoted_value() {
        let args = DirectiveArgs::parse("", "width=560");
        assert_eq!(args.get("width"), Some("560"));
    }

    #[test]
    fn test_id_and_classes_are_skipped() {
        let args = DirectiveArgs::parse("content", r#"#my-id .foo.bar lang="en" width=100"#);
        assert_eq!(args.content, "content");
        assert_eq!(attrs(&args), [("lang", "en"), ("width", "100")]);
    }

    #[test]
    fn test_attrs_keep_author_order() {
        let args = DirectiveArgs::parse("", "b=2 a=1");
        assert_eq!(attrs(&args), [("b", "2"), ("a", "1")]);
    }

    #[test]
    fn test_empty_quoted_value() {
        let args = DirectiveArgs::parse("", r#"alt="""#);
        assert_eq!(args.get("alt"), Some(""));
    }

    #[test]
    fn test_get_nonexistent() {
        let args = DirectiveArgs::parse("", "foo=bar");
        assert_eq!(args.get("baz"), None);
    }

    /// `HashMap::insert` semantics that a `Vec` representation must preserve —
    /// the length assertion is what a naive `push` would break.
    #[test]
    fn test_duplicate_key_last_value_wins() {
        let args = DirectiveArgs::parse("", r#"lang="en" lang="fr""#);
        assert_eq!(attrs(&args), [("lang", "fr")]);
    }

    #[test]
    fn test_bare_word_does_not_join_the_next_key() {
        let args = DirectiveArgs::parse("", "draft color=green");
        assert_eq!(attrs(&args), [("color", "green")]);
    }

    #[test]
    fn test_unclosed_quote_skips_only_its_own_token() {
        let args = DirectiveArgs::parse("", r#"title="open other=x"#);
        assert_eq!(attrs(&args), [("other", "x")]);
    }

    #[test]
    fn test_non_ascii_bareword_does_not_panic() {
        // Each input covers a different UTF-8 codepoint width (2, 4, 2 bytes).
        for input in ["цвет", "🎉", "اختبار"] {
            let args = DirectiveArgs::parse("", input);
            assert_eq!(attrs(&args), []);
        }
    }

    #[test]
    fn test_non_ascii_bareword_then_valid_token() {
        let args = DirectiveArgs::parse("", "цвет k=v");
        assert_eq!(attrs(&args), [("k", "v")]);
    }
}
