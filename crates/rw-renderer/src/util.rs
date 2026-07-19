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

/// Appends `s` to `out`, escaping the five HTML special characters
/// (`&`, `<`, `>`, `"`, `'`).
///
/// Prefer this over [`escape_html`] on a hot path: it writes straight into the
/// caller's buffer and bulk-copies the (usually long) runs between special
/// characters with `push_str`, so text with nothing to escape — the common case
/// — is a single copy and no allocation. All five specials are ASCII, so the
/// byte scan never splits a multi-byte character.
///
/// # Examples
///
/// ```
/// use rw_renderer::escape_into;
///
/// let mut out = String::from("<p>");
/// escape_into("a <b> & c", &mut out);
/// out.push_str("</p>");
/// assert_eq!(out, "<p>a &lt;b&gt; &amp; c</p>");
/// ```
pub fn escape_into(s: &str, out: &mut String) {
    let mut run_start = 0;
    for (i, &b) in s.as_bytes().iter().enumerate() {
        let entity = ENTITY[usize::from(SPECIAL[b as usize])];
        if entity.is_empty() {
            continue;
        }
        // Bulk-copy the verbatim run before this special byte, then the entity.
        out.push_str(&s[run_start..i]);
        out.push_str(entity);
        run_start = i + 1;
    }
    out.push_str(&s[run_start..]);
}

/// Maps a byte to its index in [`ENTITY`] — `0` for everything that passes
/// through verbatim, which is almost every byte of almost every text run.
///
/// Kept as 256 *bytes* (four cache lines) rather than a table of `&str`
/// directly: a `[&str; 256]` would be 4 KB of fat pointers and evict the
<<<<<<< Updated upstream
/// surrounding hot data. The scan then costs one load and one compare per byte
/// instead of walking a five-way compare chain.
=======
/// surrounding hot data.
///
/// The scan indexes this table and then [`ENTITY`] on every byte — two loads,
/// not the one a compare-chain replacement suggests — and tests the result's
/// length. That reads like more work than branching on the index first and
/// touching [`ENTITY`] only for the rare special byte, but it measured ~4%
/// faster that way over realistic prose: the unconditional form has no
/// data-dependent branch to mispredict.
///
/// `memchr` was also tried here and is *slower* end to end, despite being
/// several times faster in isolation on long inputs — this function is called
/// with short strings (a mean well under 100 bytes, many under 16), and
/// splitting the single inlined byte loop into a separate search call costs
/// more than the vectorized scan saves. Measure before "simplifying" either
/// decision.
>>>>>>> Stashed changes
static SPECIAL: [u8; 256] = {
    let mut table = [0u8; 256];
    table[b'&' as usize] = 1;
    table[b'<' as usize] = 2;
    table[b'>' as usize] = 3;
    table[b'"' as usize] = 4;
    table[b'\'' as usize] = 5;
    table
};

/// Replacements indexed by [`SPECIAL`]; slot 0 is the "not special" sentinel.
static ENTITY: [&str; 6] = ["", "&amp;", "&lt;", "&gt;", "&quot;", "&#x27;"];

/// Escapes the five HTML special characters (`&`, `<`, `>`, `"`, `'`),
/// returning a new [`String`].
///
/// This is a convenience wrapper over [`escape_into`] for callers that need an
/// owned value; on a hot path where the result is appended to an existing
/// buffer, call [`escape_into`] directly to avoid the intermediate allocation.
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
    escape_into(s, &mut result);
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

    #[test]
    fn test_escape_into_appends() {
        // Appends to existing content rather than replacing it.
        let mut out = String::from("<p>");
        escape_into("a <b> & \"c\" 'd'", &mut out);
        out.push_str("</p>");
        assert_eq!(out, "<p>a &lt;b&gt; &amp; &quot;c&quot; &#x27;d&#x27;</p>");
    }

    #[test]
    fn test_escape_into_no_specials_and_multibyte() {
        // The common case (no specials) and multi-byte UTF-8 must pass through
        // verbatim — the byte scan never splits a multi-byte character.
        let mut out = String::new();
        escape_into("plain — Привет 你好 🎉", &mut out);
        assert_eq!(out, "plain — Привет 你好 🎉");

        // A special char adjacent to multi-byte text still escapes correctly.
        let mut out = String::new();
        escape_into("Привет <b>", &mut out);
        assert_eq!(out, "Привет &lt;b&gt;");
    }

    #[test]
    fn test_escape_into_matches_reference_for_every_ascii_byte() {
        // The lookup table is easy to typo, and a wrong slot would either drop
        // an escape (an XSS hole) or mangle ordinary text. Check every ASCII
        // byte against a spelled-out reference.
        fn reference(b: u8) -> &'static str {
            match b {
                b'&' => "&amp;",
                b'<' => "&lt;",
                b'>' => "&gt;",
                b'"' => "&quot;",
                b'\'' => "&#x27;",
                _ => "",
            }
        }

        for b in 0..=127u8 {
            let ch = char::from(b);
            let expected = match reference(b) {
                "" => String::from(ch),
                entity => entity.to_owned(),
            };
            let mut out = String::new();
            escape_into(&String::from(ch), &mut out);
            assert_eq!(out, expected, "byte {b:#04x} ({ch:?})");
        }
    }

    #[test]
    fn test_escape_into_empty() {
        let mut out = String::from("x");
        escape_into("", &mut out);
        assert_eq!(out, "x");
    }
}
