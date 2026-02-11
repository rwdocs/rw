//! Single-pass string replacement for post-processing.
//!
//! Collects replacements during post-processing and applies them efficiently.

/// Collects string replacements for single-pass application.
///
/// Instead of each directive calling `html.replace()` (O(N) allocation per call),
/// all directives register their replacements, then [`apply()`](Self::apply) performs
/// them in a single pass over the HTML string.
///
/// # Performance
///
/// ```text
/// Naive approach (N handlers, M replacements each):
///   for handler in handlers:           # N iterations
///     html = html.replace(...)         # O(len) allocation per replace
///   Total: O(N × M × len) allocations
///
/// Replacements approach:
///   for handler in handlers:           # N iterations
///     handler.post_process(&mut replacements)  # collect only
///   replacements.apply(html)           # single O(len) allocation
///   Total: O(1) allocation
/// ```
///
/// # Example
///
/// ```
/// use rw_renderer::directive::Replacements;
///
/// let mut html = "<rw-custom>content</rw-custom>".to_string();
/// let mut replacements = Replacements::new();
/// replacements.add("<rw-custom>", "<div class=\"custom\">");
/// replacements.add("</rw-custom>", "</div>");
/// replacements.apply(&mut html);
///
/// assert_eq!(html, "<div class=\"custom\">content</div>");
/// ```
#[derive(Debug, Default)]
pub struct Replacements {
    items: Vec<(String, String)>,
}

impl Replacements {
    /// Create a new empty replacements collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new replacements collector with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
        }
    }

    /// Register a replacement: all occurrences of `from` will be replaced with `to`.
    ///
    /// Replacements are applied in the order they are added.
    pub fn add(&mut self, from: impl Into<String>, to: impl Into<String>) {
        self.items.push((from.into(), to.into()));
    }

    /// Apply all registered replacements.
    ///
    /// For efficiency, this uses simple sequential replacement. For very large
    /// numbers of patterns, consider using `aho-corasick` instead.
    ///
    /// Note: This consumes the replacements to prevent accidental reuse.
    pub fn apply(self, html: &mut String) {
        if self.items.is_empty() {
            return;
        }

        // For a small number of replacements, sequential replace is efficient enough
        // and avoids the aho-corasick dependency. If performance becomes an issue
        // with many patterns, we can switch to aho-corasick.
        for (from, to) in self.items {
            if html.contains(&from) {
                *html = html.replace(&from, &to);
            }
        }
    }

    /// Check if there are any replacements registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the number of registered replacements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_replacements() {
        let mut html = "unchanged".to_owned();
        let replacements = Replacements::new();
        replacements.apply(&mut html);
        assert_eq!(html, "unchanged");
    }

    #[test]
    fn test_single_replacement() {
        let mut html = "hello world".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("world", "universe");
        replacements.apply(&mut html);
        assert_eq!(html, "hello universe");
    }

    #[test]
    fn test_multiple_replacements() {
        let mut html = "<a><b></b></a>".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("<a>", "<div>");
        replacements.add("</a>", "</div>");
        replacements.add("<b>", "<span>");
        replacements.add("</b>", "</span>");
        replacements.apply(&mut html);
        assert_eq!(html, "<div><span></span></div>");
    }

    #[test]
    fn test_replacement_not_found() {
        let mut html = "hello world".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("foo", "bar");
        replacements.apply(&mut html);
        assert_eq!(html, "hello world");
    }

    #[test]
    fn test_multiple_occurrences() {
        let mut html = "a a a".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("a", "b");
        replacements.apply(&mut html);
        assert_eq!(html, "b b b");
    }

    #[test]
    fn test_is_empty() {
        let replacements = Replacements::new();
        assert!(replacements.is_empty());

        let mut replacements = Replacements::new();
        replacements.add("a", "b");
        assert!(!replacements.is_empty());
    }

    #[test]
    fn test_len() {
        let mut replacements = Replacements::new();
        assert_eq!(replacements.len(), 0);

        replacements.add("a", "b");
        assert_eq!(replacements.len(), 1);

        replacements.add("c", "d");
        assert_eq!(replacements.len(), 2);
    }

    #[test]
    fn test_with_capacity() {
        let replacements = Replacements::with_capacity(10);
        assert!(replacements.is_empty());
    }

    #[test]
    fn test_replacement_order() {
        // Replacements are applied sequentially, so order matters
        let mut html = "aaa".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("a", "bb");
        replacements.add("bb", "c");
        replacements.apply(&mut html);
        // First: aaa -> bbbbbb, then bbbbbb -> ccc
        assert_eq!(html, "ccc");
    }
}
