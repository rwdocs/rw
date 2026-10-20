//! The YAML block that opens a page, split off before cmark sees the rest.
//!
//! cmark's metadata blocks start wherever a block can, so a `---` line, text,
//! and another `---` line anywhere in a page would vanish as metadata. rw
//! leaves that option off, and every `---` cmark sees is a thematic break or a
//! setext underline.

/// Split `markdown` into its frontmatter and the rest, after an optional BOM.
///
/// Frontmatter must be the first thing in the file. It follows cmark's
/// metadata block rules: a `---` line, a first line that is neither blank nor
/// a closer, and a closing `---` or `...` line. Its text keeps the source's
/// line endings.
pub(crate) fn split(markdown: &str) -> (Option<&str>, &str) {
    let markdown = markdown.strip_prefix('\u{feff}').unwrap_or(markdown);
    let Some((opener, yaml)) = markdown.split_once('\n') else {
        return (None, markdown);
    };
    if opener
        .strip_prefix("---")
        .is_none_or(|rest| !rest.trim_ascii().is_empty())
    {
        return (None, markdown);
    }
    let mut len = 0;
    for line in yaml.split_inclusive('\n') {
        if is_closer(line) {
            if len == 0 {
                break;
            }
            return (Some(&yaml[..len]), &yaml[len + line.len()..]);
        }
        if len == 0 && line.trim_ascii().is_empty() {
            break;
        }
        len += line.len();
    }
    (None, markdown)
}

fn is_closer(line: &str) -> bool {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    matches!(line.trim_end_matches(' '), "---" | "...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_closed_block_at_the_start_is_frontmatter() {
        for (md, frontmatter, rest) in [
            ("---\na: 1\n---\nBody\n", Some("a: 1\n"), "Body\n"),
            ("---  \na: 1\n...\n", Some("a: 1\n"), ""),
            ("---\na: 1\n\nb: 2\n--- ", Some("a: 1\n\nb: 2\n"), ""),
            ("\u{feff}---\na: 1\n---\n", Some("a: 1\n"), ""),
            ("---\r\na: 1\r\n---\r\nBody", Some("a: 1\r\n"), "Body"),
            ("\u{feff}Body", None, "Body"),
            ("---\n---\nBody", None, "---\n---\nBody"),
            ("---\n\na: 1\n---\n", None, "---\n\na: 1\n---\n"),
            ("---\na: 1\n", None, "---\na: 1\n"),
            ("----\na: 1\n----\n", None, "----\na: 1\n----\n"),
            ("--- x\na: 1\n---\n", None, "--- x\na: 1\n---\n"),
            ("---\na: 1\n ---\n", None, "---\na: 1\n ---\n"),
            ("---", None, "---"),
            ("\n---\na: 1\n---\n", None, "\n---\na: 1\n---\n"),
            (
                "<!-- x -->\n---\na: 1\n---\n",
                None,
                "<!-- x -->\n---\na: 1\n---\n",
            ),
        ] {
            assert_eq!(split(md), (frontmatter, rest), "{md:?}");
        }
    }
}
