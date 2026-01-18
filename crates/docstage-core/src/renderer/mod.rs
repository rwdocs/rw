//! Unified markdown renderer with pluggable backends.
//!
//! This module provides a generic [`MarkdownRenderer`] that can produce either
//! HTML or Confluence XHTML output using the [`RenderBackend`] trait.
//!
//! # Architecture
//!
//! The renderer uses a trait-based abstraction to handle format-specific differences:
//! - [`HtmlBackend`]: Produces semantic HTML5 with relative link resolution
//! - [`ConfluenceBackend`]: Produces Confluence XHTML storage format
//!
//! Shared functionality (tables, lists, inline formatting) is handled by the
//! generic renderer, while format-specific elements (code blocks, blockquotes,
//! images) are delegated to the backend.
//!
//! # Example
//!
//! ```ignore
//! use pulldown_cmark::Parser;
//! use docstage_core::renderer::{MarkdownRenderer, HtmlBackend};
//!
//! let markdown = "# Hello\n\n**Bold** text";
//! let parser = Parser::new(markdown);
//! let result = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_title_extraction()
//!     .render(parser);
//! ```

mod backend;
mod confluence;
mod html;
mod state;

use std::fmt::Write;
use std::marker::PhantomData;

use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};

pub use backend::RenderBackend;
pub use confluence::ConfluenceBackend;
pub use html::HtmlBackend;
pub use state::{TocEntry, escape_html, slugify};

use state::{CodeBlockState, HeadingState, ImageState, TableState};

use crate::util::heading_level_to_num;

/// Result of rendering markdown.
#[derive(Clone, Debug)]
pub struct RenderResult {
    /// Rendered HTML/XHTML content.
    pub html: String,
    /// Title extracted from first H1 heading (if `extract_title` was enabled).
    pub title: Option<String>,
    /// Table of contents entries.
    pub toc: Vec<TocEntry>,
}

/// Generic markdown renderer with pluggable backend.
///
/// Uses the [`RenderBackend`] trait to delegate format-specific rendering
/// while handling common elements (tables, lists, inline formatting) generically.
pub struct MarkdownRenderer<B: RenderBackend> {
    output: String,
    /// Stack of nested list types (true = ordered, false = unordered).
    list_stack: Vec<bool>,
    /// Code block rendering state.
    code: CodeBlockState,
    /// Table rendering state.
    table: TableState,
    /// Image alt text capture state.
    image: ImageState,
    /// Heading and title extraction state.
    heading: HeadingState,
    /// Base path for resolving relative links.
    base_path: Option<String>,
    /// Pending image data (src, title) waiting for alt text.
    pending_image: Option<(String, String)>,
    /// Phantom data for the backend type.
    _backend: PhantomData<B>,
}

impl<B: RenderBackend> MarkdownRenderer<B> {
    /// Create a new renderer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            list_stack: Vec::new(),
            code: CodeBlockState::default(),
            table: TableState::default(),
            image: ImageState::default(),
            heading: HeadingState::new(false, B::TITLE_AS_METADATA),
            base_path: None,
            pending_image: None,
            _backend: PhantomData,
        }
    }

    /// Enable title extraction from first H1 heading.
    ///
    /// Behavior depends on the backend:
    /// - HTML: First H1 is extracted as title but still rendered
    /// - Confluence: First H1 is extracted as title and skipped, levels shifted
    #[must_use]
    pub fn with_title_extraction(mut self) -> Self {
        self.heading = HeadingState::new(true, B::TITLE_AS_METADATA);
        self
    }

    /// Set base path for resolving relative links.
    ///
    /// Only used by HTML backend. Confluence backend ignores this.
    #[must_use]
    pub fn with_base_path(mut self, path: impl Into<String>) -> Self {
        self.base_path = Some(path.into());
        self
    }

    /// Push content to output or heading buffer based on context.
    fn push_inline(&mut self, content: &str) {
        if self.heading.is_active() {
            self.heading.push_html(content);
        } else {
            self.output.push_str(content);
        }
    }

    /// Render markdown events and return the result.
    pub fn render<'a, I>(mut self, events: I) -> RenderResult
    where
        I: Iterator<Item = Event<'a>>,
    {
        for event in events {
            self.process_event(event);
        }
        RenderResult {
            html: self.output,
            title: self.heading.take_title(),
            toc: self.heading.take_toc(),
        }
    }

    fn process_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.inline_code(&code),
            Event::Html(html) | Event::InlineHtml(html) => self.raw_html(&html),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.horizontal_rule(),
            Event::TaskListMarker(checked) => self.task_list_marker(checked),
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                // Not supported
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                if !self.code.is_active() {
                    self.output.push_str("<p>");
                }
            }
            Tag::Heading { level, .. } => {
                // Start heading tracking. If false, we're capturing first H1 for title.
                // Opening tag is written in end_tag after we have the ID.
                self.heading.start_heading(heading_level_to_num(level));
            }
            Tag::BlockQuote(_) => {
                B::blockquote_start(&mut self.output);
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(ref lang) if !lang.is_empty() => {
                        lang.split_whitespace().next().map(str::to_string)
                    }
                    _ => None,
                };
                self.code.start(lang);
            }
            Tag::List(start) => {
                self.list_stack.push(start.is_some());
                match start {
                    Some(1) => self.output.push_str("<ol>"),
                    Some(n) => write!(self.output, r#"<ol start="{n}">"#).unwrap(),
                    None => self.output.push_str("<ul>"),
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
            Tag::Table(alignments) => {
                self.table.start(alignments.clone());
                self.output.push_str("<table>");
            }
            Tag::TableHead => {
                self.table.start_head();
                self.output.push_str("<thead><tr>");
            }
            Tag::TableRow => {
                self.table.start_row();
                self.output.push_str("<tr>");
            }
            Tag::TableCell => {
                let align = self.table.current_alignment_style();
                let tag = if self.table.is_in_head() { "th" } else { "td" };
                write!(self.output, "<{tag}{align}>").unwrap();
            }
            Tag::Emphasis => self.push_inline("<em>"),
            Tag::Strong => self.push_inline("<strong>"),
            Tag::Strikethrough => self.push_inline("<del>"),
            Tag::Link { dest_url, .. } => {
                let href = B::transform_link(&dest_url, self.base_path.as_deref());
                let link_tag = format!(r#"<a href="{}">"#, escape_html(&href));
                self.push_inline(&link_tag);
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                // Start collecting alt text; image will be rendered in end_tag
                self.image.start();
                self.pending_image = Some((dest_url.to_string(), title.to_string()));
            }
            Tag::Superscript => self.push_inline("<sup>"),
            Tag::Subscript => self.push_inline("<sub>"),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if !self.code.is_active() {
                    self.output.push_str("</p>");
                }
            }
            TagEnd::Heading(_level) => {
                if self.heading.is_in_first_h1() {
                    // Complete first H1 capture for Confluence mode
                    self.heading.complete_first_h1();
                } else if let Some((level, id, _text, html)) = self.heading.complete_heading() {
                    // Write heading with ID
                    write!(
                        self.output,
                        r#"<h{level} id="{id}">{}</h{level}>"#,
                        html.trim()
                    )
                    .unwrap();
                }
            }
            TagEnd::BlockQuote(_) => {
                B::blockquote_end(&mut self.output);
            }
            TagEnd::CodeBlock => {
                let (lang, content) = self.code.end();
                B::code_block(lang.as_deref(), &content, &mut self.output);
            }
            TagEnd::List(ordered) => {
                self.list_stack.pop();
                self.output
                    .push_str(if ordered { "</ol>" } else { "</ul>" });
            }
            TagEnd::Item => {
                self.output.push_str("</li>");
            }
            TagEnd::FootnoteDefinition | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
            TagEnd::Image => {
                // Render image with collected alt text
                let alt = self.image.end();
                if let Some((src, title)) = self.pending_image.take() {
                    B::image(&src, &alt, &title, &mut self.output);
                }
            }
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
                self.output.push_str("</tr></thead><tbody>");
                self.table.end_head();
            }
            TagEnd::TableRow => {
                self.output.push_str("</tr>");
            }
            TagEnd::TableCell => {
                self.output.push_str(if self.table.is_in_head() {
                    "</th>"
                } else {
                    "</td>"
                });
                self.table.next_cell();
            }
            TagEnd::Emphasis => self.push_inline("</em>"),
            TagEnd::Strong => self.push_inline("</strong>"),
            TagEnd::Strikethrough => self.push_inline("</del>"),
            TagEnd::Link => self.push_inline("</a>"),
            TagEnd::Superscript => self.push_inline("</sup>"),
            TagEnd::Subscript => self.push_inline("</sub>"),
        }
    }

    fn text(&mut self, text: &str) {
        if self.code.is_active() {
            // Buffer code content
            self.code.push_str(text);
        } else if self.image.is_active() {
            // Capture alt text
            self.image.push_str(text);
        } else if self.heading.is_in_first_h1() {
            // Capture first H1 text for title (Confluence mode)
            self.heading.push_text(text);
        } else if self.heading.is_active() {
            // Capture heading text and HTML
            self.heading.push_text(text);
            self.heading.push_html(&escape_html(text));
        } else {
            self.output.push_str(&escape_html(text));
        }
    }

    fn inline_code(&mut self, code: &str) {
        if self.heading.is_active() {
            self.heading.push_text(code);
            write!(
                self.heading.html_buffer(),
                "<code>{}</code>",
                escape_html(code)
            )
            .unwrap();
        } else {
            write!(self.output, "<code>{}</code>", escape_html(code)).unwrap();
        }
    }

    fn raw_html(&mut self, html: &str) {
        self.output.push_str(html);
    }

    fn soft_break(&mut self) {
        if self.code.is_active() {
            self.code.push_newline();
        } else {
            self.output.push('\n');
        }
    }

    fn hard_break(&mut self) {
        B::hard_break(&mut self.output);
    }

    fn horizontal_rule(&mut self) {
        B::horizontal_rule(&mut self.output);
    }

    fn task_list_marker(&mut self, checked: bool) {
        B::task_list_marker(checked, &mut self.output);
    }
}

impl<B: RenderBackend> Default for MarkdownRenderer<B> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::{Options, Parser};

    fn render_html(markdown: &str) -> RenderResult {
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
        let parser = Parser::new_ext(markdown, options);
        MarkdownRenderer::<HtmlBackend>::new().render(parser)
    }

    fn render_html_with_title(markdown: &str) -> RenderResult {
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
        let parser = Parser::new_ext(markdown, options);
        MarkdownRenderer::<HtmlBackend>::new()
            .with_title_extraction()
            .render(parser)
    }

    fn render_confluence(markdown: &str) -> RenderResult {
        let parser = Parser::new(markdown);
        MarkdownRenderer::<ConfluenceBackend>::new().render(parser)
    }

    fn render_confluence_with_title(markdown: &str) -> RenderResult {
        let parser = Parser::new(markdown);
        MarkdownRenderer::<ConfluenceBackend>::new()
            .with_title_extraction()
            .render(parser)
    }

    // HTML backend tests

    #[test]
    fn test_html_basic_paragraph() {
        let result = render_html("Hello, world!");
        assert_eq!(result.html, "<p>Hello, world!</p>");
    }

    #[test]
    fn test_html_heading_with_id() {
        let result = render_html("## Section Title");
        assert_eq!(result.html, r#"<h2 id="section-title">Section Title</h2>"#);
        assert_eq!(result.toc.len(), 1);
        assert_eq!(result.toc[0].level, 2);
        assert_eq!(result.toc[0].title, "Section Title");
        assert_eq!(result.toc[0].id, "section-title");
    }

    #[test]
    fn test_html_title_extraction() {
        let markdown = "# My Title\n\nSome content\n\n## Section";
        let result = render_html_with_title(markdown);

        assert_eq!(result.title, Some("My Title".to_string()));
        // H1 is still rendered in HTML mode
        assert!(result.html.contains(r#"<h1 id="my-title">My Title</h1>"#));
        // ToC excludes title but includes other headings
        assert_eq!(result.toc.len(), 1);
        assert_eq!(result.toc[0].level, 2);
    }

    #[test]
    fn test_html_code_block() {
        let result = render_html("```rust\nfn main() {}\n```");
        assert!(result.html.contains(r#"class="language-rust""#));
        assert!(result.html.contains("fn main() {}"));
    }

    #[test]
    fn test_html_blockquote() {
        let result = render_html("> Note");
        assert!(result.html.contains("<blockquote>"));
        assert!(result.html.contains("</blockquote>"));
    }

    #[test]
    fn test_html_image() {
        let result = render_html("![Alt text](image.png)");
        assert!(
            result
                .html
                .contains(r#"<img src="image.png" alt="Alt text">"#)
        );
    }

    #[test]
    fn test_html_table() {
        let result = render_html("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(result.html.contains("<table>"));
        assert!(result.html.contains("<thead>"));
        assert!(result.html.contains("<th>"));
        assert!(result.html.contains("<tbody>"));
        assert!(result.html.contains("<td>"));
    }

    #[test]
    fn test_html_link_with_base_path() {
        let options = Options::ENABLE_TABLES;
        let parser = Parser::new_ext("[Link](./page.md)", options);
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("base/path")
            .render(parser);
        assert!(result.html.contains(r#"href="/base/path/page""#));
    }

    // Confluence backend tests

    #[test]
    fn test_confluence_basic_paragraph() {
        let result = render_confluence("Hello, world!");
        assert_eq!(result.html, "<p>Hello, world!</p>");
    }

    #[test]
    fn test_confluence_heading() {
        let result = render_confluence("## Title");
        assert!(result.html.contains("<h2"));
        assert!(result.html.contains("</h2>"));
    }

    #[test]
    fn test_confluence_title_extraction() {
        let markdown = "# My Title\n\nSome content\n\n## Section\n\n### Subsection";
        let result = render_confluence_with_title(markdown);

        assert_eq!(result.title, Some("My Title".to_string()));
        // H1 is NOT rendered in Confluence mode
        assert!(!result.html.contains("My Title"));
        // Levels are shifted: H2→H1, H3→H2
        assert!(result.html.contains("<h1"));
        assert!(result.html.contains("Section"));
        assert!(result.html.contains("<h2"));
        assert!(result.html.contains("Subsection"));
    }

    #[test]
    fn test_confluence_code_block() {
        let result = render_confluence("```python\nprint('hello')\n```");
        assert!(result.html.contains(r#"ac:name="code""#));
        assert!(result.html.contains(r#"ac:name="language">python"#));
        assert!(result.html.contains("<![CDATA["));
    }

    #[test]
    fn test_confluence_blockquote() {
        let result = render_confluence("> Note");
        assert!(result.html.contains(r#"ac:name="info""#));
    }

    #[test]
    fn test_confluence_external_image() {
        let result = render_confluence("![alt](https://example.com/image.png)");
        assert!(result.html.contains(r"<ac:image>"));
        assert!(
            result
                .html
                .contains(r#"ri:url ri:value="https://example.com/image.png""#)
        );
    }

    #[test]
    fn test_confluence_local_image() {
        let result = render_confluence("![alt](./images/diagram.png)");
        assert!(result.html.contains(r"<ac:image>"));
        assert!(
            result
                .html
                .contains(r#"ri:attachment ri:filename="diagram.png""#)
        );
    }

    #[test]
    fn test_confluence_hard_break() {
        let result = render_confluence("Line one  \nLine two");
        assert!(result.html.contains("<br />"));
    }

    #[test]
    fn test_confluence_horizontal_rule() {
        let result = render_confluence("---");
        assert!(result.html.contains("<hr />"));
    }

    // Common functionality tests

    #[test]
    fn test_duplicate_heading_ids() {
        let result = render_html("## FAQ\n\n## FAQ\n\n## FAQ");
        assert_eq!(result.toc.len(), 3);
        assert_eq!(result.toc[0].id, "faq");
        assert_eq!(result.toc[1].id, "faq-1");
        assert_eq!(result.toc[2].id, "faq-2");
    }

    #[test]
    fn test_heading_with_inline_code() {
        let result = render_html("## Install `npm`");
        assert!(result.html.contains("<code>npm</code>"));
        assert_eq!(result.toc[0].title, "Install npm");
    }

    #[test]
    fn test_emphasis() {
        let result = render_html("*italic* and **bold**");
        assert!(result.html.contains("<em>italic</em>"));
        assert!(result.html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn test_strikethrough() {
        let result = render_html("~~deleted~~");
        assert!(result.html.contains("<del>deleted</del>"));
    }

    #[test]
    fn test_lists() {
        let result = render_html("- Item 1\n- Item 2");
        assert!(result.html.contains("<ul>"));
        assert!(result.html.contains("<li>"));
        assert!(result.html.contains("</ul>"));

        let result = render_html("1. First\n2. Second");
        assert!(result.html.contains("<ol>"));
        assert!(result.html.contains("</ol>"));
    }

    #[test]
    fn test_task_list_html() {
        let options = Options::ENABLE_TASKLISTS;
        let parser = Parser::new_ext("- [ ] Unchecked\n- [x] Checked", options);
        let result = MarkdownRenderer::<HtmlBackend>::new().render(parser);
        assert!(result.html.contains(r#"<input type="checkbox" disabled>"#));
        assert!(
            result
                .html
                .contains(r#"<input type="checkbox" checked disabled>"#)
        );
    }

    #[test]
    fn test_task_list_confluence() {
        let options = Options::ENABLE_TASKLISTS;
        let parser = Parser::new_ext("- [ ] Unchecked\n- [x] Checked", options);
        let result = MarkdownRenderer::<ConfluenceBackend>::new().render(parser);
        assert!(result.html.contains("[ ] Unchecked"));
        assert!(result.html.contains("[x] Checked"));
    }

    #[test]
    fn test_default_renderer() {
        let parser = Parser::new("Hello");
        let result = MarkdownRenderer::<HtmlBackend>::default().render(parser);
        assert_eq!(result.html, "<p>Hello</p>");
    }
}
