//! Source-to-source rewriting of fenced code block bodies.
//!
//! [`rewrite_fences`] replaces the body of selected fences and copies
//! everything else through byte for byte, so fences, info strings, and
//! surrounding prose survive untouched. Used to inline external references
//! (`PlantUML` `!include` directives) into a document before it is published
//! somewhere the originals cannot be read.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser as CmarkParser, Tag, TagEnd};

use crate::parse_fence_info;

/// Rewrite the body of each fenced code block for which `rewrite` returns
/// `Some`.
///
/// `rewrite` receives the fence's language (the first word of the info string)
/// and its body without the trailing newline. Returning `None` leaves that
/// fence exactly as it was.
///
/// Three kinds of block never reach `rewrite` at all, so returning `Some` for
/// them is impossible: fences with no language, indented code blocks (they
/// have no info string), and fences with an empty body.
///
/// Everything outside a replaced body — fence markers, info strings including
/// their `{…}` attributes, and all surrounding content — is preserved byte for
/// byte.
///
/// # Examples
///
/// ```
/// use rw_parser::rewrite_fences;
///
/// let md = "```upper\nhello\n```\n";
/// let out = rewrite_fences(md, |lang, src| {
///     (lang == "upper").then(|| src.to_uppercase())
/// });
/// assert_eq!(out, "```upper\nHELLO\n```\n");
/// ```
#[must_use]
pub fn rewrite_fences(
    markdown: &str,
    mut rewrite: impl FnMut(&str, &str) -> Option<String>,
) -> String {
    // Parsed with `pulldown_cmark` directly rather than `crate::Parser`:
    // splicing needs each body's byte range, and `Parser::next` lends events
    // without one. Exposing ranges there would be a public API for this single
    // caller. Fence delimiters are core `CommonMark`, so the dialect's extras
    // cannot change which fences are found.
    let parser = CmarkParser::new_ext(markdown, Options::empty());
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();

    let mut language: Option<String> = None;
    let mut body_range: Option<Range<usize>> = None;
    let mut body = String::new();

    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let (lang, _attrs) = parse_fence_info(&info);
                language = (!lang.is_empty()).then_some(lang);
                body.clear();
                body_range = None;
            }
            Event::Text(text) if language.is_some() => {
                match body_range.as_mut() {
                    Some(existing) => existing.end = range.end,
                    None => body_range = Some(range.clone()),
                }
                body.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let (Some(lang), Some(found)) = (&language, &body_range) {
                    // pulldown-cmark counts the newline before the closing
                    // fence as body text. Splicing over it would swallow the
                    // line break and glue the replacement to the fence.
                    let (source, replace_range) = match body.strip_suffix('\n') {
                        Some(stripped) => (stripped, found.start..found.end - 1),
                        None => (body.as_str(), found.clone()),
                    };
                    if let Some(resolved) = rewrite(lang, source) {
                        replacements.push((replace_range, resolved));
                    }
                }
                language = None;
                body_range = None;
                body.clear();
            }
            _ => {}
        }
    }

    if replacements.is_empty() {
        return markdown.to_owned();
    }

    // Single forward pass: copy the gap before each replacement, then the
    // replacement. Ranges index the original, so they stay valid however much
    // the replacements change the length.
    let mut result = String::with_capacity(markdown.len());
    let mut pos = 0;
    for (range, replacement) in &replacements {
        result.push_str(&markdown[pos..range.start]);
        result.push_str(replacement);
        pos = range.end;
    }
    result.push_str(&markdown[pos..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Uppercases the body of every `upper` fence, ignores all others.
    fn upper(language: &str, source: &str) -> Option<String> {
        (language == "upper").then(|| source.to_uppercase())
    }

    #[test]
    fn no_matching_fence_returns_input_unchanged() {
        let md = "# Hello\n\n```rust\nfn main() {}\n```\n";
        assert_eq!(rewrite_fences(md, upper), md);
    }

    #[test]
    fn no_fences_at_all_returns_input_unchanged() {
        let md = "# Hello\n\nSome paragraph text.\n\n- List item\n";
        assert_eq!(rewrite_fences(md, upper), md);
    }

    #[test]
    fn matching_fence_body_is_replaced() {
        let md = "```upper\nhello world\n```\n";
        assert_eq!(rewrite_fences(md, upper), "```upper\nHELLO WORLD\n```\n");
    }

    #[test]
    fn surrounding_content_is_preserved_exactly() {
        let md = "# Title\n\nBefore.\n\n```upper\nhello\n```\n\nAfter.\n";
        assert_eq!(
            rewrite_fences(md, upper),
            "# Title\n\nBefore.\n\n```upper\nHELLO\n```\n\nAfter.\n"
        );
    }

    #[test]
    fn only_matching_fences_among_several_are_replaced() {
        let md = "```rust\nkeep me\n```\n\n```upper\nchange me\n```\n";
        assert_eq!(
            rewrite_fences(md, upper),
            "```rust\nkeep me\n```\n\n```upper\nCHANGE ME\n```\n"
        );
    }

    /// A caller wanting several rules composes them in one closure. This pins
    /// that composition still reaches each fence with the right rule.
    #[test]
    fn one_closure_can_dispatch_on_language() {
        let md = "```upper\nab\n```\n\n```rev\nxyz\n```\n";
        let result = rewrite_fences(md, |language, source| match language {
            "upper" => Some(source.to_uppercase()),
            "rev" => Some(source.chars().rev().collect()),
            _ => None,
        });
        assert_eq!(result, "```upper\nAB\n```\n\n```rev\nzyx\n```\n");
    }

    /// The fence info string carries `{…}` attributes; the closure sees only
    /// the language, and the whole info string survives into the output.
    #[test]
    fn fence_attributes_are_preserved() {
        let md = "```upper {#id format=png}\nhello\n```\n";
        assert_eq!(
            rewrite_fences(md, upper),
            "```upper {#id format=png}\nHELLO\n```\n"
        );
    }

    #[test]
    fn four_backtick_fence_is_handled() {
        let md = "````upper\nhello\n````\n";
        assert_eq!(rewrite_fences(md, upper), "````upper\nHELLO\n````\n");
    }

    /// Records every `(language, source)` pair the closure is offered, and
    /// replaces all of them.
    ///
    /// Asserting on the *offers* is what lets the exclusion tests
    /// discriminate. A closure that only fires for one language (like `upper`)
    /// leaves the output unchanged whether the exclusion holds or not, so it
    /// cannot tell "never offered" from "offered and declined".
    fn record_all(seen: &mut Vec<String>) -> impl FnMut(&str, &str) -> Option<String> + '_ {
        |language, source| {
            seen.push(format!("{language}:{source}"));
            Some("REPLACED".to_owned())
        }
    }

    /// Indented code blocks have no info string, so no language.
    #[test]
    fn indented_code_block_is_never_offered_to_the_closure() {
        let md = "    hello world\n";
        let mut seen = Vec::new();
        let result = rewrite_fences(md, record_all(&mut seen));
        assert!(seen.is_empty(), "closure was offered {seen:?}");
        assert_eq!(result, md);
    }

    #[test]
    fn empty_fence_body_is_never_offered_to_the_closure() {
        let md = "```upper\n```\n";
        let mut seen = Vec::new();
        let result = rewrite_fences(md, record_all(&mut seen));
        assert!(seen.is_empty(), "closure was offered {seen:?}");
        assert_eq!(result, md);
    }

    #[test]
    fn fence_without_a_language_is_never_offered_to_the_closure() {
        let md = "```\nhello\n```\n";
        let mut seen = Vec::new();
        let result = rewrite_fences(md, record_all(&mut seen));
        assert!(seen.is_empty(), "closure was offered {seen:?}");
        assert_eq!(result, md);
    }

    #[test]
    fn multiline_body_keeps_its_interior_newlines() {
        let md = "```upper\nline one\nline two\nline three\n```\n";
        assert_eq!(
            rewrite_fences(md, upper),
            "```upper\nLINE ONE\nLINE TWO\nLINE THREE\n```\n"
        );
    }

    /// A replacement longer than what it replaces must not corrupt the offsets
    /// of later splices — the reason the copy is a single forward pass.
    #[test]
    fn replacement_of_different_length_keeps_later_fences_intact() {
        let md = "```grow\na\n```\n\nmiddle\n\n```grow\nb\n```\n";
        let result = rewrite_fences(md, |language, source| {
            (language == "grow").then(|| source.repeat(5))
        });
        assert_eq!(
            result,
            "```grow\naaaaa\n```\n\nmiddle\n\n```grow\nbbbbb\n```\n"
        );
    }

    /// Splicing is all byte-offset arithmetic, so a multibyte document is the
    /// case that would panic on a slice landing mid-character.
    #[test]
    fn multibyte_content_inside_and_around_a_fence_survives() {
        let md = "Привет 👋\n\n```upper\nдіаграма — ok\n```\n\nпосле 🎉\n";
        assert_eq!(
            rewrite_fences(md, upper),
            "Привет 👋\n\n```upper\nДІАГРАМА — OK\n```\n\nпосле 🎉\n"
        );
    }

    /// An unterminated fence still closes at EOF, so its body is rewritten
    /// like any other.
    #[test]
    fn fence_left_unclosed_at_end_of_input_is_still_rewritten() {
        let md = "```upper\nhello\n";
        assert_eq!(rewrite_fences(md, upper), "```upper\nHELLO\n");
    }

    /// KNOWN WART, pinned rather than endorsed: a rewritten body's final line
    /// break comes back as LF even when the document uses CRLF, leaving mixed
    /// endings. `pulldown_cmark` normalizes CRLF to LF in text events, so the
    /// body reports one byte where the source has two, and trimming one byte
    /// off the range consumes the `\r`. Every line break outside the rewritten
    /// body is untouched.
    #[test]
    fn crlf_body_loses_its_carriage_return_at_the_rewritten_line() {
        let md = "```upper\r\nhello\r\n```\r\n";
        assert_eq!(rewrite_fences(md, upper), "```upper\r\nHELLO\n```\r\n");
    }

    /// The closure is `FnMut` so it can accumulate — which is exactly what the
    /// publish path needs for warnings.
    #[test]
    fn closure_may_accumulate_state_across_fences() {
        let md = "```upper\na\n```\n\n```upper\nb\n```\n\n```rust\nc\n```\n";
        let mut seen = Vec::new();
        let _ = rewrite_fences(md, |language, source| {
            seen.push(format!("{language}:{source}"));
            None
        });
        assert_eq!(seen, ["upper:a", "upper:b", "rust:c"]);
    }
}
