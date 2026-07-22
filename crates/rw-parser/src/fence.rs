//! The fence info string: a language plus an optional `{ … }` attribute block.
//!
//! ```text
//! mermaid {#architecture format=png}
//!         ^^^^^^^^^^^^^^^^^^^^^^^^^^ the attribute block
//! ```
//!
//! Attributes are parsed but never interpreted here: they are carried
//! through to whichever code block processor claims the fence, which is what
//! reads `format`, `id` and the rest.
//!
//! The `{#id}` form is part of rw's documented fence dialect, so the grammar
//! belongs beside the rest of the syntax rather than beside its consumer.
//!
//! This grammar is close to, but not the same as, the one
//! [`DirectiveArgs`](crate::DirectiveArgs) accepts — see the crate docs for
//! where the two disagree and why they are left that way.

/// Parsed fence info string: the language plus an optional `{ … }`
/// attribute block.
///
/// Only the brace block populates attributes. Outside the braces, the first
/// whitespace token is the language and every other bare token is ignored.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FenceAttrs {
    /// Explicit id from `{#id}` (last one wins). `None` when absent.
    pub id: Option<String>,
    /// Classes from `{.class}`, in source order.
    pub classes: Vec<String>,
    /// `key=value` attributes (and valueless flags, value `""`) from the block,
    /// in source order.
    ///
    /// A `Vec` rather than a `HashMap`. Fences carry a handful of attributes at
    /// most, so hashing earns nothing at this size, iteration order is the
    /// author's instead of `RandomState`'s, and the struct is 24 bytes smaller
    /// (measured: 96 → 72). Write through [`insert`](Self::insert) to preserve
    /// last-write-wins.
    pub map: Vec<(String, String)>,
}

impl FenceAttrs {
    /// Look up an attribute value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Set an attribute, replacing any existing value for `key`.
    ///
    /// Reproduces `HashMap::insert`'s last-write-wins: a repeated key
    /// overwrites in place rather than appending a second entry, so the last
    /// token wins. Unlike `HashMap::insert` it does not hand back the displaced
    /// value — no caller wants it.
    pub fn insert(&mut self, key: String, value: String) {
        if let Some(entry) = self.map.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = value;
        } else {
            self.map.push((key, value));
        }
    }

    /// Attribute keys, in source order.
    ///
    /// No `#[must_use]`: `impl Iterator` already carries one, and doubling it
    /// trips `clippy::double_must_use`.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.map.iter().map(|(k, _)| k.as_str())
    }
}

/// Parse a fence info string into its language and attribute block.
///
/// Grammar inside a single `{ … }` span: whitespace-separated tokens, each
/// classified by its first byte — `#id`, `.class`, `key=value`, or a bare flag.
/// Tokens of length ≤ 1 are ignored. This is an original implementation modeled
/// on the documented Pandoc/heading-attribute behavior; no third-party parser
/// code is reused.
#[must_use]
pub fn parse_fence_info(info: &str) -> (String, FenceAttrs) {
    let (before_brace, inner) = split_brace_block(info);
    let language = before_brace
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_owned();

    let mut attrs = FenceAttrs::default();
    if let Some(inner) = inner {
        parse_attr_block(inner, &mut attrs);
    }
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

/// Parse the tokens inside a brace block into `attrs`, dispatching each
/// whitespace-separated token by its first byte (`#`→id, `.`→class, else
/// `key=value`). A later `#id` overwrites an earlier one (last wins); classes
/// accumulate.
fn parse_attr_block(inner: &str, attrs: &mut FenceAttrs) {
    for token in inner.split_whitespace() {
        if token.len() <= 1 {
            // Lone `#`, `.`, or a single-char token — nothing to name.
            continue;
        }
        match token.as_bytes()[0] {
            b'#' => attrs.id = Some(token[1..].to_owned()),
            b'.' => attrs.classes.push(token[1..].to_owned()),
            _ => {
                if let Some((key, value)) = token.split_once('=') {
                    if !key.is_empty() {
                        let value = value.trim_matches('"').trim_matches('\'');
                        attrs.insert(key.to_owned(), value.to_owned());
                    }
                } else {
                    attrs.insert(token.to_owned(), String::new());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_language_only() {
        let (lang, attrs) = parse_fence_info("rust");
        assert_eq!(lang, "rust");
        assert_eq!(attrs, FenceAttrs::default());
    }

    /// A valueless flag stores the empty string, so `get` reports it present
    /// with an empty value rather than absent. `parse_attr_block`'s bare-token
    /// branch is otherwise unexercised — every other fence test uses `#id`,
    /// `.class`, or `key=value`.
    #[test]
    fn parse_brace_valueless_flag_stores_empty_value() {
        let (_lang, attrs) = parse_fence_info("mermaid {standalone}");
        assert_eq!(attrs.get("standalone"), Some(""));
        assert_eq!(attrs.keys().collect::<Vec<_>>(), ["standalone"]);
    }

    /// `keys` yields every key once, in the order the author wrote them, so a
    /// consumer reporting unknown attributes reports them in source order.
    #[test]
    fn keys_yields_author_order_and_get_misses_are_none() {
        let (_lang, attrs) = parse_fence_info("mermaid {zebra=1 alpha=2 zebra=3}");
        // Author order, not sorted; the repeated key keeps its first position
        // while taking its last value.
        assert_eq!(attrs.keys().collect::<Vec<_>>(), ["zebra", "alpha"]);
        assert_eq!(attrs.get("zebra"), Some("3"));
        assert_eq!(attrs.get("absent"), None);
    }

    #[test]
    fn parse_brace_id() {
        let (lang, attrs) = parse_fence_info("mermaid {#architecture}");
        assert_eq!(lang, "mermaid");
        assert_eq!(attrs.id.as_deref(), Some("architecture"));
        assert!(attrs.classes.is_empty());
        assert!(attrs.map.is_empty());
    }

    #[test]
    fn parse_brace_id_classes_kv() {
        let (lang, attrs) = parse_fence_info("plantuml {#a .b .c format=png k=v}");
        assert_eq!(lang, "plantuml");
        assert_eq!(attrs.id.as_deref(), Some("a"));
        assert_eq!(attrs.classes, vec!["b".to_owned(), "c".to_owned()]);
        assert_eq!(attrs.get("format"), Some("png"));
        assert_eq!(attrs.get("k"), Some("v"));
    }

    #[test]
    fn parse_brace_last_id_wins() {
        let (_lang, attrs) = parse_fence_info("mermaid {#a #b}");
        assert_eq!(attrs.id.as_deref(), Some("b"));
    }

    /// A repeated `key=value` in one brace block keeps only the last value.
    /// This is `HashMap::insert` semantics; a `Vec` representation must match it
    /// by overwriting in place rather than appending a second entry — hence the
    /// length assertion, which a naive `push` would fail while `get` still
    /// happened to return the right value.
    #[test]
    fn parse_brace_last_duplicate_key_wins() {
        let (_lang, attrs) = parse_fence_info("mermaid {format=svg format=png}");
        assert_eq!(attrs.get("format"), Some("png"));
        assert_eq!(attrs.map.len(), 1, "duplicate key must not add an entry");
    }

    #[test]
    fn parse_bare_tokens_ignored() {
        // Outside the braces, bare id=/format= are NOT attributes.
        let (lang, attrs) = parse_fence_info("mermaid id=foo format=png");
        assert_eq!(lang, "mermaid");
        assert_eq!(attrs.id, None);
        assert!(attrs.map.is_empty());
    }

    #[test]
    fn parse_brace_format_only() {
        let (_lang, attrs) = parse_fence_info("mermaid {format=svg}");
        assert_eq!(attrs.id, None);
        assert_eq!(attrs.get("format"), Some("svg"));
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
    fn parse_non_ascii_id_no_panic() {
        let (_lang, attrs) = parse_fence_info("mermaid {#заголовок}");
        assert_eq!(attrs.id.as_deref(), Some("заголовок"));
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
        assert!(attrs.map.is_empty());
    }

    #[test]
    fn parse_quoted_kv_value_trimmed() {
        let (_lang, attrs) = parse_fence_info("mermaid {caption=\"User\"}");
        assert_eq!(attrs.get("caption"), Some("User"));
    }
}
