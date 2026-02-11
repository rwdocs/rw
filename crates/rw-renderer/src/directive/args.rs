//! Directive argument parsing.
//!
//! Parses the `[content]{#id .class key="value"}` syntax from directives.

use std::collections::HashMap;

/// Parsed arguments from directive syntax.
///
/// Represents the content and attributes extracted from a directive:
/// `:name[content]{#id .class key="value"}`
///
/// # Example
///
/// ```
/// use rw_renderer::directive::DirectiveArgs;
///
/// let args = DirectiveArgs::parse("hello", r#"#my-id .foo .bar lang="en""#);
/// assert_eq!(args.content, "hello");
/// assert_eq!(args.id, Some("my-id".to_string()));
/// assert_eq!(args.classes, vec!["foo", "bar"]);
/// assert_eq!(args.get("lang"), Some("en"));
/// ```
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DirectiveArgs {
    /// Content from brackets: `[content]` (empty string if not provided).
    pub content: String,
    /// ID from attributes: `{#id}`.
    pub id: Option<String>,
    /// Classes from attributes: `{.class1 .class2}`.
    pub classes: Vec<String>,
    /// Key-value attributes: `{key="value"}`.
    pub attrs: HashMap<String, String>,
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
        let mut args = Self {
            content: content.to_owned(),
            ..Default::default()
        };

        if attrs_str.is_empty() {
            return args;
        }

        // Parse attributes: #id, .class, key="value", key='value', or key=value
        let mut remaining = attrs_str.trim();

        while !remaining.is_empty() {
            remaining = remaining.trim_start();

            if remaining.starts_with('#') {
                // ID: #my-id
                let end = remaining[1..]
                    .find(|c: char| c.is_whitespace() || c == '.' || c == '#')
                    .map_or(remaining.len(), |i| i + 1);
                args.id = Some(remaining[1..end].to_string());
                remaining = &remaining[end..];
            } else if remaining.starts_with('.') {
                // Class: .my-class
                let end = remaining[1..]
                    .find(|c: char| c.is_whitespace() || c == '.' || c == '#')
                    .map_or(remaining.len(), |i| i + 1);
                args.classes.push(remaining[1..end].to_string());
                remaining = &remaining[end..];
            } else if let Some((key, value, rest)) = parse_key_value(remaining) {
                // Key-value: key="value" or key='value' or key=value
                args.attrs.insert(key.to_owned(), value.to_owned());
                remaining = rest;
            } else {
                // Skip unrecognized character
                remaining = &remaining[1..];
            }
        }

        args
    }

    /// Get an attribute value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }

    /// Reconstruct the original syntax string `[content]{attrs}`.
    ///
    /// Used for pass-through when a directive is not handled.
    ///
    /// # Example
    ///
    /// ```
    /// use rw_renderer::directive::DirectiveArgs;
    ///
    /// let args = DirectiveArgs::parse("hello", r#"#my-id .foo lang="en""#);
    /// let syntax = args.to_syntax();
    /// assert!(syntax.starts_with("[hello]"));
    /// assert!(syntax.contains("#my-id"));
    /// ```
    #[must_use]
    pub fn to_syntax(&self) -> String {
        let mut result = String::new();

        // Add content in brackets (always include brackets if content is non-empty)
        if !self.content.is_empty() {
            result.push('[');
            result.push_str(&self.content);
            result.push(']');
        }

        // Build attributes string
        let mut attrs_parts = Vec::new();

        if let Some(id) = &self.id {
            attrs_parts.push(format!("#{id}"));
        }

        for class in &self.classes {
            attrs_parts.push(format!(".{class}"));
        }

        // Sort keys for deterministic output in tests
        let mut keys: Vec<_> = self.attrs.keys().collect();
        keys.sort();
        for key in keys {
            let value = &self.attrs[key];
            // Use double quotes and escape internal quotes if needed
            let escaped = value.replace('"', r#"\""#);
            attrs_parts.push(format!(r#"{key}="{escaped}""#));
        }

        if !attrs_parts.is_empty() {
            result.push('{');
            result.push_str(&attrs_parts.join(" "));
            result.push('}');
        }

        result
    }
}

/// Parse a key-value pair from the attributes string.
///
/// Supports: `key="value"`, `key='value'`, `key=value`
fn parse_key_value(s: &str) -> Option<(&str, &str, &str)> {
    let eq_pos = s.find('=')?;
    let key = s[..eq_pos].trim();

    if key.is_empty() || key.starts_with('#') || key.starts_with('.') {
        return None;
    }

    let after_eq = &s[eq_pos + 1..];

    if let Some(stripped) = after_eq.strip_prefix('"') {
        // Quoted with double quotes
        let end_quote = stripped.find('"')?;
        let value = &stripped[..end_quote];
        let rest = &stripped[end_quote + 1..];
        Some((key, value, rest))
    } else if let Some(stripped) = after_eq.strip_prefix('\'') {
        // Quoted with single quotes
        let end_quote = stripped.find('\'')?;
        let value = &stripped[..end_quote];
        let rest = &stripped[end_quote + 1..];
        Some((key, value, rest))
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

    #[test]
    fn test_empty_args() {
        let args = DirectiveArgs::parse("", "");
        assert_eq!(args.content, "");
        assert_eq!(args.id, None);
        assert!(args.classes.is_empty());
        assert!(args.attrs.is_empty());
    }

    #[test]
    fn test_content_only() {
        let args = DirectiveArgs::parse("hello world", "");
        assert_eq!(args.content, "hello world");
        assert_eq!(args.id, None);
        assert!(args.classes.is_empty());
    }

    #[test]
    fn test_id() {
        let args = DirectiveArgs::parse("", "#my-id");
        assert_eq!(args.id, Some("my-id".to_owned()));
    }

    #[test]
    fn test_single_class() {
        let args = DirectiveArgs::parse("", ".foo");
        assert_eq!(args.classes, vec!["foo"]);
    }

    #[test]
    fn test_multiple_classes() {
        let args = DirectiveArgs::parse("", ".foo .bar .baz");
        assert_eq!(args.classes, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn test_id_and_classes() {
        let args = DirectiveArgs::parse("", "#my-id .foo .bar");
        assert_eq!(args.id, Some("my-id".to_owned()));
        assert_eq!(args.classes, vec!["foo", "bar"]);
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
    fn test_mixed_attributes() {
        let args = DirectiveArgs::parse("content", r#"#my-id .foo lang="en" width=100"#);
        assert_eq!(args.content, "content");
        assert_eq!(args.id, Some("my-id".to_owned()));
        assert_eq!(args.classes, vec!["foo"]);
        assert_eq!(args.get("lang"), Some("en"));
        assert_eq!(args.get("width"), Some("100"));
    }

    #[test]
    fn test_compact_classes() {
        let args = DirectiveArgs::parse("", ".foo.bar.baz");
        assert_eq!(args.classes, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn test_id_followed_by_class() {
        let args = DirectiveArgs::parse("", "#id.class");
        assert_eq!(args.id, Some("id".to_owned()));
        assert_eq!(args.classes, vec!["class"]);
    }

    #[test]
    fn test_value_with_spaces() {
        let args = DirectiveArgs::parse("", r#"title="Hello World""#);
        assert_eq!(args.get("title"), Some("Hello World"));
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

    #[test]
    fn test_to_syntax_empty() {
        let args = DirectiveArgs::default();
        assert_eq!(args.to_syntax(), "");
    }

    #[test]
    fn test_to_syntax_content_only() {
        let args = DirectiveArgs::parse("hello", "");
        assert_eq!(args.to_syntax(), "[hello]");
    }

    #[test]
    fn test_to_syntax_with_id() {
        let args = DirectiveArgs::parse("content", "#my-id");
        assert_eq!(args.to_syntax(), "[content]{#my-id}");
    }

    #[test]
    fn test_to_syntax_with_classes() {
        let args = DirectiveArgs::parse("content", ".foo .bar");
        assert_eq!(args.to_syntax(), "[content]{.foo .bar}");
    }

    #[test]
    fn test_to_syntax_with_attrs() {
        let args = DirectiveArgs::parse("content", r#"lang="en""#);
        assert_eq!(args.to_syntax(), r#"[content]{lang="en"}"#);
    }

    #[test]
    fn test_to_syntax_full() {
        let args = DirectiveArgs::parse("content", r#"#id .class lang="en""#);
        let syntax = args.to_syntax();
        assert!(syntax.starts_with("[content]{"));
        assert!(syntax.contains("#id"));
        assert!(syntax.contains(".class"));
        assert!(syntax.contains(r#"lang="en""#));
        assert!(syntax.ends_with('}'));
    }

    #[test]
    fn test_to_syntax_attrs_only() {
        let args = DirectiveArgs::parse("", "#my-id");
        assert_eq!(args.to_syntax(), "{#my-id}");
    }
}
