//! Confluence storage format renderer for pulldown-cmark events.
//!
//! This module converts `CommonMark` events to Confluence XHTML storage format,
//! which is the format used by Confluence's REST API for page content.
//!
//! # Features
//!
//! - Code blocks with language and line numbers via `<ac:structured-macro>`
//! - Blockquotes rendered as Confluence info panels
//! - Task lists with checkbox support
//! - Table of contents macro insertion
//! - Title extraction from first H1 with header level adjustment
//! - `PlantUML` diagram placeholder support
//!
//! # Example
//!
//! ```ignore
//! use pulldown_cmark::Parser;
//! use docstage_core::ConfluenceRenderer;
//!
//! let markdown = "# Title\n\nHello, **world**!";
//! let parser = Parser::new(markdown);
//! let renderer = ConfluenceRenderer::new().with_title_extraction();
//! let result = renderer.render(&mut parser.into_iter().peekable());
//! ```

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use std::fmt::Write;

use crate::util::heading_level_to_num;

/// Information about an extracted `PlantUML` diagram.
#[derive(Debug, Clone)]
pub struct DiagramInfo {
    /// Original source code from markdown (as-is, without include resolution)
    pub source: String,
    /// Zero-based index of this diagram
    pub index: usize,
}

/// Result of rendering markdown to Confluence format.
pub struct RenderResult {
    /// Rendered Confluence XHTML
    pub html: String,
    /// Title extracted from first H1 heading (if `extract_title` was enabled)
    pub title: Option<String>,
    /// `PlantUML` diagrams extracted from code blocks
    pub diagrams: Vec<DiagramInfo>,
}

/// Renders pulldown-cmark events to Confluence XHTML storage format.
#[allow(clippy::struct_excessive_bools)]
pub struct ConfluenceRenderer {
    output: String,
    /// Stack of nested list types (true = ordered, false = unordered)
    list_stack: Vec<bool>,
    /// Whether we're inside a code block
    in_code_block: bool,
    /// Language of current code block
    code_language: Option<String>,
    /// Whether we're inside a table header row
    in_table_head: bool,
    /// Whether to extract title from first H1 and level up headers
    extract_title: bool,
    /// Extracted title from first H1
    title: Option<String>,
    /// Whether we've seen the first H1
    seen_first_h1: bool,
    /// Whether we're currently inside the first H1 (to capture its text)
    in_first_h1: bool,
    /// Buffer for first H1 text
    h1_text: String,
    /// Extracted `PlantUML` diagrams
    diagrams: Vec<DiagramInfo>,
    /// Whether we're inside a plantuml code block
    in_plantuml_block: bool,
    /// Buffer for plantuml source
    plantuml_source: String,
}

impl ConfluenceRenderer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            list_stack: Vec::new(),
            in_code_block: false,
            code_language: None,
            in_table_head: false,
            extract_title: false,
            title: None,
            seen_first_h1: false,
            in_first_h1: false,
            h1_text: String::new(),
            diagrams: Vec::new(),
            in_plantuml_block: false,
            plantuml_source: String::new(),
        }
    }

    /// Enable title extraction from first H1 heading.
    /// When enabled, the first H1 is extracted as title and not rendered,
    /// and all other headers are leveled up (H2→H1, H3→H2, etc.)
    #[must_use]
    pub fn with_title_extraction(mut self) -> Self {
        self.extract_title = true;
        self
    }

    /// Compute adjusted heading level for Confluence output.
    ///
    /// When title extraction is enabled and we've seen the first H1,
    /// all subsequent headings are leveled up (H2→H1, H3→H2, etc.).
    fn adjusted_heading_level(&self, level_num: u8) -> u8 {
        if self.extract_title && self.seen_first_h1 && level_num > 1 {
            level_num - 1
        } else {
            level_num
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

    /// Render markdown events and return HTML, extracted title, and diagrams.
    pub fn render_with_title<'a, I>(mut self, events: I) -> RenderResult
    where
        I: Iterator<Item = Event<'a>>,
    {
        for event in events {
            self.process_event(event);
        }
        RenderResult {
            html: self.output,
            title: self.title,
            diagrams: self.diagrams,
        }
    }

    fn process_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.inline_code(&code),
            Event::Html(html) | Event::InlineHtml(html) => self.html(&html),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.horizontal_rule(),
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x]" } else { "[ ]" };
                write!(self.output, "{marker} ").unwrap();
            }
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                // Not supported in Confluence
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                if !self.in_code_block {
                    self.output.push_str("<p>");
                }
            }
            Tag::Heading { level, .. } => {
                if self.extract_title && level == HeadingLevel::H1 && !self.seen_first_h1 {
                    // First H1 - capture as title, don't render
                    self.in_first_h1 = true;
                    self.h1_text.clear();
                } else {
                    let level = self.adjusted_heading_level(heading_level_to_num(level));
                    write!(self.output, "<h{level}>").unwrap();
                }
            }
            Tag::BlockQuote(_) => {
                self.output.push_str(
                    r#"<ac:structured-macro ac:name="info" ac:schema-version="1"><ac:rich-text-body>"#,
                );
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                        Some(lang.split_whitespace().next().unwrap_or("").to_string())
                    }
                    _ => None,
                };

                // Check if this is a plantuml block
                if lang.as_deref() == Some("plantuml") {
                    self.in_plantuml_block = true;
                    self.plantuml_source.clear();
                } else {
                    self.in_code_block = true;
                    self.code_language.clone_from(&lang);

                    // Confluence code macro
                    self.output
                        .push_str(r#"<ac:structured-macro ac:name="code" ac:schema-version="1">"#);
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
                    self.output.push_str(r"<ac:plain-text-body><![CDATA[");
                }
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
            Tag::FootnoteDefinition(_) | Tag::HtmlBlock | Tag::MetadataBlock(_) => {}
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
                self.in_table_head = true;
                self.output.push_str("<tr>");
            }
            Tag::TableRow => {
                self.output.push_str("<tr>");
            }
            Tag::TableCell => {
                if self.in_table_head {
                    self.output.push_str("<th>");
                } else {
                    self.output.push_str("<td>");
                }
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
                if self.in_first_h1 {
                    // End of first H1 - save title, don't render
                    self.title = Some(self.h1_text.trim().to_string());
                    self.in_first_h1 = false;
                    self.seen_first_h1 = true;
                } else {
                    let level = self.adjusted_heading_level(heading_level_to_num(level));
                    write!(self.output, "</h{level}>").unwrap();
                }
            }
            TagEnd::BlockQuote(_) => {
                self.output
                    .push_str("</ac:rich-text-body></ac:structured-macro>");
            }
            TagEnd::CodeBlock => {
                if self.in_plantuml_block {
                    // End of plantuml block - save diagram and output placeholder
                    let index = self.diagrams.len();
                    self.diagrams.push(DiagramInfo {
                        source: std::mem::take(&mut self.plantuml_source),
                        index,
                    });
                    write!(self.output, "{{{{DIAGRAM_{index}}}}}").unwrap();
                    self.in_plantuml_block = false;
                } else {
                    self.output
                        .push_str("]]></ac:plain-text-body></ac:structured-macro>");
                    self.in_code_block = false;
                    self.code_language = None;
                }
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
            TagEnd::FootnoteDefinition
            | TagEnd::Image
            | TagEnd::HtmlBlock
            | TagEnd::MetadataBlock(_) => {}
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
            TagEnd::TableHead => {
                self.output.push_str("</tr>");
                self.in_table_head = false;
            }
            TagEnd::TableRow => {
                self.output.push_str("</tr>");
            }
            TagEnd::TableCell => {
                if self.in_table_head {
                    self.output.push_str("</th>");
                } else {
                    self.output.push_str("</td>");
                }
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
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_first_h1 {
            // Capture text for title
            self.h1_text.push_str(text);
        } else if self.in_plantuml_block {
            // Capture plantuml source
            self.plantuml_source.push_str(text);
        } else if self.in_code_block {
            // Don't escape text in code blocks (CDATA)
            self.output.push_str(text);
        } else {
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
    use pulldown_cmark::{Options, Parser};

    fn render(markdown: &str) -> String {
        let parser = Parser::new(markdown);
        ConfluenceRenderer::new().render(parser)
    }

    fn render_with_options(markdown: &str) -> String {
        let options =
            Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
        let parser = Parser::new_ext(markdown, options);
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

    #[test]
    fn test_title_extraction() {
        let markdown = "# My Title\n\nSome content\n\n## Section\n\n### Subsection";
        let parser = Parser::new(markdown);
        let result = ConfluenceRenderer::new()
            .with_title_extraction()
            .render_with_title(parser);

        assert_eq!(result.title, Some("My Title".to_string()));
        assert!(!result.html.contains("<h1>My Title</h1>"));
        assert!(result.html.contains("<h1>Section</h1>")); // H2 -> H1
        assert!(result.html.contains("<h2>Subsection</h2>")); // H3 -> H2
    }

    #[test]
    fn test_no_title_extraction() {
        let markdown = "# My Title\n\n## Section";
        let parser = Parser::new(markdown);
        let result = ConfluenceRenderer::new().render_with_title(parser);

        assert_eq!(result.title, None);
        assert!(result.html.contains("<h1>My Title</h1>"));
        assert!(result.html.contains("<h2>Section</h2>"));
    }

    #[test]
    fn test_unordered_list() {
        let result = render("- Item 1\n- Item 2\n- Item 3");
        assert!(result.contains("<ul>"));
        assert!(result.contains("<li>"));
        assert!(result.contains("Item 1"));
        assert!(result.contains("</li>"));
        assert!(result.contains("</ul>"));
    }

    #[test]
    fn test_ordered_list() {
        let result = render("1. First\n2. Second\n3. Third");
        assert!(result.contains("<ol>"));
        assert!(result.contains("<li>"));
        assert!(result.contains("First"));
        assert!(result.contains("</ol>"));
    }

    #[test]
    fn test_nested_list() {
        let result = render("- Outer\n  - Inner");
        assert!(result.contains("<ul>"));
        assert!(result.contains("Outer"));
        assert!(result.contains("Inner"));
    }

    #[test]
    fn test_table() {
        let result = render_with_options("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(result.contains("<table>"));
        assert!(result.contains("<tbody>"));
        assert!(result.contains("<tr>"));
        assert!(result.contains("<th>"));
        assert!(result.contains("<td>"));
        assert!(result.contains("</table>"));
    }

    #[test]
    fn test_emphasis() {
        let result = render("*italic* and **bold**");
        assert!(result.contains("<em>italic</em>"));
        assert!(result.contains("<strong>bold</strong>"));
    }

    #[test]
    fn test_strikethrough() {
        let result = render_with_options("~~deleted~~");
        assert!(result.contains("<s>deleted</s>"));
    }

    #[test]
    fn test_link() {
        let result = render("[text](https://example.com)");
        assert!(result.contains(r#"<a href="https://example.com">text</a>"#));
    }

    #[test]
    fn test_link_with_special_chars() {
        let result = render(r"[test](https://example.com?a=1&b=2)");
        assert!(result.contains(r#"href="https://example.com?a=1&amp;b=2""#));
    }

    #[test]
    fn test_external_image() {
        let result = render("![alt](https://example.com/image.png)");
        assert!(result.contains(r"<ac:image>"));
        assert!(result.contains(r#"ri:url ri:value="https://example.com/image.png""#));
    }

    #[test]
    fn test_local_image() {
        let result = render("![alt](./images/diagram.png)");
        assert!(result.contains(r"<ac:image>"));
        assert!(result.contains(r#"ri:attachment ri:filename="diagram.png""#));
    }

    #[test]
    fn test_inline_code() {
        let result = render("Use `code` here");
        assert!(result.contains("<code>code</code>"));
    }

    #[test]
    fn test_inline_code_escaping() {
        let result = render("Use `<script>` tag");
        assert!(result.contains("<code>&lt;script&gt;</code>"));
    }

    #[test]
    fn test_code_block_without_language() {
        let result = render("```\nplain code\n```");
        assert!(result.contains(r#"ac:name="code""#));
        assert!(!result.contains(r#"ac:name="language""#));
        assert!(result.contains("plain code"));
    }

    #[test]
    fn test_horizontal_rule() {
        let result = render("Above\n\n---\n\nBelow");
        assert!(result.contains("<hr />"));
    }

    #[test]
    fn test_hard_break() {
        let result = render("Line one  \nLine two");
        assert!(result.contains("<br />"));
    }

    #[test]
    fn test_xml_escaping() {
        // Use backticks to prevent markdown parsing HTML-like content
        let result = render("Use & \"quotes\" and 'apostrophes'");
        assert!(result.contains("&amp;"));
        assert!(result.contains("&quot;quotes&quot;"));
        assert!(result.contains("&#39;apostrophes&#39;"));
    }

    #[test]
    fn test_escape_xml_function() {
        assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
        assert_eq!(escape_xml("a & b"), "a &amp; b");
        assert_eq!(escape_xml("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(escape_xml("it's"), "it&#39;s");
    }

    #[test]
    fn test_plantuml_diagram() {
        let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```";
        let parser = Parser::new(markdown);
        let result = ConfluenceRenderer::new().render_with_title(parser);

        assert_eq!(result.diagrams.len(), 1);
        assert_eq!(result.diagrams[0].index, 0);
        assert!(result.diagrams[0].source.contains("@startuml"));
        assert!(result.html.contains("{{DIAGRAM_0}}"));
    }

    #[test]
    fn test_multiple_plantuml_diagrams() {
        let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```\n\nText\n\n```plantuml\n@startuml\nC -> D\n@enduml\n```";
        let parser = Parser::new(markdown);
        let result = ConfluenceRenderer::new().render_with_title(parser);

        assert_eq!(result.diagrams.len(), 2);
        assert_eq!(result.diagrams[0].index, 0);
        assert_eq!(result.diagrams[1].index, 1);
        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert!(result.html.contains("{{DIAGRAM_1}}"));
    }

    #[test]
    fn test_task_list() {
        let result = render_with_options("- [ ] Unchecked\n- [x] Checked");
        assert!(result.contains("[ ] Unchecked"));
        assert!(result.contains("[x] Checked"));
    }

    #[test]
    fn test_raw_html_passthrough() {
        let result = render("<div class=\"custom\">Content</div>");
        assert!(result.contains("<div class=\"custom\">Content</div>"));
    }

    #[test]
    fn test_all_heading_levels() {
        let result = render("## H2\n\n### H3\n\n#### H4\n\n##### H5\n\n###### H6");
        assert!(result.contains("<h2>H2</h2>"));
        assert!(result.contains("<h3>H3</h3>"));
        assert!(result.contains("<h4>H4</h4>"));
        assert!(result.contains("<h5>H5</h5>"));
        assert!(result.contains("<h6>H6</h6>"));
    }

    #[test]
    fn test_default_renderer() {
        let renderer = ConfluenceRenderer::default();
        let parser = Parser::new("Hello");
        let result = renderer.render(parser);
        assert_eq!(result, "<p>Hello</p>");
    }

    #[test]
    fn test_heading_level_adjustment_all_levels() {
        let markdown = "# Title\n\n## H2\n\n### H3\n\n#### H4\n\n##### H5\n\n###### H6";
        let parser = Parser::new(markdown);
        let result = ConfluenceRenderer::new()
            .with_title_extraction()
            .render_with_title(parser);

        assert_eq!(result.title, Some("Title".to_string()));
        // H2-H6 should be adjusted to H1-H5
        assert!(result.html.contains("<h1>H2</h1>"));
        assert!(result.html.contains("<h2>H3</h2>"));
        assert!(result.html.contains("<h3>H4</h3>"));
        assert!(result.html.contains("<h4>H5</h4>"));
        assert!(result.html.contains("<h5>H6</h5>"));
    }

    #[test]
    fn test_code_block_with_language_extra_info() {
        // Some markdown has extra info after language like ```python {.class}
        let result = render("```python extra\ncode\n```");
        assert!(result.contains(r#"ac:name="language">python"#));
    }
}
