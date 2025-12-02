//! Confluence storage format renderer for pulldown-cmark events.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use std::fmt::Write;

/// Renders pulldown-cmark events to Confluence XHTML storage format.
pub struct ConfluenceRenderer {
    output: String,
    /// Stack of nested list types (true = ordered, false = unordered)
    list_stack: Vec<bool>,
    /// Current heading level for TOC extraction
    in_heading: Option<HeadingLevel>,
    /// Buffer for heading text
    heading_text: String,
    /// Whether we're inside a code block
    in_code_block: bool,
    /// Language of current code block
    code_language: Option<String>,
}

impl ConfluenceRenderer {
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            list_stack: Vec::new(),
            in_heading: None,
            heading_text: String::new(),
            in_code_block: false,
            code_language: None,
        }
    }

    /// Render markdown events to Confluence storage format.
    pub fn render<'a, I>(mut self, events: I) -> String
    where
        I: Iterator<Item = Event<'a>>,
    {
        for event in events {
            self.process_event(event);
        }
        self.output
    }

    fn process_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.inline_code(&code),
            Event::Html(html) => self.html(&html),
            Event::InlineHtml(html) => self.html(&html),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.horizontal_rule(),
            Event::TaskListMarker(checked) => {
                if checked {
                    self.output.push_str("[x] ");
                } else {
                    self.output.push_str("[ ] ");
                }
            }
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                // Not supported in Confluence
            }
        }
    }

    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                if !self.in_code_block {
                    self.output.push_str("<p>");
                }
            }
            Tag::Heading { level, .. } => {
                self.in_heading = Some(level);
                self.heading_text.clear();
                let level_num = heading_level_to_num(level);
                write!(self.output, "<h{}>", level_num).unwrap();
            }
            Tag::BlockQuote(_) => {
                self.output.push_str(
                    r#"<ac:structured-macro ac:name="info" ac:schema-version="1"><ac:rich-text-body>"#,
                );
            }
            Tag::CodeBlock(kind) => {
                self.in_code_block = true;
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                        Some(lang.split_whitespace().next().unwrap_or("").to_string())
                    }
                    _ => None,
                };
                self.code_language = lang.clone();

                // Confluence code macro
                self.output.push_str(
                    r#"<ac:structured-macro ac:name="code" ac:schema-version="1">"#,
                );
                if let Some(ref lang) = lang {
                    write!(
                        self.output,
                        r#"<ac:parameter ac:name="language">{}</ac:parameter>"#,
                        escape_xml(lang)
                    )
                    .unwrap();
                }
                self.output
                    .push_str(r#"<ac:parameter ac:name="linenumbers">true</ac:parameter>"#);
                self.output
                    .push_str(r#"<ac:plain-text-body><![CDATA["#);
            }
            Tag::List(start) => {
                let ordered = start.is_some();
                self.list_stack.push(ordered);
                if ordered {
                    self.output.push_str("<ol>");
                } else {
                    self.output.push_str("<ul>");
                }
            }
            Tag::Item => {
                self.output.push_str("<li>");
            }
            Tag::FootnoteDefinition(_) => {}
            Tag::DefinitionList => {
                self.output.push_str("<dl>");
            }
            Tag::DefinitionListTitle => {
                self.output.push_str("<dt>");
            }
            Tag::DefinitionListDefinition => {
                self.output.push_str("<dd>");
            }
            Tag::Table(_alignments) => {
                self.output.push_str("<table><tbody>");
            }
            Tag::TableHead => {
                self.output.push_str("<tr>");
            }
            Tag::TableRow => {
                self.output.push_str("<tr>");
            }
            Tag::TableCell => {
                self.output.push_str("<td>");
            }
            Tag::Emphasis => {
                self.output.push_str("<em>");
            }
            Tag::Strong => {
                self.output.push_str("<strong>");
            }
            Tag::Strikethrough => {
                self.output.push_str("<s>");
            }
            Tag::Link { dest_url, .. } => {
                write!(self.output, r#"<a href="{}">"#, escape_xml(&dest_url)).unwrap();
            }
            Tag::Image { dest_url, .. } => {
                // Confluence image macro for attachments or external URLs
                if dest_url.starts_with("http://") || dest_url.starts_with("https://") {
                    write!(
                        self.output,
                        r#"<ac:image><ri:url ri:value="{}" /></ac:image>"#,
                        escape_xml(&dest_url)
                    )
                    .unwrap();
                } else {
                    // Local file - assume it will be uploaded as attachment
                    let filename = dest_url.rsplit('/').next().unwrap_or(&dest_url);
                    write!(
                        self.output,
                        r#"<ac:image><ri:attachment ri:filename="{}" /></ac:image>"#,
                        escape_xml(filename)
                    )
                    .unwrap();
                }
            }
            Tag::HtmlBlock | Tag::MetadataBlock(_) => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if !self.in_code_block {
                    self.output.push_str("</p>");
                }
            }
            TagEnd::Heading(level) => {
                self.in_heading = None;
                let level_num = heading_level_to_num(level);
                write!(self.output, "</h{}>", level_num).unwrap();
            }
            TagEnd::BlockQuote(_) => {
                self.output.push_str("</ac:rich-text-body></ac:structured-macro>");
            }
            TagEnd::CodeBlock => {
                self.output.push_str("]]></ac:plain-text-body></ac:structured-macro>");
                self.in_code_block = false;
                self.code_language = None;
            }
            TagEnd::List(ordered) => {
                self.list_stack.pop();
                if ordered {
                    self.output.push_str("</ol>");
                } else {
                    self.output.push_str("</ul>");
                }
            }
            TagEnd::Item => {
                self.output.push_str("</li>");
            }
            TagEnd::FootnoteDefinition => {}
            TagEnd::DefinitionList => {
                self.output.push_str("</dl>");
            }
            TagEnd::DefinitionListTitle => {
                self.output.push_str("</dt>");
            }
            TagEnd::DefinitionListDefinition => {
                self.output.push_str("</dd>");
            }
            TagEnd::Table => {
                self.output.push_str("</tbody></table>");
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                self.output.push_str("</tr>");
            }
            TagEnd::TableCell => {
                self.output.push_str("</td>");
            }
            TagEnd::Emphasis => {
                self.output.push_str("</em>");
            }
            TagEnd::Strong => {
                self.output.push_str("</strong>");
            }
            TagEnd::Strikethrough => {
                self.output.push_str("</s>");
            }
            TagEnd::Link => {
                self.output.push_str("</a>");
            }
            TagEnd::Image => {
                // Image is self-closing in start_tag
            }
            TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_code_block {
            // Don't escape text in code blocks (CDATA)
            self.output.push_str(text);
        } else {
            if self.in_heading.is_some() {
                self.heading_text.push_str(text);
            }
            self.output.push_str(&escape_xml(text));
        }
    }

    fn inline_code(&mut self, code: &str) {
        write!(self.output, "<code>{}</code>", escape_xml(code)).unwrap();
    }

    fn html(&mut self, html: &str) {
        // Pass through HTML as-is
        self.output.push_str(html);
    }

    fn soft_break(&mut self) {
        self.output.push('\n');
    }

    fn hard_break(&mut self) {
        self.output.push_str("<br />");
    }

    fn horizontal_rule(&mut self) {
        self.output.push_str("<hr />");
    }
}

impl Default for ConfluenceRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn heading_level_to_num(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn escape_xml(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Parser;

    fn render(markdown: &str) -> String {
        let parser = Parser::new(markdown);
        ConfluenceRenderer::new().render(parser)
    }

    #[test]
    fn test_basic_paragraph() {
        let result = render("Hello, world!");
        assert_eq!(result, "<p>Hello, world!</p>");
    }

    #[test]
    fn test_heading() {
        let result = render("# Title");
        assert_eq!(result, "<h1>Title</h1>");
    }

    #[test]
    fn test_code_block() {
        let result = render("```python\nprint('hello')\n```");
        assert!(result.contains(r#"ac:name="code""#));
        assert!(result.contains(r#"ac:name="language">python"#));
        assert!(result.contains("print('hello')"));
    }

    #[test]
    fn test_blockquote() {
        let result = render("> Note");
        assert!(result.contains(r#"ac:name="info""#));
    }
}
