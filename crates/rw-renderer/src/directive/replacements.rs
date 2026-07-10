//! Single-pass string replacement for post-processing.
//!
//! Collects replacements during post-processing and applies them efficiently.

/// Collects string replacements for single-pass application.
///
/// Instead of each directive calling `html.replace()` (a full scan + fresh
/// allocation per call), all directives register their replacements, then
/// [`apply()`](Self::apply) rewrites the HTML in a single scan with one output
/// allocation — regardless of how many patterns are registered.
///
/// # Semantics
///
/// `apply` scans the input once and, at each position, replaces the
/// **leftmost–longest** matching pattern (every occurrence, left to right). It
/// does **not** cascade: a replacement's output is copied verbatim and is never
/// re-scanned, so one pattern cannot rewrite another pattern's output. Directive
/// markers never expand into other markers, so nothing in this crate needs
/// cascading — but an external caller must not rely on it.
///
/// # Performance
///
/// The obvious implementation — a `str::replace` per pattern — costs up to
/// `2·N` full-document scans and `N` allocations for `N` patterns (a
/// `contains` scan plus a scan-and-reallocate `replace` each). This single
/// pass avoids both: one left-to-right scan bulk-copies the spans between
/// matches into one output buffer (the same run-copy idiom as
/// [`escape_into`](crate::escape_into)), allocated only if something actually
/// matches.
///
/// Every directive marker begins with `<`, so when all patterns share one
/// ASCII lead byte the scan jumps between its occurrences with the standard
/// library's memchr-accelerated `find` — SIMD-skipping the verbatim runs
/// between markers. A pattern set with mixed lead bytes falls back to a plain
/// byte scan gated by a 256-entry first-byte lookup table.
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
    /// For distinct patterns, registration order does not matter:
    /// [`apply`](Self::apply) resolves overlaps by leftmost–longest match, not
    /// by insertion order. Registering the same `from` twice is the one
    /// order-sensitive case — the last registration's `to` wins.
    pub fn add(&mut self, from: impl Into<String>, to: impl Into<String>) {
        self.items.push((from.into(), to.into()));
    }

    /// Apply all registered replacements in a single scan.
    ///
    /// See the [type docs](Self) for the leftmost–longest, non-cascading
    /// semantics. Consumes `self` to prevent accidental reuse.
    pub fn apply(self, html: &mut String) {
        // Ignore empty patterns (a `from` of "" would "match" every position
        // and never advance). Filter in place to reuse the existing allocation.
        let mut patterns = self.items;
        patterns.retain(|(from, _)| !from.is_empty());
        if patterns.is_empty() {
            return;
        }

        // Allocate the output buffer lazily, on the first match: a page with no
        // marker (patterns registered but unused — the common case) leaves
        // `html` untouched with zero allocation.
        let mut out: Option<String> = None;
        // `run_start` is the start of the span not yet copied into `out`.
        let mut run_start = 0;

        // Every directive marker begins with `<`, so when all patterns share one
        // ASCII lead byte, jump between its occurrences with the standard
        // library's memchr-accelerated `find` — the SIMD skip over the verbatim
        // runs between markers is why this beats a per-byte scan on prose-heavy
        // pages. A pattern set with mixed lead bytes falls back to a plain byte
        // scan gated by a 256-entry first-byte lookup table.
        let lead = patterns[0].0.as_bytes()[0];
        if lead.is_ascii() && patterns.iter().all(|(from, _)| from.as_bytes()[0] == lead) {
            let lead = lead as char;
            let mut search_from = 0;
            while let Some(rel) = html[search_from..].find(lead) {
                let at = search_from + rel;
                if let Some((from, to)) = longest_match(&patterns, &html.as_bytes()[at..]) {
                    let out = out.get_or_insert_with(|| String::with_capacity(html.len()));
                    out.push_str(&html[run_start..at]);
                    out.push_str(to);
                    run_start = at + from.len();
                    search_from = run_start;
                } else {
                    // A `<` that starts no marker: keep scanning past it.
                    search_from = at + 1;
                }
            }
        } else {
            let mut maybe_start = [false; 256];
            for (from, _) in &patterns {
                maybe_start[from.as_bytes()[0] as usize] = true;
            }
            let bytes = html.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if maybe_start[bytes[i] as usize]
                    && let Some((from, to)) = longest_match(&patterns, &bytes[i..])
                {
                    let out = out.get_or_insert_with(|| String::with_capacity(html.len()));
                    out.push_str(&html[run_start..i]);
                    out.push_str(to);
                    i += from.len();
                    run_start = i;
                    continue;
                }
                i += 1;
            }
        }

        if let Some(mut out) = out {
            out.push_str(&html[run_start..]);
            *html = out;
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

/// The leftmost–longest pattern matching at the start of `hay`, if any.
///
/// Longest wins so a shorter prefix can't shadow a longer marker (e.g.
/// `</rw-tab>` must not preempt `</rw-tabs>`). Patterns are valid UTF-8, so a
/// byte match against valid-UTF-8 input can only start on a char boundary, and
/// a whole-pattern match that starts on a boundary also ends on one — hence the
/// caller can slice the input at both ends of a match safely.
fn longest_match<'a>(patterns: &'a [(String, String)], hay: &[u8]) -> Option<&'a (String, String)> {
    patterns
        .iter()
        .filter(|(from, _)| hay.starts_with(from.as_bytes()))
        .max_by_key(|(from, _)| from.len())
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
    fn test_no_cascade() {
        // A single pass does not re-scan its own output: `a -> bb` fires, but
        // the freshly written `bb` is never rewritten by the `bb -> c` rule.
        let mut html = "aaa".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("a", "bb");
        replacements.add("bb", "c");
        replacements.apply(&mut html);
        assert_eq!(html, "bbbbbb");
    }

    #[test]
    fn test_longest_match_wins() {
        // Where two patterns genuinely match at the same position (`ab` and
        // `abc` both match at index 0 of "abcd"), the longer one wins —
        // regardless of registration order. A `min`-length pick would give
        // "Xcd"; leftmost–longest gives "Yd".
        let mut html = "abcd".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("ab", "X");
        replacements.add("abc", "Y");
        replacements.apply(&mut html);
        assert_eq!(html, "Yd");
    }

    #[test]
    fn test_duplicate_from_last_registration_wins() {
        // Documented edge: identical `from` registered twice is the one
        // order-sensitive case — the last `to` wins (max_by_key keeps the last
        // of equal-length matches).
        let mut html = "<x>".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("<x>", "AAA");
        replacements.add("<x>", "BBB");
        replacements.apply(&mut html);
        assert_eq!(html, "BBB");
    }

    #[test]
    fn test_mixed_lead_bytes_use_byte_scan_fallback() {
        // Patterns with different first bytes can't use the single-lead-byte
        // memchr jump, so this drives the byte-table fallback path; the result
        // must be identical.
        let mut html = "x=1 & y<2 or z>3".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("&", "AMP");
        replacements.add("<", "LT");
        replacements.add(">", "GT");
        replacements.apply(&mut html);
        assert_eq!(html, "x=1 AMP yLT2 or zGT3");
    }

    #[test]
    fn test_candidate_byte_without_full_match_is_left_intact() {
        // Exercises the scanner's near-miss path: `<` makes every position a
        // *candidate* (it is a registered pattern's first byte), but the full
        // marker never appears, so nothing is replaced and — via the lazy
        // buffer — the input is returned untouched with no rebuild.
        let mut html = "a < b <rw-y> c </rw-z>".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("<rw-x>", "<b>");
        replacements.add("</rw-x>", "</b>");
        replacements.apply(&mut html);
        assert_eq!(html, "a < b <rw-y> c </rw-z>");
    }

    #[test]
    fn test_multibyte_runs_preserved() {
        // Verbatim runs around a match must survive byte-scanning intact.
        let mut html = "Привет <rw-x> 你好 <rw-x> 🎉".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("<rw-x>", "<b>");
        replacements.apply(&mut html);
        assert_eq!(html, "Привет <b> 你好 <b> 🎉");
    }

    #[test]
    fn test_empty_pattern_ignored() {
        let mut html = "abc".to_owned();
        let mut replacements = Replacements::new();
        replacements.add("", "X");
        replacements.add("b", "Y");
        replacements.apply(&mut html);
        assert_eq!(html, "aYc");
    }
}
