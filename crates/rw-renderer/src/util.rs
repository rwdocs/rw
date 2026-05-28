//! Shared utility functions for markdown rendering.

use pulldown_cmark::HeadingLevel;

/// Convert heading level enum to number (1-6).
#[must_use]
pub(crate) fn heading_level_to_num(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Convert text to URL-safe slug.
///
/// Converts to lowercase, replaces whitespace/dashes/underscores with single dashes,
/// and removes other non-alphanumeric characters. Preserves non-Latin Unicode characters
/// (Cyrillic, CJK, etc.) following GitHub-style heading ID generation.
#[must_use]
pub(crate) fn slugify(text: &str) -> String {
    let mut result = String::new();
    let mut last_was_dash = true; // Prevents leading dash

    for c in text.trim().chars() {
        if c.is_alphanumeric() {
            for lc in c.to_lowercase() {
                result.push(lc);
            }
            last_was_dash = false;
        } else if !last_was_dash && (c.is_whitespace() || c == '-' || c == '_') {
            result.push('-');
            last_was_dash = true;
        }
    }

    // Remove trailing dash if present
    if result.ends_with('-') {
        result.pop();
    }

    result
}

/// Escapes the five HTML special characters (`&`, `<`, `>`, `"`, `'`).
///
/// # Examples
///
/// ```
/// use rw_renderer::escape_html;
///
/// assert_eq!(escape_html("<script>"), "&lt;script&gt;");
/// assert_eq!(escape_html(r#"a "b" & 'c'"#), "a &quot;b&quot; &amp; &#x27;c&#x27;");
/// ```
#[must_use]
pub fn escape_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#x27;"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("What's New?"), "whats-new");
        assert_eq!(slugify("  Spaces  "), "spaces");
        assert_eq!(slugify("Multiple   Spaces"), "multiple-spaces");
        assert_eq!(slugify("kebab-case"), "kebab-case");
        assert_eq!(slugify("snake_case"), "snake-case");
    }

    #[test]
    fn test_slugify_non_latin() {
        // Cyrillic
        assert_eq!(slugify("Привет мир"), "привет-мир");
        // Chinese
        assert_eq!(slugify("你好世界"), "你好世界");
        // Japanese
        assert_eq!(slugify("こんにちは世界"), "こんにちは世界");
        // Mixed Latin and non-Latin
        assert_eq!(slugify("Hello Привет"), "hello-привет");
        // Non-Latin with punctuation
        assert_eq!(slugify("Привет, мир!"), "привет-мир");
        // Fully non-Latin should NOT produce empty string
        assert!(!slugify("Заголовок").is_empty());
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html(r#""quoted""#), "&quot;quoted&quot;");
        assert_eq!(escape_html("it's"), "it&#x27;s");
    }
}
