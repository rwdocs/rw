//! Source-to-source rewrites for publishing a document somewhere the files
//! it references cannot be read.
//!
//! [`rewrite`] replaces fence bodies (to inline `PlantUML` `!include`
//! directives) and lone images with fences (to embed diagram files) in one
//! pass, leaving the rest of the document as written.

use std::borrow::Cow;
use std::ops::Range;

use crate::{CodeBlockPayload, Event, Parser, Tag, TagEnd};

/// What [`rewrite`] replaces. Both methods leave everything as it was by
/// default.
pub trait Rewrite {
    /// The new body of a fenced code block, or `None` to leave it exactly as
    /// it was.
    ///
    /// Receives the fence's language (the first word of the info string) and
    /// its body without the trailing newline. Never offered: fences with no
    /// language, indented code blocks (they have no info string), and fences
    /// whose body is empty or only blank lines.
    ///
    /// Only the body is replaced. Fence markers, the info string with its
    /// `{…}` attributes, and everything around the fence stay byte for byte.
    /// Every line of the replacement gets the body's container prefix (`> `,
    /// a list item's indentation) and line ending, so the fence stays inside
    /// its blockquote or list item.
    fn fence(&mut self, language: &str, body: &str) -> Option<String> {
        let _ = (language, body);
        None
    }

    /// A fence `(language, body)` to put in place of an image that is alone
    /// in its paragraph, or `None` to leave the image exactly as it was.
    ///
    /// Receives the image's destination and its alt text as the renderer
    /// collects it: the alt's text and inline code, markup dropped, line
    /// breaks as spaces. An inline directive in the alt contributes its
    /// source as written. The destination has its backslash escapes and
    /// entities decoded, as cmark decodes them, but not its percent-escapes:
    /// a caller resolving it to a file decodes those, as the renderer does.
    ///
    /// The fence is `` ```language {alt="…"} `` over `body`, and its `alt`
    /// parses back to the image's alt text, except that an info string holds
    /// no line break and an attribute block ends at the first `}`: line
    /// breaks become spaces and `}` is dropped. When both quote kinds appear
    /// before whitespace, no quote could close the value, so `"` is dropped
    /// too. An empty alt writes no attribute. `language` must be one word with
    /// no `{`. The fence is written as returned, not offered to
    /// [`fence`](Self::fence).
    ///
    /// Only the image's lines are replaced: in `:::tab[A]\n![a](x.puml)\n:::`,
    /// one cmark paragraph, the directive lines stay, and the fence interrupts
    /// the paragraph so they still parse as before. The fence stays in the
    /// image's blockquote or list item.
    fn lone_image(&mut self, dest: &str, alt: &str) -> Option<(String, String)> {
        let _ = (dest, alt);
        None
    }
}

/// A [`Rewrite`] of fence bodies only, by a closure taking the fence's
/// language and body.
pub struct Fences<F>(pub F)
where
    F: FnMut(&str, &str) -> Option<String>;

impl<F> Rewrite for Fences<F>
where
    F: FnMut(&str, &str) -> Option<String>,
{
    fn fence(&mut self, language: &str, body: &str) -> Option<String> {
        (self.0)(language, body)
    }
}

/// Rewrite `markdown`'s fences and lone images as `rewriter` asks, leaving
/// the rest of it as written.
///
/// `wikilinks` and `directives` are [`Parser::new`]'s, and must be what the
/// rewritten document will be rendered with: they decide which fences and
/// lone images it has. `![[x]]` is an image only with wikilinks, and only a
/// `:::` line split out of a paragraph can leave an image alone in what
/// remains of it. Nothing in the frontmatter is offered, nor an image in code
/// or HTML.
///
/// A document with no `` ``` ``, `~~~` or `![` has neither, and comes back
/// borrowed without being parsed.
///
/// # Examples
///
/// ```
/// use rw_parser::{Fences, rewrite};
///
/// let md = "```upper\nhello\n```\n";
/// let out = rewrite(md, true, true, &mut Fences(|lang, body| {
///     (lang == "upper").then(|| body.to_uppercase())
/// }));
/// assert_eq!(out, "```upper\nHELLO\n```\n");
/// ```
///
/// ```
/// use rw_parser::{Rewrite, rewrite};
///
/// struct Diagrams;
///
/// impl Rewrite for Diagrams {
///     fn lone_image(&mut self, dest: &str, _alt: &str) -> Option<(String, String)> {
///         (dest == "flow.puml").then(|| ("plantuml".to_owned(), "A -> B\n".to_owned()))
///     }
/// }
///
/// let md = "Intro.\n\n![Flow](flow.puml)\n";
/// let out = rewrite(md, true, true, &mut Diagrams);
/// assert_eq!(out, "Intro.\n\n```plantuml {alt=\"Flow\"}\nA -> B\n```\n");
/// ```
#[must_use]
pub fn rewrite<'a>(
    markdown: &'a str,
    wikilinks: bool,
    directives: bool,
    rewriter: &mut impl Rewrite,
) -> Cow<'a, str> {
    enum Step {
        Open,
        Close,
        Fence(String),
        LoneImage,
        Other,
    }

    if !has_marker(markdown) {
        return Cow::Borrowed(markdown);
    }
    let mut parser = Parser::new(markdown, wikilinks, directives);
    let source = parser.source();
    let source_at = markdown.len() - source.len();
    // Where each open blockquote and list item starts, innermost last.
    let mut containers = Vec::new();
    let mut replacements = Vec::new();
    loop {
        // Classified first: the event borrows `parser` until it is dropped.
        let step = match parser.next() {
            None => break,
            Some(Event::Start(Tag::BlockQuote(_) | Tag::Item)) => Step::Open,
            Some(Event::End(TagEnd::BlockQuote(_) | TagEnd::Item)) => Step::Close,
            Some(Event::CodeBlock(CodeBlockPayload {
                language: Some(language),
                source,
                ..
            })) if source.contains(|c| c != '\n' && c != '\r') => {
                let body = source.strip_suffix('\n').unwrap_or(&source);
                rewriter
                    .fence(&language, body)
                    .map_or(Step::Other, Step::Fence)
            }
            Some(Event::Start(Tag::Paragraph { lone_image: true })) => Step::LoneImage,
            Some(_) => Step::Other,
        };
        let (range, text) = match step {
            Step::Open => {
                containers.push(parser.container_start());
                continue;
            }
            Step::Close => {
                containers.pop();
                continue;
            }
            Step::Fence(body) => {
                let code = parser.code_body();
                let first_content = code.first_content.unwrap_or(code.range.start);
                splice(source, &code.range, first_content, &body)
            }
            Step::LoneImage => {
                let paragraph_end = parser.paragraph_end();
                let image = parser
                    .image_range_next()
                    .expect("a lone image's paragraph opens with the image");
                let (dest_url, alt) = take_image(&mut parser);
                let Some((language, body)) = rewriter.lone_image(&dest_url, &alt) else {
                    continue;
                };
                debug_assert!(
                    !language.is_empty()
                        && !language.contains(|c: char| c == '{' || c.is_whitespace()),
                    "a fence language is one word with no `{{`: {language:?}"
                );
                let fence = fence(&language, &alt, &body);
                let container = containers.last().copied();
                splice_image(source, container, &image, paragraph_end, &fence)
            }
            Step::Other => continue,
        };
        replacements.push((range.start + source_at..range.end + source_at, text));
    }
    if replacements.is_empty() {
        return Cow::Borrowed(markdown);
    }
    Cow::Owned(apply(markdown, &replacements))
}

/// Whether `markdown` holds a fence marker or an image's `![`, which can be
/// neither escaped nor encoded: without one it has no fence or image.
fn has_marker(markdown: &str) -> bool {
    ["```", "~~~", "!["].into_iter().any(|marker| {
        // `find(char)` runs `memchr`, which `contains(&str)` does not.
        let lead = char::from(marker.as_bytes()[0]);
        let mut at = 0;
        while let Some(i) = markdown[at..].find(lead) {
            if markdown[at + i..].starts_with(marker) {
                return true;
            }
            at += i + 1;
        }
        false
    })
}

/// The source range a body's replacement covers, and the text to put there.
///
/// Body text leaves out container prefixes (`> `, a list item's indentation),
/// so the body is replaced from the start of its first line and every
/// replacement line gets a prefix back. It is taken from the first non-blank
/// line, at `first_content`: a blank line in a list item has no prefix at all.
fn splice(
    markdown: &str,
    body: &Range<usize>,
    first_content: usize,
    replacement: &str,
) -> (Range<usize>, String) {
    let prefix = continuation(&markdown[line_start(markdown, first_content)..first_content]);
    let start = line_start(markdown, body.start);
    let eol = if markdown[..start].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    // The line break before the closing fence counts as body text; splicing
    // over it would glue the replacement to the fence.
    let end = markdown[..body.end]
        .strip_suffix('\n')
        .map_or(body.end, |s| s.strip_suffix('\r').unwrap_or(s).len());
    let separator = format!("{eol}{prefix}");
    let text = format!("{prefix}{}", replacement.replace('\n', &separator));
    (start..end, text)
}

/// Consume the image `parser` is about to open: its destination, and its alt
/// text as the renderer's walker collects it.
fn take_image(parser: &mut Parser<'_>) -> (String, String) {
    let Some(Event::Start(Tag::Image { dest_url, .. })) = parser.next() else {
        unreachable!("a lone image's paragraph opens with the image");
    };
    let dest_url = dest_url.into_string();
    let mut alt = String::new();
    // The walker drops a nested image whole, its alt text included.
    let mut depth = 1;
    while depth > 0 {
        match parser.next() {
            Some(Event::Start(Tag::Image { .. })) => depth += 1,
            Some(Event::End(TagEnd::Image)) => depth -= 1,
            Some(Event::Text(text) | Event::Code(text)) if depth == 1 => alt.push_str(&text),
            Some(Event::InlineDirective(directive)) if depth == 1 => alt.push_str(&directive.raw),
            Some(Event::SoftBreak | Event::HardBreak) if depth == 1 => alt.push(' '),
            Some(_) => {}
            None => unreachable!("an image always ends"),
        }
    }
    (dest_url, alt)
}

/// A fence of `language` over `body` with `alt` as its attribute, lines
/// separated by `\n`. Its marker runs longer than any run of that character
/// in `body`, so no body line can close it.
fn fence(language: &str, alt: &str, body: &str) -> String {
    let info = info_string(language, alt);
    // A backtick fence's info string may not hold a backtick.
    let marker = if info.contains('`') { '~' } else { '`' };
    let longest = body.split(|c| c != marker).map(str::len).max().unwrap_or(0);
    let fence = marker.to_string().repeat((longest + 1).max(3));
    let mut out = format!("{fence}{info}\n");
    for line in body.lines() {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&fence);
    out
}

/// `language {alt="…"}`, written so that cmark's decoding of the info string
/// and then [`crate::fence::parse_fence_info`] give back `alt`, less what
/// cannot survive (see [`Rewrite::lone_image`]).
fn info_string(language: &str, alt: &str) -> String {
    let mut alt: String = alt
        .chars()
        .filter(|&c| c != '}')
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    if alt.is_empty() {
        return language.to_owned();
    }
    // A quoted value ends at the first of its quotes that whitespace follows.
    let closes_early = |alt: &str, quote: char| {
        alt.match_indices(quote)
            .any(|(i, _)| alt[i + 1..].starts_with(char::is_whitespace))
    };
    let quote = if !closes_early(&alt, '"') {
        '"'
    } else if !closes_early(&alt, '\'') {
        '\''
    } else {
        alt.retain(|c| c != '"');
        '"'
    };
    let mut info = format!("{language} {{alt={quote}");
    // cmark decodes entities and backslash escapes in an info string.
    for c in alt.chars() {
        match c {
            '&' => info.push_str("&amp;"),
            '\\' => info.push_str("\\\\"),
            c => info.push(c),
        }
    }
    info.push(quote);
    info.push('}');
    info
}

/// The source range a lone image's fence covers, and the text to put there.
///
/// Every line gets the prefix that continues the image's innermost blockquote
/// or list item, which starts at `container`. Only a line that opens that
/// container keeps what precedes the image, its markers. Any other line is
/// replaced whole: it may be a paragraph's continuation, lazy or indented
/// four columns or more, which a fence's line may not be. So is the prefix of
/// a `:::` line the image's cmark paragraph went on into.
fn splice_image(
    markdown: &str,
    container: Option<usize>,
    image: &Range<usize>,
    paragraph_end: usize,
    fence: &str,
) -> (Range<usize>, String) {
    let image_line = line_start(markdown, image.start);
    let (opening, opens_here) = container.map_or((String::new(), false), |at| {
        let opens_here = line_start(markdown, at) == image_line;
        (container_opening(markdown, at), opens_here)
    });
    let prefix = continuation(&opening);
    let mut text = if opens_here { opening } else { prefix.clone() };
    // Only whitespace follows a lone image on its line.
    let mut end = markdown[image.end..]
        .find(['\n', '\r'])
        .map_or(markdown.len(), |i| image.end + i);
    let line_break = LINE_BREAKS
        .into_iter()
        .find(|eol| markdown[end..].starts_with(eol));
    let eol = line_break
        .or_else(|| {
            LINE_BREAKS
                .into_iter()
                .find(|eol| markdown[..image_line].ends_with(eol))
        })
        // cmark does not end a fence's info string at a bare `\r`.
        .filter(|&eol| eol != "\r")
        .unwrap_or("\n");
    text.push_str(&fence.replace('\n', &format!("{eol}{prefix}")));
    let next_line = end + line_break.map_or(0, str::len);
    if next_line < paragraph_end {
        let rest = &markdown[next_line..];
        text.push_str(eol);
        text.push_str(&prefix);
        end = next_line + rest.len() - rest.trim_start_matches([' ', '\t', '>']).len();
    }
    (image_line..end, text)
}

/// The line holding the blockquote or list item at `at`, up to where the
/// container's content starts: what that line opens a fence with.
fn container_opening(markdown: &str, at: usize) -> String {
    let line = line_start(markdown, at);
    // cmark may start a container's range at its indentation.
    let at = markdown[at..]
        .find(|c| c != ' ' && c != '\t')
        .map_or(at, |i| at + i);
    let rest = &markdown[at..];
    if rest.starts_with('>') {
        return format!("{} ", &markdown[line..=at]);
    }
    let marker = at + rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(0) + 1;
    let gap = markdown[marker..].len() - markdown[marker..].trim_start_matches([' ', '\t']).len();
    let column = |to: usize| {
        markdown[line..to].chars().fold(0, |col, c| {
            if c == '\t' {
                col + 4 - col % 4
            } else {
                col + 1
            }
        })
    };
    let content_follows =
        !markdown[marker + gap..].starts_with(['\n', '\r']) && marker + gap < markdown.len();
    // Content more than four columns past the marker is indented code, and
    // an item that opens blank takes its content from the next line: either
    // way it sits one column past the marker.
    if gap > 0 && content_follows && column(marker + gap) - column(marker) <= 4 {
        markdown[line..marker + gap].to_owned()
    } else {
        format!("{} ", &markdown[line..marker])
    }
}

const LINE_BREAKS: [&str; 3] = ["\r\n", "\n", "\r"];

fn line_start(markdown: &str, at: usize) -> usize {
    let before = &markdown[..at];
    // cmark starts a CRLF blank line's text at its `\n`: `\r\n` is one break.
    let before = if markdown[at..].starts_with('\n') {
        before.strip_suffix('\r').unwrap_or(before)
    } else {
        before
    };
    before.rfind(['\n', '\r']).map_or(0, |i| i + 1)
}

/// The prefix that continues, on a later line, the containers `prefix` opens:
/// a list marker becomes spaces, a quote marker stays.
fn continuation(prefix: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + 1);
    let mut column = 0;
    let mut shifted = false;
    let mut chars = prefix.chars().peekable();
    while let Some(c) = chars.next() {
        let width = if c == '\t' { 4 - column % 4 } else { 1 };
        column += width;
        match c {
            '>' => {
                out.push(c);
                // A compact `>x` marker would swallow the space standing in
                // for `x`, or a replacement line's leading space.
                if !chars
                    .peek()
                    .is_some_and(|&next| next == ' ' || next == '\t')
                {
                    out.push(' ');
                    shifted = true;
                }
            }
            // A space added above would move this tab's stop.
            '\t' if !shifted => out.push(c),
            _ => out.push_str(&" ".repeat(width)),
        }
    }
    out
}

/// Copy `markdown` with each range replaced, in one forward pass: the ranges
/// index the original and are in order, so they stay valid however much the
/// replacements change the length.
fn apply(markdown: &str, replacements: &[(Range<usize>, String)]) -> String {
    let mut result = String::with_capacity(markdown.len());
    let mut pos = 0;
    for (range, replacement) in replacements {
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
    use crate::FenceAttrs;
    use pretty_assertions::assert_eq;

    fn rewrite_fences(markdown: &str, rewrite: impl FnMut(&str, &str) -> Option<String>) -> String {
        super::rewrite(markdown, true, true, &mut Fences(rewrite)).into_owned()
    }

    struct LoneImages<F>(F);

    impl<F: FnMut(&str, &str) -> Option<(String, String)>> Rewrite for LoneImages<F> {
        fn lone_image(&mut self, dest: &str, alt: &str) -> Option<(String, String)> {
            (self.0)(dest, alt)
        }
    }

    fn rewrite_lone_images(
        markdown: &str,
        wikilinks: bool,
        directives: bool,
        rewrite: impl FnMut(&str, &str) -> Option<(String, String)>,
    ) -> String {
        super::rewrite(markdown, wikilinks, directives, &mut LoneImages(rewrite)).into_owned()
    }

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
    fn a_fence_after_a_leading_bom_is_replaced() {
        let md = "\u{feff}```upper\nhello\n```\n";
        assert_eq!(rewrite_fences(md, upper), "\u{feff}```upper\nHELLO\n```\n");
    }

    #[test]
    fn a_fence_in_frontmatter_is_left_alone() {
        let md = "---\nnote: |\n  ```upper\n  keep\n  ```\n---\n```upper\nhi\n```\n";
        assert_eq!(
            rewrite_fences(md, upper),
            "---\nnote: |\n  ```upper\n  keep\n  ```\n---\n```upper\nHI\n```\n"
        );
    }

    /// GFM syntax decides where blocks end: an empty task item followed by a
    /// blank line ends its list, so the fence after it is not in the item.
    #[test]
    fn fences_are_the_ones_the_renderer_sees() {
        assert_eq!(
            rewrite_fences("- [ ]\n\n  ```upper\nhi\n", upper),
            "- [ ]\n\n  ```upper\nHI\n"
        );
    }

    #[test]
    fn only_a_fence_marker_or_an_image_opener_needs_a_parse() {
        for md in ["", "a ``code`` b ~~s~~ ! [x](y) ~", "`` ` ``"] {
            assert!(!has_marker(md), "{md:?}");
        }
        for md in [
            "```x\n```",
            "~~~x\n~~~",
            "![a](b)",
            "![[a]]",
            "x\n```",
            "x ! ~ ![a",
        ] {
            assert!(has_marker(md), "{md:?}");
        }
    }

    #[test]
    fn a_page_with_nothing_rewritten_comes_back_borrowed() {
        for md in [
            "a ``code`` b",
            "```rust\nfn main() {}\n```\n\n![a](x.png)\n",
        ] {
            let out = super::rewrite(md, true, true, &mut Fences(upper));
            assert!(matches!(out, Cow::Borrowed(_)), "{md:?}");
        }
    }

    /// Fences, then images, in document order, all spliced over the original.
    #[test]
    fn fences_and_lone_images_are_rewritten_in_one_pass() {
        struct Both;
        impl Rewrite for Both {
            fn fence(&mut self, language: &str, body: &str) -> Option<String> {
                upper(language, body)
            }
            fn lone_image(&mut self, dest: &str, _: &str) -> Option<(String, String)> {
                Some(diagram(dest))
            }
        }
        let md = "> ```upper\n> a\n> ```\n>\n> ![x](x.puml)\n\n```upper\nb\n```\n\n![y](y.puml)\n";
        assert_eq!(
            super::rewrite(md, true, true, &mut Both),
            "> ```upper\n> A\n> ```\n>\n> ```plantuml {alt=\"x\"}\n> A -> x.puml\n> ```\n\n\
             ```upper\nB\n```\n\n```plantuml {alt=\"y\"}\nA -> y.puml\n```\n"
        );
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

    /// A blank line has no container prefix to give the replacement, so in
    /// a list item the replacement would end the list.
    #[test]
    fn blank_fence_body_is_never_offered_to_the_closure() {
        for md in [
            "```upper\n\n```\n",
            "```upper\r\n\r\n\r\n```\r\n",
            "- a\n\n  ```upper\n\n\n  ```\n- b\n",
        ] {
            let mut seen = Vec::new();
            let result = rewrite_fences(md, record_all(&mut seen));
            assert!(seen.is_empty(), "{md:?}: closure was offered {seen:?}");
            assert_eq!(result, md);
        }
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

    #[test]
    fn crlf_body_keeps_its_line_endings() {
        let md = "```upper\r\nhello\r\nworld\r\n```\r\n";
        assert_eq!(
            rewrite_fences(md, upper),
            "```upper\r\nHELLO\r\nWORLD\r\n```\r\n"
        );
    }

    /// cmark starts a CRLF blank line's text at its `\n`, which must not
    /// read as the start of a line of its own.
    #[test]
    fn a_crlf_body_opening_blank_is_replaced_from_its_first_line() {
        for (md, expected) in [
            (
                "```upper\r\n\r\nbody\r\n```\r\n",
                "```upper\r\nX\r\nY\r\n```\r\n",
            ),
            (
                "> ```upper\r\n>\r\n> body\r\n> ```\r\n",
                "> ```upper\r\n> X\r\n> Y\r\n> ```\r\n",
            ),
            (
                "- a\r\n\r\n  ```upper\r\n\r\n  body\r\n  ```\r\n",
                "- a\r\n\r\n  ```upper\r\n  X\r\n  Y\r\n  ```\r\n",
            ),
        ] {
            let out = rewrite_fences(md, |_, _| Some("X\nY".to_owned()));
            assert_eq!(out, expected, "{md:?}");
        }
    }

    /// Appends a line to every body, so each test exercises a line the source
    /// never had.
    fn add_line(markdown: &str) -> String {
        rewrite_fences(markdown, |_, source| Some(format!("{source}\nadded")))
    }

    #[test]
    fn added_line_in_a_blockquote_keeps_the_quote_marker() {
        let md = "> ```x\n> a\n> ```\n";
        assert_eq!(add_line(md), "> ```x\n> a\n> added\n> ```\n");
    }

    #[test]
    fn added_line_in_a_list_item_keeps_the_item_indentation() {
        let md = "- item\n\n  ```x\n  a\n  ```\n\nafter\n";
        assert_eq!(
            add_line(md),
            "- item\n\n  ```x\n  a\n  added\n  ```\n\nafter\n"
        );
    }

    #[test]
    fn added_line_in_a_quote_inside_a_list_item_keeps_both_prefixes() {
        let md = "- > ```x\n  > a\n  > ```\n";
        assert_eq!(add_line(md), "- > ```x\n  > a\n  > added\n  > ```\n");
    }

    #[test]
    fn added_line_in_a_crlf_blockquote_keeps_marker_and_line_ending() {
        let md = "> ```x\r\n> a\r\n> ```\r\n";
        assert_eq!(add_line(md), "> ```x\r\n> a\r\n> added\r\n> ```\r\n");
    }

    #[test]
    fn indented_line_in_a_compact_blockquote_keeps_its_indentation() {
        let md = ">```x\n>a\n>  b\n>```\n";
        assert_eq!(
            rewrite_fences(md, |_, source| Some(source.to_owned())),
            ">```x\n> a\n>  b\n>```\n"
        );
    }

    /// A blank list-item line has no prefix, so the prefix must come from a
    /// line with content, and content written where the blank line was needs
    /// it too.
    #[test]
    fn content_replacing_a_blank_first_line_keeps_the_item_indentation() {
        let md = "- item\n\n  ```x\n\n  a\n  ```\n";
        let out = rewrite_fences(md, |_, source| Some(source.trim_start().to_owned()));
        assert_eq!(out, "- item\n\n  ```x\n  a\n  ```\n");
    }

    /// What the renderer tokenizes, code block bodies passed through
    /// `rewrite` the way [`rewrite_fences`] would.
    fn events_with(
        markdown: &str,
        mut rewrite: impl FnMut(&str, &str) -> Option<String>,
    ) -> Vec<String> {
        let mut parser = Parser::new(markdown, true, true);
        let mut events = Vec::new();
        while let Some(event) = parser.next() {
            let mut event = event.clone();
            if let crate::Event::CodeBlock(code) = &mut event
                && let Some(language) = &code.language
                && let Some(source) = code.source.strip_suffix('\n')
                && let Some(new) = rewrite(language, source)
            {
                code.source = format!("{new}\n");
            }
            events.push(format!("{event:?}"));
        }
        events
    }

    /// The rewritten document must tokenize as the original with only the
    /// fence bodies swapped: no container closed early, no fence swallowing
    /// what follows it.
    #[test]
    fn rewritten_document_keeps_its_block_structure() {
        let grow = |_: &str, source: &str| Some(source.replace('a', "a1\na2\n\na3"));
        for md in [
            "> ```x\n> a\n> b\n> ```\n\nafter\n",
            "- item\n\n  ```x\n  a\n\n  b\n  ```\n\nafter\n",
            "- ```x\n  b\n\n  a\n  ```\n- next\n",
            "- > ```x\n  > a\n  > b\n  > ```\n\nafter\n",
            "> ```x\r\n> a\r\n>\r\n> b\r\n> ```\r\n\r\nafter\r\n",
            "1. one\n   > - ```x\n   >   a\n   >   ```\n   > - two\n",
            "> ```x\n>\n> a\n> ```\n\nafter\n",
            "- item\n\n  ```x\n\n  a\n  ```\n\nafter\n",
        ] {
            let rewritten = rewrite_fences(md, grow);
            assert_eq!(
                events_with(&rewritten, |_, _| None),
                events_with(md, grow),
                "rewritten:\n{rewritten:?}"
            );
        }
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

    /// A `plantuml` fence over a body naming `dest`.
    fn diagram(dest: &str) -> (String, String) {
        ("plantuml".to_owned(), format!("A -> {dest}\n"))
    }

    /// Every lone image becomes a [`diagram`].
    fn rewrite(markdown: &str) -> String {
        rewrite_lone_images(markdown, true, true, |dest, _| Some(diagram(dest)))
    }

    /// Every `(dest_url, alt)` the closure is offered, declining them all.
    fn offers(markdown: &str, wikilinks: bool, directives: bool) -> Vec<(String, String)> {
        let mut seen = Vec::new();
        let out = rewrite_lone_images(markdown, wikilinks, directives, |dest, alt| {
            seen.push((dest.to_owned(), alt.to_owned()));
            None
        });
        assert_eq!(out, markdown, "a declined image changed");
        seen
    }

    fn offer(dest: &str, alt: &str) -> Vec<(String, String)> {
        vec![(dest.to_owned(), alt.to_owned())]
    }

    #[test]
    fn a_lone_image_becomes_a_fence() {
        assert_eq!(
            rewrite("Intro.\n\n![Payment flow](x.puml)\n\nAfter.\n"),
            "Intro.\n\n```plantuml {alt=\"Payment flow\"}\nA -> x.puml\n```\n\nAfter.\n"
        );
    }

    #[test]
    fn whitespace_around_the_image_goes_with_it() {
        assert_eq!(
            rewrite("   ![a](x.puml)  \n"),
            "```plantuml {alt=\"a\"}\nA -> x.puml\n```\n"
        );
        assert_eq!(
            rewrite("![a](x.puml)"),
            "```plantuml {alt=\"a\"}\nA -> x.puml\n```"
        );
    }

    #[test]
    fn a_title_or_reference_is_part_of_the_image() {
        let fence = "```plantuml {alt=\"a\"}\nA -> x.puml\n```";
        for (md, rest) in [
            ("![a](x.puml \"Title\")\n", "\n"),
            ("![a][r]\n\n[r]: x.puml\n", "\n\n[r]: x.puml\n"),
            ("![a][]\n\n[a]: x.puml\n", "\n\n[a]: x.puml\n"),
            ("![a]\n\n[a]: x.puml\n", "\n\n[a]: x.puml\n"),
        ] {
            assert_eq!(rewrite(md), format!("{fence}{rest}"), "{md:?}");
        }
        assert_eq!(
            rewrite("[a]: x.puml\n![a]\n"),
            format!("[a]: x.puml\n{fence}\n")
        );
    }

    #[test]
    fn a_wikilink_embed_is_an_image_only_with_wikilinks() {
        assert_eq!(
            offers("![[x.puml|Flow]]", true, true),
            offer("x.puml", "Flow")
        );
        assert_eq!(offers("![[x.puml]]", true, true), offer("x.puml", "x.puml"));
        assert_eq!(offers("![[x.puml]]", false, true), []);
    }

    #[test]
    fn an_empty_alt_writes_no_attribute() {
        assert_eq!(rewrite("![](x.puml)\n"), "```plantuml\nA -> x.puml\n```\n");
    }

    #[test]
    fn the_alt_text_is_what_the_renderer_shows() {
        for (md, alt) in [
            ("![a *b* `c` [d](e)](x.puml)", "a b c d"),
            ("![a\nb](x.puml)", "a b"),
            ("![a\\\nb](x.puml)", "a b"),
            ("![a &amp; b \\* c](x.puml)", "a & b * c"),
            ("![a <b>c</b>](x.puml)", "a c"),
            ("![a ![b](y.png) c](x.puml)", "a  c"),
            ("![:kbd[x]{k=v} a](x.puml)", ":kbd[x]{k=v} a"),
        ] {
            assert_eq!(offers(md, true, true), offer("x.puml", alt), "{md:?}");
        }
    }

    #[test]
    fn a_lone_image_in_a_container_keeps_its_prefix() {
        let body =
            |prefix: &str| format!("```plantuml {{alt=\"a\"}}\n{prefix}A -> x.puml\n{prefix}```");
        for (md, expected) in [
            ("> ![a](x.puml)\n", format!("> {}\n", body("> "))),
            (">![a](x.puml)\n", format!("> {}\n", body("> "))),
            (">\t![a](x.puml)\n", format!("> {}\n", body("> "))),
            (
                "-\t![a](x.puml)\n\n- b\n",
                format!("-\t{}\n\n- b\n", body(" \t")),
            ),
            (
                "-\n  ![a](x.puml)\n\n- b\n",
                format!("-\n  {}\n\n- b\n", body("  ")),
            ),
            (
                "> [!NOTE]\n> ![a](x.puml)\n",
                format!("> [!NOTE]\n> {}\n", body("> ")),
            ),
            (
                "- ![a](x.puml)\n\n- b\n",
                format!("- {}\n\n- b\n", body("  ")),
            ),
            (
                "10. a\n\n    ![a](x.puml)\n",
                format!("10. a\n\n    {}\n", body("    ")),
            ),
            (
                "1. > - ![a](x.puml)\n   >\n   > - b\n",
                format!("1. > - {}\n   >\n   > - b\n", body("   >   ")),
            ),
            ("> > ![a](x.puml)\n", format!("> > {}\n", body("> > "))),
        ] {
            assert_eq!(rewrite(md), expected, "{md:?}");
        }
    }

    #[test]
    fn only_the_image_line_of_a_split_paragraph_is_replaced() {
        let fence = |prefix: &str| {
            format!("{prefix}```plantuml {{alt=\"a\"}}\n{prefix}A -> x.puml\n{prefix}```")
        };
        for (md, expected) in [
            (
                ":::tab[macOS]\n![a](x.puml)\n:::\n",
                format!(":::tab[macOS]\n{}\n:::\n", fence("")),
            ),
            (
                ":::note\n![a](x.puml)\n:::",
                format!(":::note\n{}\n:::", fence("")),
            ),
            (
                "::leaf[x]\n![a](x.puml)\n",
                format!("::leaf[x]\n{}\n", fence("")),
            ),
            (
                "> :::note\n> ![a](x.puml)\n> :::\n",
                format!("> :::note\n{}\n> :::\n", fence("> ")),
            ),
            (
                "- :::note\n  ![a](x.puml)\n  :::\n\n- b\n",
                format!("- :::note\n{}\n  :::\n\n- b\n", fence("  ")),
            ),
            // A lazy line takes its container's prefix.
            (
                "> :::note\n![a](x.puml)\n",
                format!("> :::note\n{}\n", fence("> ")),
            ),
            (
                "> [!NOTE]\n![a](x.puml)\n",
                format!("> [!NOTE]\n{}\n", fence("> ")),
            ),
            // Indentation only a paragraph's continuation may have goes.
            (
                "[r]: x.puml\n\t![a][r]\n",
                format!("[r]: x.puml\n{}\n", fence("")),
            ),
            (
                ":::note\n    ![a](x.puml)\n    :::\n",
                format!(":::note\n{}\n:::\n", fence("")),
            ),
        ] {
            assert_eq!(rewrite(md), expected, "{md:?}");
        }
    }

    #[test]
    fn directives_off_leave_a_split_paragraph_whole() {
        assert_eq!(offers(":::tab[A]\n![a](x.puml)\n:::\n", true, false), []);
    }

    #[test]
    fn a_multiline_image_is_replaced_whole() {
        assert_eq!(
            rewrite("> ![a\n> b](x.puml)\n>\n> after\n"),
            "> ```plantuml {alt=\"a b\"}\n> A -> x.puml\n> ```\n>\n> after\n"
        );
    }

    #[test]
    fn crlf_line_endings_are_kept() {
        assert_eq!(
            rewrite("> ![a](x.puml)\r\n>\r\n> b\r\n"),
            "> ```plantuml {alt=\"a\"}\r\n> A -> x.puml\r\n> ```\r\n>\r\n> b\r\n"
        );
        assert_eq!(
            rewrite(":::note\r\n![a](x.puml)\r\n:::"),
            ":::note\r\n```plantuml {alt=\"a\"}\r\nA -> x.puml\r\n```\r\n:::"
        );
        // A body's own line endings give way to the document's.
        let out = rewrite_lone_images("a\r\n\r\n![a](x.puml)", true, true, |_, _| {
            Some(("plantuml".to_owned(), "1\r\n2\n".to_owned()))
        });
        assert_eq!(out, "a\r\n\r\n```plantuml {alt=\"a\"}\r\n1\r\n2\r\n```");
    }

    #[test]
    fn an_image_that_is_not_lone_is_never_offered() {
        for md in [
            "see ![a](x.puml)",
            "![a](x.puml) here",
            "![a](x.puml)\n![b](y.puml)",
            "[![a](x.puml)](https://example.com)",
            "- ![a](x.puml)",
            "# ![a](x.puml)",
            "| ![a](x.puml) |\n|---|",
            ":::note\n![a](x.puml)\n::leaf[x]\n:::",
        ] {
            assert_eq!(offers(md, true, true), [], "{md:?}");
        }
    }

    #[test]
    fn an_image_in_frontmatter_code_or_html_is_never_offered() {
        for md in [
            "---\ntitle: T\nimage: ![a](x.puml)\n---\n",
            "```md\n![a](x.puml)\n```\n",
            "    ![a](x.puml)\n",
            "<div>\n![a](x.puml)\n</div>\n",
        ] {
            assert_eq!(offers(md, true, true), [], "{md:?}");
        }
        assert_eq!(
            rewrite("---\nimage: ![a](x.puml)\n---\n\n![b](y.puml)\n"),
            "---\nimage: ![a](x.puml)\n---\n\n```plantuml {alt=\"b\"}\nA -> y.puml\n```\n"
        );
    }

    #[test]
    fn a_lone_image_after_a_bom_or_frontmatter_is_replaced_in_place() {
        let fence = "```plantuml {alt=\"b\"}\nA -> y.puml\n```\n";
        for head in [
            "\u{feff}",
            "---\nt: 1\n---\n",
            "\u{feff}---\nt: 1\n---\n\n",
            "---\r\nt: 1\r\n---\r\n",
        ] {
            assert_eq!(
                rewrite(&format!("{head}![b](y.puml)\n")),
                format!("{head}{fence}"),
                "{head:?}"
            );
        }
    }

    #[test]
    fn a_declined_image_is_left_as_it_was() {
        let md = "![a](x.png)\n\n![b](y.puml)\n";
        let out = rewrite_lone_images(md, true, true, |dest, _| {
            (dest != "x.png").then(|| diagram(dest))
        });
        assert_eq!(
            out,
            "![a](x.png)\n\n```plantuml {alt=\"b\"}\nA -> y.puml\n```\n"
        );
    }

    #[test]
    fn the_fence_outruns_the_body() {
        let with_body = |alt: &str, body: &str| {
            let md = format!("![{alt}](x.puml)\n");
            rewrite_lone_images(&md, true, true, |_, _| {
                Some(("plantuml".to_owned(), body.to_owned()))
            })
        };
        assert_eq!(
            with_body("a", "```\nx ````` y\n"),
            "``````plantuml {alt=\"a\"}\n```\nx ````` y\n``````\n"
        );
        assert_eq!(
            with_body("a\\`b", "~~~~\n"),
            "~~~~~plantuml {alt=\"a`b\"}\n~~~~\n~~~~~\n"
        );
        assert_eq!(with_body("a", ""), "```plantuml {alt=\"a\"}\n```\n");
    }

    /// The info string `fence` writes for `alt`, parsed the way the renderer
    /// reads it: cmark's decoding, then the fence grammar.
    fn alt_back(alt: &str) -> Option<String> {
        let md = fence("plantuml", alt, "x");
        let mut parser = Parser::new(&md, true, true);
        let Some(Event::CodeBlock(code)) = parser.next() else {
            panic!("{md:?} is no fence");
        };
        assert_eq!(code.language.as_deref(), Some("plantuml"), "{md:?}");
        assert_eq!(code.source, "x\n", "{md:?}");
        assert_eq!(code.attrs.id, None, "{md:?}");
        assert!(code.attrs.map.len() <= 1, "{md:?}");
        code.attrs
            .map
            .into_iter()
            .find(|(k, _)| k == "alt")
            .map(|(_, v)| v)
    }

    #[test]
    fn the_alt_attribute_parses_back_to_the_alt_text() {
        for alt in [
            "Payment flow",
            "Customer's order",
            r#"Say "hi" now"#,
            r#"a "b" c's 'd'"#,
            r#"ends with ""#,
            "ends with '",
            "\"",
            "'",
            "a & b &amp; c",
            r"back\slash \\ \* \",
            "`code` and ``more``",
            "{a=b #c .d",
            "  padded  ",
            "tab\there",
            "x\u{a0}' y",
            "日本語 🎉",
        ] {
            assert_eq!(alt_back(alt).as_deref(), Some(alt), "{alt:?}");
        }
    }

    #[test]
    fn what_an_info_string_cannot_hold_is_lost() {
        for (alt, back) in [
            ("a}b", "ab"),
            ("a\nb\r\nc", "a b  c"),
            (r#"Say "hi" and 'bye' now"#, "Say hi and 'bye' now"),
        ] {
            assert_eq!(alt_back(alt).as_deref(), Some(back), "{alt:?}");
        }
        assert_eq!(alt_back(""), None);
        assert_eq!(alt_back("}"), None);
    }

    /// The events the renderer sees for `markdown`, every lone image's
    /// paragraph standing in for the fence `rewrite` would write.
    fn events_as_fences(
        markdown: &str,
        mut rewrite: impl FnMut(&str, &str) -> Option<(String, String)>,
    ) -> Vec<String> {
        let mut parser = Parser::new(markdown, true, true);
        let mut events = Vec::new();
        loop {
            let lone = match parser.next() {
                None => break,
                Some(Event::Start(Tag::Paragraph { lone_image: true })) => true,
                Some(event) => {
                    events.push(format!("{event:?}"));
                    false
                }
            };
            if !lone {
                continue;
            }
            let (dest, alt) = take_image(&mut parser);
            let (language, body) = rewrite(&dest, &alt).expect("every image is rewritten");
            let mut attrs = FenceAttrs::default();
            if !alt.is_empty() {
                attrs.insert("alt".to_owned(), alt);
            }
            let fence = Event::CodeBlock(CodeBlockPayload {
                language: Some(language),
                attrs,
                source: body,
            });
            events.push(format!("{fence:?}"));
            assert_eq!(parser.next(), Some(Event::End(TagEnd::Paragraph)));
        }
        events
    }

    /// The rewritten document must tokenize as the original with each lone
    /// image's paragraph turned into its fence: no container closed early, no
    /// directive line or neighbouring block parsed differently.
    #[test]
    fn rewritten_document_parses_as_the_original_with_fences() {
        let body = |dest: &str, _: &str| {
            Some(("plantuml".to_owned(), format!("A -> {dest}\n\n  ```\nB\n")))
        };
        for md in [
            "# T\n\n![Payment flow](x.puml)\n\nafter\n",
            "![a](x.puml)\n# H\n",
            "![a](x.puml)\n| a |\n| - |\n| b |\n",
            "![a](x.puml)\n- item\n",
            "> ![a\n> b](x.puml)\n>\n> after\n",
            "> [!WARNING]\n> ![a &amp; \\* b](x.puml)\n",
            "- ![a](x.puml)\n\n- b\n",
            "1. a\n\n   ![b](x.puml)\n2. c\n",
            "- > ![a](x.puml)\n  >\n  > - ![b](y.puml)\n  >\n  >   c\n",
            "::::tabs\n:::tab[macOS]\n![a](mac.puml)\n:::\n:::tab[Linux]\n![b](linux.puml)\n:::\n::::\n",
            ":::note\n\n![a](x.puml)\n\n:::\n",
            ":::note\r\n![a](x.puml)\r\n:::\r\n",
            "> :::note\n> ![a](x.puml)\n> :::\n\nafter\n",
            "- :::note\n  ![a](x.puml)\n  :::\n\n- b\n",
            "> :::note\n![a](x.puml)\n",
            "::leaf[x]\n![a](x.puml)\n",
            "[r]: x.puml\n![a][r]\n",
            "![[x.puml|Alt]]\n\n![[y.puml]]\n",
            "---\ntitle: T\n---\n![a](x.puml)\n",
            "Привет 👋\n\n![діаграма 🎉](x.puml)\n\nпосле\n",
            "> [!NOTE]\n![a](x.puml)\n",
            "> > [!NOTE]\n >![a](x.puml)\n",
            "[r]: x.puml\n\t![a][r]\n",
            ":::note\n![a](x.puml)\n    :::\n",
            ":::note\n\t![a](x.puml)\n    :::\n",
            " >- >\t![a](x.puml)\n",
            ">> \t> ![a](x.puml)\n",
            "  -\t1)  a\n\n    ![a](x.puml)\n",
            "-     a\n\n  ![a](x.puml)\n",
            "-\n  ![a](x.puml)\n\n- b\n",
            "a\r\rb\r\r![a](x.puml)\r\rc\r",
            "- a\r\r  ![a](x.puml)\r",
            "> a\r>\r> ![a](x.puml)\r",
        ] {
            let rewritten = rewrite_lone_images(md, true, true, body);
            assert_ne!(rewritten, md, "nothing was rewritten");
            assert_eq!(
                events_as_fences(&rewritten, |_, _| None),
                events_as_fences(md, body),
                "rewritten:\n{rewritten:?}"
            );
        }
    }
}
