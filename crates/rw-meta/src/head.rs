use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use rw_parser::is_comment_only_html;

pub(crate) struct Head {
    pub frontmatter: Option<String>,
    pub title: Option<String>,
}

impl Head {
    pub(crate) fn parse(markdown: &str) -> Self {
        let opts = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
        let parser = Parser::new_ext(markdown, opts);

        let mut frontmatter: Option<String> = None;
        let mut title: Option<String> = None;
        let mut in_metadata = false;
        let mut in_h1 = false;
        let mut title_buf = String::new();
        let mut leading_html = String::new();

        for event in parser {
            match event {
                Event::Start(Tag::MetadataBlock(_)) => {
                    in_metadata = true;
                }
                Event::Text(ref text) if in_metadata => {
                    frontmatter = Some(text.to_string());
                }
                Event::End(TagEnd::MetadataBlock(_)) => {
                    in_metadata = false;
                }
                Event::Start(Tag::Heading {
                    level: HeadingLevel::H1,
                    ..
                }) => {
                    in_h1 = true;
                }
                Event::Text(ref text) | Event::Code(ref text) if in_h1 => {
                    title_buf.push_str(text);
                }
                Event::End(TagEnd::Heading(HeadingLevel::H1)) => {
                    title = Some(title_buf);
                    break;
                }
                Event::Html(ref html) => leading_html.push_str(html),
                Event::End(TagEnd::HtmlBlock) if !is_comment_only_html(&leading_html) => break,
                // Only an H1 that opens the page, below nothing but comments, is
                // its title; `rw-renderer`'s title extraction follows the same rule.
                Event::Start(
                    Tag::Paragraph
                    | Tag::Heading { .. }
                    | Tag::BlockQuote(_)
                    | Tag::List(_)
                    | Tag::CodeBlock(_)
                    | Tag::Table(_),
                )
                | Event::Rule
                    if !in_metadata =>
                {
                    break;
                }
                _ => {}
            }
        }

        Self { frontmatter, title }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frontmatter_no_h1() {
        let head = Head::parse("Some plain text without headings.");
        assert!(head.frontmatter.is_none());
        assert!(head.title.is_none());
    }

    #[test]
    fn simple_h1() {
        let head = Head::parse("# Hello World\n\nSome content.");
        assert!(head.frontmatter.is_none());
        assert_eq!(head.title.as_deref(), Some("Hello World"));
    }

    #[test]
    fn h1_with_bold() {
        let head = Head::parse("# Hello **world**\n");
        assert_eq!(head.title.as_deref(), Some("Hello world"));
    }

    #[test]
    fn h1_with_code_span() {
        let head = Head::parse("# The `main` function\n");
        assert_eq!(head.title.as_deref(), Some("The main function"));
    }

    #[test]
    fn h1_with_italic_and_bold() {
        let head = Head::parse("# *italic* and **bold**\n");
        assert_eq!(head.title.as_deref(), Some("italic and bold"));
    }

    #[test]
    fn h1_with_link() {
        let head = Head::parse("# See [docs](url)\n");
        assert_eq!(head.title.as_deref(), Some("See docs"));
    }

    #[test]
    fn code_block_comment_not_h1() {
        let md = "```\n# comment\n```\n";
        let head = Head::parse(md);
        assert!(head.title.is_none());
    }

    #[test]
    fn no_h1_only_h2() {
        let head = Head::parse("## Second level\n\nSome content.");
        assert!(head.frontmatter.is_none());
        assert!(head.title.is_none());
    }

    #[test]
    fn frontmatter_only() {
        let md = "---\ntitle: My Page\n---\n\nSome content.";
        let head = Head::parse(md);
        assert!(head.frontmatter.is_some());
        assert!(head.frontmatter.unwrap().contains("title: My Page"));
        assert!(head.title.is_none());
    }

    #[test]
    fn frontmatter_and_h1() {
        let md = "---\ntitle: My Page\n---\n\n# Heading One\n\nContent.";
        let head = Head::parse(md);
        assert!(head.frontmatter.is_some());
        assert!(head.frontmatter.unwrap().contains("title: My Page"));
        assert_eq!(head.title.as_deref(), Some("Heading One"));
    }

    #[test]
    fn invalid_frontmatter_treated_as_thematic_break() {
        // Empty frontmatter (---\n---) is treated as thematic breaks, not metadata
        let md = "---\n---\n";
        let head = Head::parse(md);
        assert!(head.frontmatter.is_none());
        assert!(head.title.is_none());
    }

    #[test]
    fn frontmatter_with_dots_closing() {
        let md = "---\ntitle: Dots\n...\n\n# Heading\n";
        let head = Head::parse(md);
        assert!(head.frontmatter.is_some());
        assert!(head.frontmatter.unwrap().contains("title: Dots"));
        assert_eq!(head.title.as_deref(), Some("Heading"));
    }

    #[test]
    fn h1_after_paragraph_not_extracted() {
        let md = "Some introductory paragraph.\n\n# Late Heading\n";
        let head = Head::parse(md);
        assert!(head.title.is_none());
    }

    #[test]
    fn h1_after_other_heading_or_rule_not_extracted() {
        for md in ["## Before\n\n# Late\n", "***\n\n# Late\n"] {
            assert!(Head::parse(md).title.is_none(), "{md:?}");
        }
    }

    #[test]
    fn h1_below_only_comments_extracted() {
        for md in [
            "<!-- x -->\n# T\n",
            "<!-- a -->\n\n<!-- b -->\n# T\n",
            "<!--\nline one\nline two\n-->\n# T\n",
            "---\ntitle: Meta\n---\n<!-- x -->\n# T\n",
        ] {
            assert_eq!(Head::parse(md).title.as_deref(), Some("T"), "{md:?}");
        }
    }

    #[test]
    fn h1_below_other_html_not_extracted() {
        assert!(Head::parse("<!-- x --><div></div>\n# T\n").title.is_none());
    }

    #[test]
    fn only_takes_first_h1() {
        let md = "# First\n\n# Second\n";
        let head = Head::parse(md);
        assert_eq!(head.title.as_deref(), Some("First"));
    }
}
