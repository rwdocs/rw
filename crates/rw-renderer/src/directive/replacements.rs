//! Single-pass string replacement for post-processing.
//!
//! Collects replacements during post-processing and applies them efficiently.

/// Collects string replacements for single-pass application.
///
/// Instead of each directive calling `html.replace()` (a full scan + a whole
/// new allocation per call), all directives register their replacements, then
/// [`apply()`](Self::apply) rewrites them in a single left-to-right pass over
/// the HTML string with a single output allocation.
///
/// # Performance
///
/// ```text
/// Naive approach (P registered patterns):
///   for (from, to) in patterns:        # P iterations
///     html = html.replace(from, to)    # full scan + full realloc per matching pattern
///   Total: O(P × len), up to P whole-string reallocations
///
/// Replacements approach:
///   replacements.apply(html)           # one scan, one output String
///   Total: O(P × len) comparisons, a single allocation
/// ```
///
/// The pattern set is small (a handful of directive markers) and known only at
/// call time, so this uses a plain per-position scan rather than an
/// Aho-Corasick automaton — for so few patterns, building an automaton per
/// render costs far more than it saves.
///
/// # Semantics
///
/// Matching is a single left-to-right pass: the replacement text is *not*
/// re-scanned, so replacements do not chain (a pattern cannot match text
/// produced by an earlier replacement). Where two patterns could match at the
/// same position, the one added first wins, so registration order still
/// resolves overlaps — it just no longer feeds one replacement's output into
/// the next. The directive markers this is used for are unique, non-overlapping
/// sentinels, so this is transparent in practice.
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

    /// Apply all registered replacements in a single pass.
    ///
    /// Rewrites the HTML left-to-right into a single new allocation, bulk-copying
    /// the spans between marker matches — replacing the previous approach of one
    /// full scan and one whole-string reallocation per pattern. See the
    /// [type docs](Self#semantics) for the (non-chaining) match semantics.
    ///
    /// Every directive marker begins with `<`, so in that (universal in practice)
    /// case candidates are found by jumping between `<` positions with the
    /// standard library's memchr-accelerated `find` — the same SIMD search
    /// `str::replace` uses. A pattern set with some other leading byte takes an
    /// equivalent but non-accelerated per-character scan; the result is identical.
    ///
    /// Note: This consumes the replacements to prevent accidental reuse.
    pub fn apply(self, html: &mut String) {
        if self.items.is_empty() {
            return;
        }

        let src = std::mem::take(html);
        let mut out = String::with_capacity(src.len());
        // `run_start` is the start of the span not yet copied to `out`.
        let mut run_start = 0;

        // Fast path: every marker starts with '<', so memchr-jump between '<'.
        if self
            .items
            .iter()
            .all(|(from, _)| from.as_bytes().first() == Some(&b'<'))
        {
            // `search_from` diverges from `run_start` across the many '<' that
            // begin ordinary tags (not markers), so the uncopied span grows and
            // is flushed in one bulk copy at the next real match.
            let mut search_from = 0;
            while let Some(rel) = src[search_from..].find('<') {
                let at = search_from + rel;
                if let Some((from, to)) = self.find_match(&src[at..]) {
                    out.push_str(&src[run_start..at]);
                    out.push_str(to);
                    run_start = at + from.len();
                    search_from = run_start;
                } else {
                    search_from = at + 1;
                }
            }
        } else {
            // General path: advance one char at a time (no SIMD skip). Only
            // reached if a caller registers a non-'<' pattern — never the
            // directive markers — so its slower scan is irrelevant in practice.
            let mut i = 0;
            while i < src.len() {
                if src.is_char_boundary(i)
                    && let Some((from, to)) = self.find_match(&src[i..])
                {
                    out.push_str(&src[run_start..i]);
                    out.push_str(to);
                    i += from.len();
                    run_start = i;
                    continue;
                }
                i += 1;
            }
        }

        out.push_str(&src[run_start..]);
        *html = out;
    }

    /// The first registered pattern that is a prefix of `rest`, if any. First
    /// wins, preserving the documented "applied in the order they are added"
    /// priority when patterns could match at the same position.
    fn find_match(&self, rest: &str) -> Option<(&String, &String)> {
        self.items
            .iter()
            .find(|(from, _)| !from.is_empty() && rest.starts_with(from.as_str()))
            .map(|(from, to)| (from, to))
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
    fn test_single_pass_no_chaining() {
        // Single pass: replacement output is NOT re-scanned, so "bb" (produced
        // by the first rule) is not itself replaced by the second rule.
        let mut html = "aaa".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("a", "bb");
        replacements.add("bb", "c");
        replacements.apply(&mut html);
        // Each "a" is rewritten to "bb" once; the emitted "bb" stays put.
        assert_eq!(html, "bbbbbb");
    }

    #[test]
    fn test_overlap_prefers_first_registered() {
        // When two patterns can match at the same position, the one added first
        // wins (LeftmostFirst) — registration order resolves the overlap.
        let mut html = "<rw-tabs>".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("<rw-tab>", "<SHORT>");
        replacements.add("<rw-tabs>", "<LONG>");
        replacements.apply(&mut html);
        // "<rw-tab>" cannot match here (char mismatch at 's'), so "<rw-tabs>" wins.
        assert_eq!(html, "<LONG>");
    }
}
