//! The `{#id .class key=value}` attribute block, in a fence info string or
//! after a directive.
//!
//! ```text
//! mermaid {#architecture format=png}
//! :status[Done]{color=green}
//! ```
//!
//! Attributes are parsed but never interpreted here: whoever claims the fence
//! or directive reads `format`, `id`, `color` and the rest.

use crate::directive::line::Escapes;

/// Parsed `{ … }` attribute block. A `.class` token in it is skipped.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Attrs {
    /// Explicit id from `{#id}` (last one wins). `None` when absent.
    pub id: Option<String>,
    /// `key=value` attributes (and valueless flags, value `""`) from the block,
    /// in source order. Write through [`insert`](Self::insert) to keep
    /// last-write-wins.
    pub map: Vec<(String, String)>,
}

impl Attrs {
    /// Set an attribute, replacing any existing value for `key` in place.
    pub fn insert(&mut self, key: String, value: String) {
        if let Some(entry) = self.map.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = value;
        } else {
            self.map.push((key, value));
        }
    }
}

/// Parse a fence info string into its language and attribute block.
///
/// Outside the braces, the first whitespace token is the language and every
/// other bare token is ignored.
#[must_use]
pub(crate) fn parse_fence_info(info: &str) -> (String, Attrs) {
    let (before_brace, inner) = split_brace_block(info);
    let language = before_brace
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_owned();
    // cmark has already decoded the info string's escapes.
    let attrs = inner
        .map(|inner| parse_attrs(inner, Escapes::default()))
        .unwrap_or_default();
    (language, attrs)
}

/// Split off a single `{ … }` block: the substring before the first `{`, and
/// the content between the first `{` and the *first* `}` after it. Closing on
/// the first `}` (not the last) keeps two adjacent groups like `{#a}{#b}` from
/// merging into one corrupted block; only the first group is honored.
fn split_brace_block(info: &str) -> (&str, Option<&str>) {
    if let Some(open) = info.find('{')
        && let Some(close_rel) = info[open + 1..].find('}')
    {
        let close = open + 1 + close_rel;
        return (&info[..open], Some(&info[open + 1..close]));
    }
    (info, None)
}

/// Parse the inside of a `{ … }` block: whitespace-separated tokens, each
/// classified by its first byte — `#id`, `.class`, `key=value`, or a bare
/// flag. Tokens of length ≤ 1 are ignored. This is an original implementation
/// modeled on the documented Pandoc/heading-attribute behavior; no
/// third-party parser code is reused.
///
/// A value opening with `"` or `'` runs, whitespace included, to the first
/// matching quote that is followed by whitespace or the end of the block:
/// `alt="Payment flow"`, `alt='Customer's order'`. One with no such quote
/// ends at whitespace like an unquoted value, less its opening quote. A quote
/// in `escapes` neither opens nor closes a value.
pub(crate) fn parse_attrs(inner: &str, escapes: Escapes<'_>) -> Attrs {
    let mut attrs = Attrs::default();
    let mut rest = inner.trim_start();
    while !rest.is_empty() {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let token = &rest[..end];
        let mut next = &rest[end..];
        // A lone `#`, `.`, or single-char token names nothing.
        if token.len() > 1 {
            match token.as_bytes()[0] {
                b'#' => attrs.id = Some(token[1..].to_owned()),
                b'.' => {}
                _ => match token.split_once('=') {
                    Some(("", _)) => {}
                    Some((key, _)) => {
                        let after_eq = &rest[key.len() + 1..];
                        let at = inner.len() - after_eq.len();
                        let (value, after) = split_value(after_eq, escapes.skip(at));
                        attrs.insert(key.to_owned(), value.to_owned());
                        next = after;
                    }
                    None => attrs.insert(token.to_owned(), String::new()),
                },
            }
        }
        rest = next.trim_start();
    }
    attrs
}

/// Split a value off the start of `s`: its content, and what follows it.
fn split_value<'s>(s: &'s str, escapes: Escapes<'_>) -> (&'s str, &'s str) {
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    let Some(quote) = s
        .chars()
        .next()
        .filter(|&c| (c == '"' || c == '\'') && !escapes.contains(0))
    else {
        return (&s[..end], &s[end..]);
    };
    let close = s[1..].match_indices(quote).map(|(i, _)| i + 1).find(|&i| {
        !escapes.contains(i) && s[i + 1..].chars().next().is_none_or(char::is_whitespace)
    });
    match close {
        Some(close) => (&s[1..close], &s[close + 1..]),
        None => (&s[1..end], &s[end..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_language_only() {
        let (lang, attrs) = parse_fence_info("rust");
        assert_eq!(lang, "rust");
        assert_eq!(attrs, Attrs::default());
    }

    fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|&(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }

    /// A valueless flag stores the empty string: present with an empty value
    /// rather than absent, and not joined to the key after it.
    #[test]
    fn parse_brace_valueless_flag_stores_empty_value() {
        let (_lang, attrs) = parse_fence_info("mermaid {standalone format=png}");
        assert_eq!(attrs.map, pairs(&[("standalone", ""), ("format", "png")]));
    }

    /// Keys stay in the order the author wrote them, so a consumer reporting
    /// unknown attributes reports them in source order.
    #[test]
    fn attrs_keep_author_order() {
        let (_lang, attrs) = parse_fence_info("mermaid {zebra=1 alpha=2 zebra=3}");
        // Author order, not sorted; the repeated key keeps its first position
        // while taking its last value.
        assert_eq!(attrs.map, pairs(&[("zebra", "3"), ("alpha", "2")]));
    }

    #[test]
    fn parse_brace_id() {
        let (lang, attrs) = parse_fence_info("mermaid {#architecture}");
        assert_eq!(lang, "mermaid");
        assert_eq!(attrs.id.as_deref(), Some("architecture"));
        assert_eq!(attrs.map, [] as [(String, String); 0]);
    }

    #[test]
    fn parse_brace_id_classes_kv() {
        let (lang, attrs) = parse_fence_info("plantuml {#a .b .c format=png k=v}");
        assert_eq!(lang, "plantuml");
        assert_eq!(attrs.id.as_deref(), Some("a"));
        assert_eq!(attrs.map, pairs(&[("format", "png"), ("k", "v")]));
    }

    #[test]
    fn parse_brace_last_id_wins() {
        let (_lang, attrs) = parse_fence_info("mermaid {#a #b}");
        assert_eq!(attrs.id.as_deref(), Some("b"));
    }

    /// A repeated `key=value` in one brace block keeps only the last value.
    #[test]
    fn parse_brace_last_duplicate_key_wins() {
        let (_lang, attrs) = parse_fence_info("mermaid {format=svg format=png}");
        assert_eq!(attrs.map, pairs(&[("format", "png")]));
    }

    #[test]
    fn parse_bare_tokens_ignored() {
        // Outside the braces, bare id=/format= are NOT attributes.
        let (lang, attrs) = parse_fence_info("mermaid id=foo format=png");
        assert_eq!(lang, "mermaid");
        assert_eq!(attrs.id, None);
        assert_eq!(attrs.map, [] as [(String, String); 0]);
    }

    #[test]
    fn parse_brace_format_only() {
        let (_lang, attrs) = parse_fence_info("mermaid {format=svg}");
        assert_eq!(attrs.id, None);
        assert_eq!(attrs.map, pairs(&[("format", "svg")]));
    }

    #[test]
    fn parse_degenerate_braces_no_panic() {
        for info in ["mermaid {}", "mermaid {#}", "mermaid {#foo", "mermaid }{"] {
            let (lang, attrs) = parse_fence_info(info);
            assert_eq!(lang, "mermaid");
            assert_eq!(attrs.id, None, "info: {info}");
        }
    }

    #[test]
    fn parse_non_ascii_id_and_flags_no_panic() {
        let (_lang, attrs) = parse_fence_info("mermaid {#заголовок цвет 🎉 k=v}");
        assert_eq!(attrs.id.as_deref(), Some("заголовок"));
        assert_eq!(attrs.map, pairs(&[("цвет", ""), ("🎉", ""), ("k", "v")]));
    }

    #[test]
    fn parse_multiple_brace_groups_takes_first() {
        // Two adjacent groups must not merge into one corrupted block: the
        // block ends at the first `}`, so only the first group is honored.
        let (lang, attrs) = parse_fence_info("mermaid {#hello}{format=png}");
        assert_eq!(lang, "mermaid");
        assert_eq!(attrs.id.as_deref(), Some("hello"));
        assert!(
            attrs.map.is_empty(),
            "second group must be ignored, not merged"
        );

        let (_lang, attrs) = parse_fence_info("mermaid {#a} {b=c}");
        assert_eq!(attrs.id.as_deref(), Some("a"));
        assert_eq!(attrs.map, [] as [(String, String); 0]);
    }

    #[test]
    fn parse_quoted_kv_value_trimmed() {
        let (_lang, attrs) = parse_fence_info("mermaid {caption=\"User\"}");
        assert_eq!(attrs.map, pairs(&[("caption", "User")]));
    }

    #[test]
    fn a_quoted_value_keeps_its_whitespace() {
        let (_lang, attrs) = parse_fence_info(r#"plantuml {#flow alt="Payment  flow" format=png}"#);
        assert_eq!(attrs.id.as_deref(), Some("flow"));
        assert_eq!(
            attrs.map,
            pairs(&[("alt", "Payment  flow"), ("format", "png")])
        );
    }

    #[test]
    fn a_quoted_value_may_hold_the_other_quote() {
        let (_lang, attrs) = parse_fence_info(r#"plantuml {a='Say "hi" now' b="it's"}"#);
        assert_eq!(attrs.map, pairs(&[("a", r#"Say "hi" now"#), ("b", "it's")]));
    }

    /// An apostrophe is not the end of a `'`-quoted value.
    #[test]
    fn a_quote_closes_only_before_whitespace_or_the_block_end() {
        let (_lang, attrs) = parse_fence_info(r"plantuml {alt='Customer's order flow' format=png}");
        assert_eq!(
            attrs.map,
            pairs(&[("alt", "Customer's order flow"), ("format", "png")])
        );

        let (_lang, attrs) = parse_fence_info(r#"plantuml {alt="Say "hi""}"#);
        assert_eq!(attrs.map, pairs(&[("alt", r#"Say "hi""#)]));
    }

    /// With no closing quote at all, the value does not swallow the rest of
    /// the block.
    #[test]
    fn a_quote_that_never_closes_ends_at_whitespace() {
        let (_lang, attrs) = parse_fence_info(r#"plantuml {alt="open format=png}"#);
        assert_eq!(attrs.map, pairs(&[("alt", "open"), ("format", "png")]));
    }

    /// Nothing tells which quote the author forgot, so a later quoted value's
    /// closing quote closes this one. Pinned so a change to it is deliberate.
    #[test]
    fn a_missing_closing_quote_swallows_a_later_quoted_value() {
        let (_lang, attrs) = parse_fence_info(r#"plantuml {alt="open format="png"}"#);
        assert_eq!(attrs.map, pairs(&[("alt", r#"open format="png"#)]));
    }

    #[test]
    fn an_empty_quoted_value_is_empty() {
        let (_lang, attrs) = parse_fence_info(r#"plantuml {alt="" k=v}"#);
        assert_eq!(attrs.map, pairs(&[("alt", ""), ("k", "v")]));
    }
}
