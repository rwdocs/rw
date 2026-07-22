//! Directive syntax parsing.
//!
//! Parses `CommonMark` directive syntax: `:name`, `::name`, `:::name`

use std::ops::Range;

use super::DirectiveArgs;

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
        directive: Directive,
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

/// Parse a line for an inline directive (`:name`).
///
/// Walks colon runs from left to right and returns the first that opens a
/// valid inline directive. To stay out of the way of prose punctuation, the
/// scanner only considers a single colon as a directive opener when it sits
/// at a word boundary — start-of-line or preceded by a non-name character.
/// Colons embedded inside an alphanumeric run (`9:30`, `14:30:45`, URL
/// schemes like `https:`, or qualified identifiers like `pkg:mod`) are
/// skipped so prose containing them does not dispatch unintended directives.
/// Multi-colon runs (`::`, `:::`, …) are skipped wholesale — those belong
/// to leaf / container tokens. Returns `None` when no inline directive
/// remains in the input.
pub fn parse_line(line: &str) -> Option<InlineMatch> {
    let mut search_from = 0;
    loop {
        let rel = line[search_from..].find(':')?;
        let abs = search_from + rel;
        let count = line[abs..].chars().take_while(|&c| c == ':').count();

        if count != 1 {
            search_from = abs + count;
            continue;
        }

        if is_directive_boundary(line, abs)
            && let Some(matched) = try_parse_inline_at(line, abs)
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
/// by a valid directive name. Callers should advance past the colon and keep
/// scanning.
fn try_parse_inline_at(line: &str, start: usize) -> Option<InlineMatch> {
    let mut pos = start + 1;
    let after_colon = &line[pos..];

    let name_end = after_colon
        .find(|c: char| c == '[' || c == '{' || c.is_whitespace())
        .unwrap_or(after_colon.len());

    let name = &after_colon[..name_end];
    if name.is_empty() || !is_valid_directive_name(name) {
        return None;
    }

    pos += name_end;

    let (content, content_consumed) = parse_brackets(&line[pos..]);
    pos += content_consumed;

    let (attrs_str, attrs_consumed) = parse_braces(&line[pos..]);
    pos += attrs_consumed;

    let args = DirectiveArgs::parse(&content, &attrs_str);

    Some(InlineMatch {
        directive: Directive {
            name: name.to_owned(),
            args,
        },
        range: start..pos,
    })
}

/// Parse a whole line for a leaf directive.
///
/// Used for leaf-style directives that take the entire line.
/// Returns `None` if the line is not a leaf directive (e.g., `:::name` is a container).
pub(crate) fn parse_leaf_line(line: &str) -> Option<Directive> {
    let trimmed = line.trim();

    // Must start with exactly two colons; three or more is a container
    if !trimmed.starts_with("::") || trimmed.starts_with(":::") {
        return None;
    }

    let after_colons = &trimmed[2..];

    // Parse name
    let name_end = after_colons
        .find(|c: char| c == '[' || c == '{' || c.is_whitespace())
        .unwrap_or(after_colons.len());

    let name = &after_colons[..name_end];
    if name.is_empty() || !is_valid_directive_name(name) {
        return None;
    }

    let after_name = &after_colons[name_end..];

    // Parse content and attributes
    let (content, content_consumed) = parse_brackets(after_name);
    let after_content = &after_name[content_consumed..];
    let (attrs_str, attrs_consumed) = parse_braces(after_content);
    let after_attrs = &after_content[attrs_consumed..];

    // The rest of the (trimmed) line must be empty — leaf consumes the whole line
    if !after_attrs.trim().is_empty() {
        return None;
    }

    let args = DirectiveArgs::parse(&content, &attrs_str);

    Some(Directive {
        name: name.to_owned(),
        args,
    })
}

/// Check if a name is a valid directive name.
///
/// Valid names contain only alphanumeric characters, hyphens, and underscores.
fn is_valid_directive_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_name_char)
}

/// Parse content from brackets: `[content]`
///
/// Returns (content, `bytes_consumed`).
fn parse_brackets(s: &str) -> (String, usize) {
    if !s.starts_with('[') {
        return (String::new(), 0);
    }

    // Find matching closing bracket, handling nesting
    let mut depth = 0;
    let mut end = None;

    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    match end {
        Some(end_idx) => {
            let content = &s[1..end_idx];
            (content.to_owned(), end_idx + 1)
        }
        None => (String::new(), 0),
    }
}

/// Parse attributes from braces: `{#id .class key="value"}`
///
/// Returns (`attrs_str` without braces, `bytes_consumed`).
fn parse_braces(s: &str) -> (String, usize) {
    if !s.starts_with('{') {
        return (String::new(), 0);
    }

    // Find matching closing brace, handling nesting
    let mut depth = 0;
    let mut end = None;

    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    match end {
        Some(end_idx) => {
            let attrs = &s[1..end_idx];
            (attrs.to_owned(), end_idx + 1)
        }
        None => (String::new(), 0),
    }
}

/// Parse a whole line for a container directive.
///
/// Used for container-style directives that take the entire line.
/// Returns `None` if the line is not a container directive.
pub(crate) fn parse_container_line(line: &str) -> Option<ContainerLine> {
    let trimmed = line.trim();

    if !trimmed.starts_with(":::") {
        return None;
    }

    let colon_count = trimmed.chars().take_while(|&c| c == ':').count();
    let after_colons = trimmed[colon_count..].trim();

    // Container end
    if after_colons.is_empty() {
        return Some(ContainerLine::End { colon_count });
    }

    // Parse name
    let name_end = after_colons
        .find(|c: char| c == '[' || c == '{' || c.is_whitespace())
        .unwrap_or(after_colons.len());

    let name = &after_colons[..name_end];
    if name.is_empty() || !is_valid_directive_name(name) {
        return None;
    }

    let after_name = &after_colons[name_end..];

    // Parse content and attributes
    let (content, content_consumed) = parse_brackets(after_name);
    let after_content = &after_name[content_consumed..];
    let (attrs_str, _) = parse_braces(after_content);

    let args = DirectiveArgs::parse(&content, &attrs_str);

    Some(ContainerLine::Start {
        directive: Directive {
            name: name.to_owned(),
            args,
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
        // parse_line now returns None for :: (leaf) sequences
        assert!(parse_line("::youtube[dQw4w9WgXcQ]").is_none());
    }

    #[test]
    fn test_leaf_with_attrs_no_longer_inline() {
        // parse_line now returns None for :: (leaf) sequences
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

    // -- Issue #390: directives after a non-directive single colon --------

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
        match parse_container_line("::: note").unwrap() {
            ContainerLine::Start {
                directive,
                colon_count,
            } => {
                assert_eq!(directive.name, "note");
                assert_eq!(directive.args.content, "");
                assert_eq!(colon_count, 3);
            }
            ContainerLine::End { .. } => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_with_content() {
        match parse_container_line(":::tab[macOS]").unwrap() {
            ContainerLine::Start { directive, .. } => {
                assert_eq!(directive.name, "tab");
                assert_eq!(directive.args.content, "macOS");
            }
            ContainerLine::End { .. } => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_with_content_and_attrs() {
        match parse_container_line(":::tab[macOS]{#os .wide}").unwrap() {
            ContainerLine::Start { directive, .. } => {
                assert_eq!(directive.name, "tab");
                assert_eq!(directive.args.content, "macOS");
                assert_eq!(directive.args.id(), Some("os"));
                assert_eq!(directive.args.classes(), ["wide"]);
            }
            ContainerLine::End { .. } => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_with_brackets() {
        match parse_container_line("::: details[Click to expand]").unwrap() {
            ContainerLine::Start { directive, .. } => {
                assert_eq!(directive.name, "details");
                assert_eq!(directive.args.content, "Click to expand");
            }
            ContainerLine::End { .. } => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_end() {
        match parse_container_line(":::").unwrap() {
            ContainerLine::End { colon_count } => assert_eq!(colon_count, 3),
            ContainerLine::Start { .. } => panic!("expected container end"),
        }
    }

    #[test]
    fn test_container_end_with_more_colons() {
        match parse_container_line("::::").unwrap() {
            ContainerLine::End { colon_count } => assert_eq!(colon_count, 4),
            ContainerLine::Start { .. } => panic!("expected container end"),
        }
    }

    #[test]
    fn test_not_directive() {
        assert!(parse_line("regular text").is_none());
        assert!(parse_line("").is_none());
        assert!(parse_container_line("not a directive").is_none());
    }

    #[test]
    fn test_invalid_name() {
        // Name with invalid characters
        assert!(parse_line(":foo@bar[content]").is_none());
        // Empty name
        assert!(parse_line(":[content]").is_none());
    }

    #[test]
    fn test_parse_brackets() {
        assert_eq!(parse_brackets("[hello]"), ("hello".to_owned(), 7));
        assert_eq!(parse_brackets("[hello] rest"), ("hello".to_owned(), 7));
        assert_eq!(
            parse_brackets("[nested [brackets]]"),
            ("nested [brackets]".to_owned(), 19)
        );
        assert_eq!(parse_brackets("no brackets"), (String::new(), 0));
        assert_eq!(parse_brackets("[unclosed"), (String::new(), 0));
    }

    #[test]
    fn test_parse_braces() {
        assert_eq!(parse_braces("{#id}"), ("#id".to_owned(), 5));
        assert_eq!(parse_braces("{.class} rest"), (".class".to_owned(), 8));
        assert_eq!(parse_braces("no braces"), (String::new(), 0));
        assert_eq!(parse_braces("{unclosed"), (String::new(), 0));
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
            let directive = parse_leaf_line("::youtube[dQw4w9WgXcQ]").unwrap();
            assert_eq!(directive.name, "youtube");
            assert_eq!(directive.args.content, "dQw4w9WgXcQ");
        }

        #[test]
        fn leaf_with_attrs() {
            let directive = parse_leaf_line("::include[snippet.md]{#code .highlight}").unwrap();
            assert_eq!(directive.name, "include");
            assert_eq!(directive.args.content, "snippet.md");
            assert_eq!(directive.args.id, Some("code".to_owned()));
            assert_eq!(directive.args.classes, vec!["highlight"]);
        }

        #[test]
        fn leading_whitespace_tolerated() {
            let directive =
                parse_leaf_line("  ::youtube[x]").expect("leading whitespace should be accepted");
            assert_eq!(directive.name, "youtube");
        }

        #[test]
        fn trailing_whitespace_tolerated() {
            let result = parse_leaf_line("::youtube[x]   ");
            assert!(result.is_some(), "trailing whitespace should be accepted");
        }

        #[test]
        fn trailing_non_whitespace_rejected() {
            // "::foo[x] bar" — extra text after the directive → None
            assert!(parse_leaf_line("::foo[x] bar").is_none());
        }

        #[test]
        fn three_colons_rejected() {
            // ":::note" is a container, not a leaf
            assert!(parse_leaf_line(":::note").is_none());
        }

        #[test]
        fn one_colon_rejected() {
            // ":kbd[X]" is inline, not a leaf
            assert!(parse_leaf_line(":kbd[X]").is_none());
        }

        #[test]
        fn not_at_start_rejected() {
            // "text ::foo[x]" — leaf must occupy the whole line
            assert!(parse_leaf_line("text ::foo[x]").is_none());
        }
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
