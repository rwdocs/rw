use crate::event::{Event, LinkKind, Tag, TagEnd};
use crate::parser::Parser;

/// What opens a page: its frontmatter and its title.
///
/// ```
/// use rw_parser::PageHead;
///
/// let head = PageHead::from("---\nkind: guide\n---\n<!-- x -->\n# Done :status[OK]\n");
/// assert_eq!(head.frontmatter.as_deref(), Some("kind: guide\n"));
/// assert_eq!(head.title.as_deref(), Some("Done OK"));
/// assert_eq!(PageHead::from("Intro\n\n# Late\n").title, None);
/// ```
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PageHead {
    /// See [`Parser::frontmatter`].
    pub frontmatter: Option<String>,
    /// The plain text of the H1 that opens the page (see
    /// [`Tag::Heading`]), or `None` when there is none or it shows no text.
    ///
    /// The text is what the rendered heading shows, as far as syntax can
    /// tell: a directive's label (all `:status` shows, though an unknown name
    /// renders literally), a `[[target]]`'s target (what an unresolved one
    /// shows), and no image's alt text.
    pub title: Option<String>,
}

impl From<&str> for PageHead {
    fn from(markdown: &str) -> Self {
        let mut parser = Parser::new(markdown, true, true);
        let title = title(&mut parser);
        Self {
            frontmatter: parser.frontmatter().map(str::to_owned),
            title,
        }
    }
}

fn title(parser: &mut Parser<'_>) -> Option<String> {
    loop {
        match parser.next()? {
            Event::Start(Tag::Heading {
                opens_page: true, ..
            }) => break,
            // An indented HTML block's indent arrives as `Text`.
            Event::RawHtml(_) | Event::Text(_) => {}
            _ => return None,
        }
    }
    let mut title = String::new();
    let mut image_depth = 0;
    loop {
        match parser.next()? {
            Event::Start(Tag::Image { .. }) => image_depth += 1,
            Event::End(TagEnd::Image) => image_depth -= 1,
            _ if image_depth > 0 => {}
            Event::Text(text) | Event::Code(text) => title.push_str(&text),
            Event::InlineDirective(directive) => title.push_str(directive.args.content().trim()),
            Event::Start(Tag::Link {
                kind: LinkKind::Wiki { has_pothole: false },
                dest_url,
            }) => title.push_str(&dest_url),
            Event::End(TagEnd::Heading) => break,
            _ => {}
        }
    }
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title_of(markdown: &str) -> Option<String> {
        PageHead::from(markdown).title
    }

    #[test]
    fn inline_formatting_is_dropped() {
        for (md, expected) in [
            ("# Hello World\n\nSome content.", "Hello World"),
            ("# Hello **world**\n", "Hello world"),
            ("# The `main` function\n", "The main function"),
            ("# *italic* and **bold**\n", "italic and bold"),
            ("# See [docs](url)\n", "See docs"),
        ] {
            assert_eq!(title_of(md).as_deref(), Some(expected), "{md:?}");
        }
    }

    #[test]
    fn directives_wikilinks_and_images_show_what_the_heading_shows() {
        for (md, expected) in [
            ("# Done :status[OK]{color=green}\n", Some("Done OK")),
            ("# See [[domain:x::y]]\n", Some("See domain:x::y")),
            ("# See [[domain:x::y|the y]]\n", Some("See the y")),
            ("# Title ![badge](b.png)\n", Some("Title")),
            ("# ![![a](a.png) b](b.png) T\n", Some("T")),
            ("# ![logo](logo.png)\n", None),
            ("#\n", None),
        ] {
            assert_eq!(title_of(md).as_deref(), expected, "{md:?}");
        }
    }

    #[test]
    fn only_an_h1_that_opens_the_page_is_the_title() {
        for md in [
            "Some plain text without headings.",
            "## Second level\n\nSome content.",
            "```\n# comment\n```\n",
            "---\n---\n",
            "Some introductory paragraph.\n\n# Late Heading\n",
            "## Before\n\n# Late\n",
            "***\n\n# Late\n",
            "> # Quoted\n",
            "<!-- x --><div></div>\n# T\n",
            "<!-- x -->\n---\ntitle: Meta\n---\n# T\n",
        ] {
            assert_eq!(title_of(md), None, "{md:?}");
        }
    }

    #[test]
    fn frontmatter_and_comments_above_the_title_do_not_count() {
        for md in [
            "---\ntitle: My Page\n---\n\n# T\n\nContent.",
            "---\ntitle: Dots\n...\n\n# T\n",
            "<!-- x -->\n# T\n",
            "   <!-- x -->\n# T\n",
            "<!-- a -->\n\n<!-- b -->\n# T\n",
            "<!--\nline one\nline two\n-->\n# T\n",
            "---\ntitle: Meta\n---\n<!-- x -->\n# T\n",
            "# T\n\n# Second\n",
            "\u{feff}# T\n",
            "\u{feff}---\ntitle: Meta\n---\n# T\n",
        ] {
            assert_eq!(title_of(md).as_deref(), Some("T"), "{md:?}");
        }
    }

    #[test]
    fn frontmatter_is_a_metadata_block_that_opens_the_source() {
        for (md, expected) in [
            (
                "---\ntitle: My Page\n---\n\nSome content.",
                Some("title: My Page\n"),
            ),
            (
                "---\ntitle: Dots\n...\n\n# Heading\n",
                Some("title: Dots\n"),
            ),
            ("\u{feff}---\ntitle: T\n---\n", Some("title: T\n")),
            (
                "---\r\ntitle: T\r\nkind: guide\r\n---\r\n",
                Some("title: T\r\nkind: guide\r\n"),
            ),
            (
                "---\ntitle: A\n---\n<!-- x -->\n---\ntitle: B\n---\n",
                Some("title: A\n"),
            ),
            ("<!-- x -->\n---\ntitle: T\n---\n", None),
            ("\n---\ntitle: T\n---\n", None),
            ("# Heading One\n\nContent.", None),
            ("---\n---\n", None),
            ("<div></div>\n\n---\ntitle: T\n---\n", None),
            ("Intro\n\n---\ntitle: T\n---\n", None),
        ] {
            assert_eq!(
                PageHead::from(md).frontmatter.as_deref(),
                expected,
                "{md:?}"
            );
        }
    }
}
