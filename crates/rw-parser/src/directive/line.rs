//! Directive syntax parsing.
//!
//! Parses `CommonMark` directive syntax: `:name`, `::name`, `:::name`

use std::ops::Range;

use super::DirectiveArgs;
use crate::event::BlockDirectivePayload;

/// One directive occurrence: its name and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub name: String,
    pub args: DirectiveArgs,
}

/// What a `:::` line does.
#[derive(Debug)]
pub(crate) enum ContainerLine {
    /// `:::name[content]{attrs}` — opens a container.
    Start {
        payload: BlockDirectivePayload,
        /// Leading colon count. Three or more: fewer colons never reach
        /// [`parse_container_line`]'s opener branch.
        colon_count: usize,
    },
    /// `:::` — closes the innermost open container.
    End { colon_count: usize },
}

/// An inline directive found in a line, and the byte range it occupies.
///
/// `range` is absolute within the line handed to [`parse_line`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineMatch {
    pub directive: Directive,
    pub range: Range<usize>,
}

/// Where a line holds characters cmark unescaped from a backslash escape.
///
/// Such a character is content: an escaped `:` opens no directive, and an
/// escaped bracket or brace neither opens nor closes a group.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Escapes<'e> {
    /// Sorted byte offsets into the buffer the line was cut from.
    offsets: &'e [usize],
    /// Where the line starts in that buffer.
    base: usize,
}

impl<'e> Escapes<'e> {
    pub(crate) fn new(offsets: &'e [usize], base: usize) -> Self {
        Self { offsets, base }
    }

    fn contains(self, i: usize) -> bool {
        self.offsets.binary_search(&(self.base + i)).is_ok()
    }

    fn any_in(self, range: Range<usize>) -> bool {
        range.into_iter().any(|i| self.contains(i))
    }

    /// The same escapes, for the line with its first `n` bytes cut off.
    fn skip(self, n: usize) -> Self {
        Self {
            base: self.base + n,
            ..self
        }
    }
}

/// Parse a line for an inline directive (`:name`).
///
/// Walks colon runs from left to right and returns the first that opens a
/// valid inline directive: a name followed directly by a closed `[...]` or
/// `{...}` group. To stay out of the way of prose punctuation, the scanner
/// only considers a single colon as a directive opener when it sits at a word
/// boundary — start-of-line or preceded by a non-name character.
/// Colons embedded inside an alphanumeric run (`9:30`, `14:30:45`, URL
/// schemes like `https:`, or qualified identifiers like `pkg:mod`) are
/// skipped so prose containing them does not dispatch unintended directives.
/// Multi-colon runs (`::`, `:::`, …) are skipped wholesale — those belong
/// to leaf / container tokens. Returns `None` when no inline directive
/// remains in the input.
pub fn parse_line(line: &str) -> Option<InlineMatch> {
    parse_escaped_line(line, Escapes::default())
}

/// [`parse_line`] for a line whose `escapes` are content.
pub(crate) fn parse_escaped_line(line: &str, escapes: Escapes<'_>) -> Option<InlineMatch> {
    let mut search_from = 0;
    loop {
        let rel = line[search_from..].find(':')?;
        let abs = search_from + rel;
        let count = line[abs..].chars().take_while(|&c| c == ':').count();

        // An escaped colon still counts toward the run, so `\::name[x]`
        // stays literal instead of becoming `:name[x]`.
        if count != 1 || escapes.contains(abs) {
            search_from = abs + count;
            continue;
        }

        if is_directive_boundary(line, abs)
            && let Some(matched) = try_parse_inline_at(line, abs, escapes)
        {
            return Some(matched);
        }

        // Colon is mid-word (`9:30`) or starts an invalid name — step past
        // it and keep scanning.
        search_from = abs + 1;
    }
}

/// True when a colon at byte offset `colon_pos` looks like the opener of an
/// inline directive rather than punctuation embedded in a word.
///
/// The opener is accepted at start-of-line or when the preceding character
/// is anything other than a directive-name character (alphanumeric, `-`,
/// `_`). That keeps `:kbd[X]`, ` :kbd[X]`, `(:kbd[X])` working while
/// rejecting `9:30`, `pkg:mod`, and `:foo:bar`'s second colon.
fn is_directive_boundary(line: &str, colon_pos: usize) -> bool {
    match line[..colon_pos].chars().next_back() {
        None => true,
        Some(prev) => !is_name_char(prev),
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// Attempt to parse an inline directive whose opening colon sits at `start`.
///
/// Returns `None` (without consuming anything) when the colon is not followed
/// by a valid directive name and a closed `[...]` or `{...}` group. Callers
/// should advance past the colon and keep scanning.
fn try_parse_inline_at(line: &str, start: usize, escapes: Escapes<'_>) -> Option<InlineMatch> {
    let name_start = start + 1;
    let (name, args, consumed) = name_and_args(&line[name_start..], escapes.skip(name_start))?;
    // A group is what tells `:kbd[X]` from prose such as `:id`, `:D` or a
    // Ruby symbol.
    if consumed == name.len() {
        return None;
    }

    Some(InlineMatch {
        directive: Directive {
            name: name.to_owned(),
            args,
        },
        range: start..name_start + consumed,
    })
}

/// Parse a whole line for a leaf directive.
///
/// Used for leaf-style directives that take the entire line.
/// Returns `None` if the line is not a leaf directive (e.g., `:::name` is a container).
pub(crate) fn parse_leaf_line(line: &str, escapes: Escapes<'_>) -> Option<BlockDirectivePayload> {
    let trimmed = line.trim();

    // Must start with exactly two colons; three or more is a container
    if !trimmed.starts_with("::") || trimmed.starts_with(":::") {
        return None;
    }
    let escapes = escapes.skip(line.len() - line.trim_start().len());
    if escapes.any_in(0..2) {
        return None;
    }

    let after_colons = &trimmed[2..];
    let (name, args, consumed) = name_and_args(after_colons, escapes.skip(2))?;

    // The rest of the (trimmed) line must be empty — leaf consumes the whole line
    if !after_colons[consumed..].trim().is_empty() {
        return None;
    }

    Some(BlockDirectivePayload {
        name: name.to_owned(),
        args,
        after_colons: after_colons.to_owned(),
    })
}

/// Parse `name[content]{attrs}` at the start of `s`; both groups are optional.
///
/// Returns the name, its arguments, and the bytes consumed.
fn name_and_args<'s>(s: &'s str, escapes: Escapes<'_>) -> Option<(&'s str, DirectiveArgs, usize)> {
    let name_end = s
        .find(|c: char| c == '[' || c == '{' || c.is_whitespace())
        .unwrap_or(s.len());

    let name = &s[..name_end];
    if !is_valid_directive_name(name) {
        return None;
    }

    let mut pos = name_end;
    let (content, consumed) = delimited(&s[pos..], escapes.skip(pos), '[', ']').unwrap_or_default();
    pos += consumed;
    let (attrs, consumed) = delimited(&s[pos..], escapes.skip(pos), '{', '}').unwrap_or_default();
    pos += consumed;

    Some((name, DirectiveArgs::parse(content, attrs), pos))
}

/// Check if a name is a valid directive name.
///
/// Valid names contain only alphanumeric characters, hyphens, and underscores.
fn is_valid_directive_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_name_char)
}

/// Split a balanced `open … close` group off the start of `s`.
///
/// Returns the text between the outermost delimiters and the bytes consumed,
/// or `None` when `s` does not start with `open` or the group never closes.
fn delimited<'s>(
    s: &'s str,
    escapes: Escapes<'_>,
    open: char,
    close: char,
) -> Option<(&'s str, usize)> {
    if !s.starts_with(open) || escapes.contains(0) {
        return None;
    }

    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        if c == open && !escapes.contains(i) {
            depth += 1;
        } else if c == close && !escapes.contains(i) {
            depth -= 1;
            if depth == 0 {
                return Some((&s[open.len_utf8()..i], i + close.len_utf8()));
            }
        }
    }
    None
}

/// Parse a whole line for a container directive.
///
/// Used for container-style directives that take the entire line.
/// Returns `None` if the line is not a container directive.
pub(crate) fn parse_container_line(line: &str, escapes: Escapes<'_>) -> Option<ContainerLine> {
    let trimmed = line.trim();

    if !trimmed.starts_with(":::") {
        return None;
    }

    let colon_count = trimmed.chars().take_while(|&c| c == ':').count();
    let escapes = escapes.skip(line.len() - line.trim_start().len());
    if escapes.any_in(0..colon_count) {
        return None;
    }
    let after_colons = &trimmed[colon_count..];
    let directive = after_colons.trim_start();

    // Container end
    if directive.is_empty() {
        return Some(ContainerLine::End { colon_count });
    }

    // Unlike a leaf, an opener tolerates trailing text: it is ignored here
    // and kept in `after_colons`.
    let (name, args, _) = name_and_args(directive, escapes.skip(trimmed.len() - directive.len()))?;

    Some(ContainerLine::Start {
        payload: BlockDirectivePayload {
            name: name.to_owned(),
            args,
            after_colons: after_colons.to_owned(),
        },
        colon_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_directive() {
        let matched = parse_line("Press :kbd[Ctrl+C] to copy.").unwrap();

        assert_eq!(matched.range, 6..18);
        assert_eq!(matched.directive.name, "kbd");
        assert_eq!(matched.directive.args.content, "Ctrl+C");
    }

    #[test]
    fn test_inline_with_attrs() {
        let matched = parse_line(r#":abbr[HTML]{title="HyperText Markup Language"}"#).unwrap();

        assert_eq!(matched.directive.name, "abbr");
        assert_eq!(matched.directive.args.content, "HTML");
        assert_eq!(
            matched.directive.args.get("title"),
            Some("HyperText Markup Language")
        );
    }

    #[test]
    fn test_leaf_directive_no_longer_inline() {
        assert!(parse_line("::youtube[dQw4w9WgXcQ]").is_none());
    }

    #[test]
    fn test_leaf_with_attrs_no_longer_inline() {
        assert!(parse_line("::include[snippet.md]{#code .highlight}").is_none());
    }

    #[test]
    fn test_double_colon_mid_line_no_longer_inline() {
        // parse_line must return None when the only colon run is >=2
        assert!(parse_line("text ::foo[x] more").is_none());
    }

    #[test]
    fn test_inline_directive_after_double_colon_run() {
        // A `::leaf` token at the start of the line must not blind the scanner
        // to a single-colon inline directive that follows on the same line.
        let matched = parse_line("::foo[x] :kbd[Y]").expect("should find :kbd after the :: run");
        assert_eq!(matched.directive.name, "kbd");
        assert_eq!(matched.directive.args.content, "Y");
        assert_eq!(matched.range, 9..16);
    }

    // -- Directives after a non-directive single colon --------

    #[test]
    fn test_inline_directive_after_punctuation_colon() {
        // "Note: " has a colon followed by whitespace — empty directive name.
        // The scanner must skip past it and find :kbd further along.
        let matched = parse_line("Note: press :kbd[Ctrl+C] to copy.")
            .expect("should find :kbd after `Note:`");
        assert_eq!(matched.directive.name, "kbd");
        assert_eq!(matched.directive.args.content, "Ctrl+C");
        assert_eq!(matched.range, 12..24);
    }

    #[test]
    fn test_inline_directive_after_url_scheme() {
        // The `:` in `https:` is followed by `//…`, which is not a valid name.
        // The scanner must keep going and find :cmd.
        let matched = parse_line("See https://example.com then :cmd[deploy]")
            .expect("should find :cmd after the URL scheme colon");
        assert_eq!(matched.directive.name, "cmd");
        assert_eq!(matched.directive.args.content, "deploy");
    }

    #[test]
    fn test_punctuation_colon_with_no_directive() {
        // A line with only a punctuation colon and no directive must still
        // return None — we should not invent directives where none exist.
        assert!(parse_line("Note: nothing to see here.").is_none());
    }

    #[test]
    fn test_time_strings_are_not_directives() {
        // The colon inside a time-of-day is wedged between digits — it is
        // not a word boundary and must not be treated as a directive opener.
        // Otherwise `:30` / `:45` would be dispatched as unknown inline
        // directives and pollute the warnings channel (failing --strict
        // publishes on benign prose).
        assert!(parse_line("Standup at 9:30 sharp").is_none());
        assert!(parse_line("Build started at 14:30:45 UTC").is_none());
        assert!(parse_line("standup: 9:30 then deploy").is_none());
    }

    #[test]
    fn test_qualified_identifier_is_not_a_directive() {
        // `:foo:bar` — second colon is preceded by an alphanumeric char, so
        // it is mid-word and not a directive opener.
        assert!(parse_line(":foo:bar").is_none());
        assert!(parse_line("aspect ratio :1:1 means equal").is_none());
        assert!(parse_line("see pkg:mod for details").is_none());
    }

    #[test]
    fn test_directive_after_open_punctuation() {
        // Colons immediately after non-name punctuation should still open a
        // directive — `(`, `]`, `,`, etc. are all word boundaries.
        let matched = parse_line("(:kbd[X])").expect("colon after `(` should open a directive");
        assert_eq!(matched.directive.name, "kbd");
        assert_eq!(matched.range.start, 1);
    }

    #[test]
    fn test_container_start() {
        match parse_container_line("::: note", Escapes::default()).unwrap() {
            ContainerLine::Start {
                payload,
                colon_count,
            } => {
                assert_eq!(payload.name, "note");
                assert_eq!(payload.args.content, "");
                assert_eq!(payload.after_colons, " note");
                assert_eq!(colon_count, 3);
            }
            ContainerLine::End { .. } => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_with_content() {
        match parse_container_line(":::tab[macOS]", Escapes::default()).unwrap() {
            ContainerLine::Start { payload, .. } => {
                assert_eq!(payload.name, "tab");
                assert_eq!(payload.args.content, "macOS");
            }
            ContainerLine::End { .. } => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_with_content_and_attrs() {
        match parse_container_line(":::tab[macOS]{#os .wide k=v}", Escapes::default()).unwrap() {
            ContainerLine::Start { payload, .. } => {
                assert_eq!(payload.name, "tab");
                assert_eq!(payload.args.content, "macOS");
                assert_eq!(payload.args.get("k"), Some("v"));
            }
            ContainerLine::End { .. } => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_with_brackets() {
        match parse_container_line("::: details[Click to expand]", Escapes::default()).unwrap() {
            ContainerLine::Start { payload, .. } => {
                assert_eq!(payload.name, "details");
                assert_eq!(payload.args.content, "Click to expand");
            }
            ContainerLine::End { .. } => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_keeps_trailing_text() {
        match parse_container_line(
            " ::::note[x]{b=2 a=1} This is important ",
            Escapes::default(),
        )
        .unwrap()
        {
            ContainerLine::Start {
                payload,
                colon_count,
            } => {
                assert_eq!(payload.name, "note");
                assert_eq!(payload.after_colons, "note[x]{b=2 a=1} This is important");
                assert_eq!(colon_count, 4);
            }
            ContainerLine::End { .. } => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_end() {
        match parse_container_line(":::", Escapes::default()).unwrap() {
            ContainerLine::End { colon_count } => assert_eq!(colon_count, 3),
            ContainerLine::Start { .. } => panic!("expected container end"),
        }
    }

    #[test]
    fn test_container_end_with_more_colons() {
        match parse_container_line("::::", Escapes::default()).unwrap() {
            ContainerLine::End { colon_count } => assert_eq!(colon_count, 4),
            ContainerLine::Start { .. } => panic!("expected container end"),
        }
    }

    #[test]
    fn test_not_directive() {
        assert!(parse_line("regular text").is_none());
        assert!(parse_line("").is_none());
        assert!(parse_container_line("not a directive", Escapes::default()).is_none());
    }

    #[test]
    fn test_invalid_name() {
        // Name with invalid characters
        assert!(parse_line(":foo@bar[content]").is_none());
        // Empty name
        assert!(parse_line(":[content]").is_none());
    }

    #[test]
    fn test_delimited_brackets() {
        let brackets = |s| delimited(s, Escapes::default(), '[', ']');
        assert_eq!(brackets("[hello]"), Some(("hello", 7)));
        assert_eq!(brackets("[hello] rest"), Some(("hello", 7)));
        assert_eq!(
            brackets("[nested [brackets]]"),
            Some(("nested [brackets]", 19))
        );
        assert_eq!(brackets("no brackets"), None);
        assert_eq!(brackets("[unclosed"), None);
    }

    #[test]
    fn test_delimited_braces() {
        let braces = |s| delimited(s, Escapes::default(), '{', '}');
        assert_eq!(braces("{#id}"), Some(("#id", 5)));
        assert_eq!(braces("{.class} rest"), Some((".class", 8)));
        assert_eq!(braces("no braces"), None);
        assert_eq!(braces("{unclosed"), None);
    }

    #[test]
    fn test_is_valid_directive_name() {
        assert!(is_valid_directive_name("kbd"));
        assert!(is_valid_directive_name("my-directive"));
        assert!(is_valid_directive_name("directive_name"));
        assert!(is_valid_directive_name("directive123"));
        assert!(!is_valid_directive_name(""));
        assert!(!is_valid_directive_name("foo@bar"));
        assert!(!is_valid_directive_name("foo bar"));
    }

    mod parse_leaf_line_tests {
        use super::*;

        #[test]
        fn bare_leaf() {
            let directive = parse_leaf_line("::youtube[dQw4w9WgXcQ]", Escapes::default()).unwrap();
            assert_eq!(directive.name, "youtube");
            assert_eq!(directive.args.content, "dQw4w9WgXcQ");
        }

        #[test]
        fn leaf_with_attrs() {
            let directive = parse_leaf_line(
                "::include[snippet.md]{#code .highlight lang=rs}",
                Escapes::default(),
            )
            .unwrap();
            assert_eq!(directive.name, "include");
            assert_eq!(directive.args.content, "snippet.md");
            assert_eq!(directive.args.get("lang"), Some("rs"));
        }

        #[test]
        fn leading_whitespace_tolerated() {
            let directive = parse_leaf_line("  ::youtube[x]  ", Escapes::default())
                .expect("leading whitespace should be accepted");
            assert_eq!(directive.name, "youtube");
            assert_eq!(directive.after_colons, "youtube[x]");
        }

        #[test]
        fn trailing_whitespace_tolerated() {
            let result = parse_leaf_line("::youtube[x]   ", Escapes::default());
            assert!(result.is_some(), "trailing whitespace should be accepted");
        }

        #[test]
        fn trailing_non_whitespace_rejected() {
            // "::foo[x] bar" — extra text after the directive → None
            assert!(parse_leaf_line("::foo[x] bar", Escapes::default()).is_none());
        }

        #[test]
        fn three_colons_rejected() {
            // ":::note" is a container, not a leaf
            assert!(parse_leaf_line(":::note", Escapes::default()).is_none());
        }

        #[test]
        fn one_colon_rejected() {
            // ":kbd[X]" is inline, not a leaf
            assert!(parse_leaf_line(":kbd[X]", Escapes::default()).is_none());
        }

        #[test]
        fn not_at_start_rejected() {
            // "text ::foo[x]" — leaf must occupy the whole line
            assert!(parse_leaf_line("text ::foo[x]", Escapes::default()).is_none());
        }
    }

    #[test]
    fn test_inline_directive_requires_a_group() {
        assert!(parse_line(":kbd").is_none());
        assert!(parse_line("Use the :id key.").is_none());
        assert!(parse_line(":kbd[unclosed").is_none());
        assert!(parse_line(":kbd{unclosed").is_none());
        assert!(parse_line(":kbd [X]").is_none());
    }

    #[test]
    fn test_braces_alone_open_a_directive() {
        let matched = parse_line(":abbr{title=x} more").unwrap();
        assert_eq!(matched.directive.name, "abbr");
        assert_eq!(matched.directive.args.get("title"), Some("x"));
        assert_eq!(matched.range, 0..14);
    }

    #[test]
    fn test_emoji_shortcodes_are_not_directives() {
        assert!(parse_line(":smile:").is_none());
        assert!(parse_line(":+1:").is_none());
        assert!(parse_line(":smile:[x]").is_none());
    }

    #[test]
    fn test_colon_after_a_colon_is_not_an_opener() {
        assert!(parse_line("a ::b[x]").is_none());
        assert!(parse_line("a ::b{x}").is_none());
    }

    #[test]
    fn test_empty_brackets_open_a_directive() {
        let matched = parse_line(":br[]").unwrap();
        assert_eq!(matched.directive.name, "br");
        assert_eq!(matched.range, 0..5);
    }

    #[test]
    fn test_bare_name_does_not_hide_a_later_directive() {
        let matched = parse_line("Use :id then :kbd[X]").unwrap();
        assert_eq!(matched.directive.name, "kbd");
        assert_eq!(matched.range, 13..20);
    }

    #[test]
    fn test_directive_at_start() {
        let matched = parse_line(":kbd[X]").unwrap();
        assert_eq!(matched.range.start, 0);
    }

    #[test]
    fn test_multiple_directives_finds_first() {
        let matched = parse_line(":a[1] :b[2]").unwrap();
        assert_eq!(matched.range.start, 0);
        assert_eq!(matched.directive.name, "a");
        assert_eq!(matched.directive.args.content, "1");
    }
}
