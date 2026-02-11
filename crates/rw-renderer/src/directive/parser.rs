//! Directive syntax parsing.
//!
//! Parses `CommonMark` directive syntax: `:name`, `::name`, `:::name`

use super::DirectiveArgs;

/// Parsed directive from a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParsedDirective {
    /// Inline directive: `:name[content]{attrs}`
    Inline { name: String, args: DirectiveArgs },
    /// Leaf directive: `::name[content]{attrs}`
    Leaf { name: String, args: DirectiveArgs },
    /// Container opening: `:::name[content]{attrs}`
    ContainerStart {
        name: String,
        args: DirectiveArgs,
        colon_count: usize,
    },
    /// Container closing: `:::`
    ContainerEnd { colon_count: usize },
}

/// Parse a line for directive syntax.
///
/// Returns `None` if the line doesn't contain a directive.
pub(crate) fn parse_line(line: &str) -> Option<(ParsedDirective, usize, usize)> {
    // Find first colon that might start a directive
    let start = line.find(':')?;

    // Count colons
    let colon_count = line[start..].chars().take_while(|&c| c == ':').count();

    if colon_count < 1 {
        return None;
    }

    let mut pos = start + colon_count;
    let after_colons = &line[pos..];

    // Container end: just colons (with optional whitespace after)
    if colon_count >= 3 && after_colons.trim().is_empty() {
        return Some((
            ParsedDirective::ContainerEnd { colon_count },
            start,
            line.len(),
        ));
    }

    // Parse name - name ends at [, {, or whitespace
    let name_end = after_colons
        .find(|c: char| c == '[' || c == '{' || c.is_whitespace())
        .unwrap_or(after_colons.len());

    let name = &after_colons[..name_end];
    if name.is_empty() || !is_valid_directive_name(name) {
        return None;
    }

    pos += name_end;

    // Parse content in brackets [...]
    let (content, content_consumed) = parse_brackets(&line[pos..]);
    pos += content_consumed;

    // Parse attributes in braces {...}
    let (attrs_str, attrs_consumed) = parse_braces(&line[pos..]);
    pos += attrs_consumed;

    let args = DirectiveArgs::parse(&content, &attrs_str);

    let directive = match colon_count {
        1 => ParsedDirective::Inline {
            name: name.to_owned(),
            args,
        },
        2 => ParsedDirective::Leaf {
            name: name.to_owned(),
            args,
        },
        _ => ParsedDirective::ContainerStart {
            name: name.to_owned(),
            args,
            colon_count,
        },
    };

    Some((directive, start, pos))
}

/// Check if a name is a valid directive name.
///
/// Valid names contain only alphanumeric characters, hyphens, and underscores.
fn is_valid_directive_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
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
pub(crate) fn parse_container_line(line: &str) -> Option<ParsedDirective> {
    let trimmed = line.trim();

    if !trimmed.starts_with(":::") {
        return None;
    }

    let colon_count = trimmed.chars().take_while(|&c| c == ':').count();
    let after_colons = trimmed[colon_count..].trim();

    // Container end
    if after_colons.is_empty() {
        return Some(ParsedDirective::ContainerEnd { colon_count });
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

    Some(ParsedDirective::ContainerStart {
        name: name.to_owned(),
        args,
        colon_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_directive() {
        let result = parse_line("Press :kbd[Ctrl+C] to copy.");
        let (directive, start, end) = result.unwrap();

        assert_eq!(start, 6);
        assert_eq!(end, 18);
        match directive {
            ParsedDirective::Inline { name, args } => {
                assert_eq!(name, "kbd");
                assert_eq!(args.content, "Ctrl+C");
            }
            _ => panic!("expected inline directive"),
        }
    }

    #[test]
    fn test_inline_with_attrs() {
        let result = parse_line(r#":abbr[HTML]{title="HyperText Markup Language"}"#);
        let (directive, _, _) = result.unwrap();

        match directive {
            ParsedDirective::Inline { name, args } => {
                assert_eq!(name, "abbr");
                assert_eq!(args.content, "HTML");
                assert_eq!(args.get("title"), Some("HyperText Markup Language"));
            }
            _ => panic!("expected inline directive"),
        }
    }

    #[test]
    fn test_leaf_directive() {
        let result = parse_line("::youtube[dQw4w9WgXcQ]");
        let (directive, _, _) = result.unwrap();

        match directive {
            ParsedDirective::Leaf { name, args } => {
                assert_eq!(name, "youtube");
                assert_eq!(args.content, "dQw4w9WgXcQ");
            }
            _ => panic!("expected leaf directive"),
        }
    }

    #[test]
    fn test_leaf_with_attrs() {
        let result = parse_line("::include[snippet.md]{#code .highlight}");
        let (directive, _, _) = result.unwrap();

        match directive {
            ParsedDirective::Leaf { name, args } => {
                assert_eq!(name, "include");
                assert_eq!(args.content, "snippet.md");
                assert_eq!(args.id, Some("code".to_owned()));
                assert_eq!(args.classes, vec!["highlight"]);
            }
            _ => panic!("expected leaf directive"),
        }
    }

    #[test]
    fn test_container_start() {
        let result = parse_container_line("::: note");
        let directive = result.unwrap();

        match directive {
            ParsedDirective::ContainerStart {
                name,
                args,
                colon_count,
            } => {
                assert_eq!(name, "note");
                assert_eq!(args.content, "");
                assert_eq!(colon_count, 3);
            }
            _ => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_with_content() {
        let result = parse_container_line(":::tab[macOS]");
        let directive = result.unwrap();

        match directive {
            ParsedDirective::ContainerStart { name, args, .. } => {
                assert_eq!(name, "tab");
                assert_eq!(args.content, "macOS");
            }
            _ => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_with_brackets() {
        let result = parse_container_line("::: details[Click to expand]");
        let directive = result.unwrap();

        match directive {
            ParsedDirective::ContainerStart { name, args, .. } => {
                assert_eq!(name, "details");
                assert_eq!(args.content, "Click to expand");
            }
            _ => panic!("expected container start"),
        }
    }

    #[test]
    fn test_container_end() {
        let result = parse_container_line(":::");
        let directive = result.unwrap();

        match directive {
            ParsedDirective::ContainerEnd { colon_count } => {
                assert_eq!(colon_count, 3);
            }
            _ => panic!("expected container end"),
        }
    }

    #[test]
    fn test_container_end_with_more_colons() {
        let result = parse_container_line("::::");
        let directive = result.unwrap();

        match directive {
            ParsedDirective::ContainerEnd { colon_count } => {
                assert_eq!(colon_count, 4);
            }
            _ => panic!("expected container end"),
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

    #[test]
    fn test_directive_at_start() {
        let result = parse_line(":kbd[X]");
        assert!(result.is_some());
        let (_, start, _) = result.unwrap();
        assert_eq!(start, 0);
    }

    #[test]
    fn test_multiple_directives_finds_first() {
        let result = parse_line(":a[1] :b[2]");
        let (directive, start, _) = result.unwrap();
        assert_eq!(start, 0);
        match directive {
            ParsedDirective::Inline { name, args } => {
                assert_eq!(name, "a");
                assert_eq!(args.content, "1");
            }
            _ => panic!("expected inline"),
        }
    }
}
