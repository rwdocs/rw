//! HTML renderer for pulldown-cmark events.
//!
//! Produces semantic HTML5 with syntax highlighting and table of contents generation.

use std::collections::HashMap;
use std::fmt::Write;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;
use syntect::parsing::SyntaxSet;

/// Table of contents entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TocEntry {
    /// Heading level (1-6).
    pub level: u8,
    /// Heading text.
    pub title: String,
    /// Anchor ID for linking.
    pub id: String,
}

/// Result of rendering markdown to HTML format.
#[derive(Clone, Debug)]
pub struct HtmlRenderResult {
    /// Rendered HTML content.
    pub html: String,
    /// Title extracted from first H1 heading (if `extract_title` was enabled).
    pub title: Option<String>,
    /// Table of contents entries.
    pub toc: Vec<TocEntry>,
}

/// Syntax highlighter with cached syntaxes and theme.
pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme_name: String,
}

impl SyntaxHighlighter {
    /// Create a new syntax highlighter with the given theme.
    #[must_use]
    pub fn new(theme_name: &str) -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            theme_name: theme_name.to_string(),
        }
    }

    /// Highlight code with the given language.
    ///
    /// Returns highlighted HTML or falls back to escaped code if language is unknown.
    pub fn highlight(&self, code: &str, lang: Option<&str>) -> String {
        let syntax = lang
            .and_then(|l| self.syntax_set.find_syntax_by_token(l))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = self
            .theme_set
            .themes
            .get(&self.theme_name)
            .unwrap_or_else(|| {
                self.theme_set
                    .themes
                    .get("base16-ocean.dark")
                    .expect("default theme should exist")
            });

        highlighted_html_for_string(code, &self.syntax_set, syntax, theme)
            .unwrap_or_else(|_| format!("<pre><code>{}</code></pre>", escape_html(code)))
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new("base16-ocean.dark")
    }
}

/// Renders pulldown-cmark events to semantic HTML5.
#[allow(clippy::struct_excessive_bools)]
pub struct HtmlRenderer {
    output: String,
    /// Stack of nested list types (true = ordered, false = unordered).
    list_stack: Vec<bool>,
    /// Whether we're inside a code block.
    in_code_block: bool,
    /// Language of current code block.
    code_language: Option<String>,
    /// Buffer for code block content.
    code_buffer: String,
    /// Whether we're inside a table header row.
    in_table_head: bool,
    /// Column alignments for current table.
    table_alignments: Vec<Alignment>,
    /// Current column index in table row.
    table_cell_index: usize,
    /// Whether we're inside an image tag (to capture alt text).
    in_image: bool,
    /// Buffer for image alt text.
    image_alt: String,
    /// Whether to extract title from first H1 and level up headers.
    extract_title: bool,
    /// Extracted title from first H1.
    title: Option<String>,
    /// Whether we've seen the first H1.
    seen_first_h1: bool,
    /// Whether we're currently inside the first H1 (to capture its text).
    in_first_h1: bool,
    /// Buffer for first H1 text.
    h1_text: String,
    /// Current heading level being processed.
    current_heading_level: Option<u8>,
    /// Buffer for current heading text (plain text for ToC and slug).
    heading_text: String,
    /// Buffer for current heading HTML (with inline formatting).
    heading_html: String,
    /// Table of contents entries.
    toc: Vec<TocEntry>,
    /// Counter for generating unique heading IDs.
    heading_counts: HashMap<String, usize>,
    /// Syntax highlighter for code blocks.
    highlighter: SyntaxHighlighter,
}

impl HtmlRenderer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            list_stack: Vec::new(),
            in_code_block: false,
            code_language: None,
            code_buffer: String::new(),
            in_table_head: false,
            table_alignments: Vec::new(),
            table_cell_index: 0,
            in_image: false,
            image_alt: String::new(),
            extract_title: false,
            title: None,
            seen_first_h1: false,
            in_first_h1: false,
            h1_text: String::new(),
            current_heading_level: None,
            heading_text: String::new(),
            heading_html: String::new(),
            toc: Vec::new(),
            heading_counts: HashMap::new(),
            highlighter: SyntaxHighlighter::default(),
        }
    }

    /// Enable title extraction from first H1 heading.
    ///
    /// When enabled, the first H1 is extracted as title and not rendered,
    /// and all other headers are leveled up (H2->H1, H3->H2, etc.).
    #[must_use]
    pub fn with_title_extraction(mut self) -> Self {
        self.extract_title = true;
        self
    }

    /// Set the syntax highlighting theme.
    #[must_use]
    pub fn with_theme(mut self, theme_name: &str) -> Self {
        self.highlighter = SyntaxHighlighter::new(theme_name);
        self
    }

    /// Render markdown events and return HTML, extracted title, and table of contents.
    pub fn render<'a, I>(mut self, events: I) -> HtmlRenderResult
    where
        I: Iterator<Item = Event<'a>>,
    {
        for event in events {
            self.process_event(event);
        }
        HtmlRenderResult {
            html: self.output,
            title: self.title,
            toc: self.toc,
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
                // Intentionally not supported: footnotes require multi-pass rendering,
                // math support would need KaTeX/MathJax integration
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
                    // Track heading for ToC
                    self.current_heading_level = Some(heading_level_to_num(level));
                    self.heading_text.clear();
                    self.heading_html.clear();
                }
            }
            Tag::BlockQuote(_) => {
                self.output.push_str("<blockquote>");
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                        Some(lang.split_whitespace().next().unwrap_or("").to_string())
                    }
                    _ => None,
                };
                self.in_code_block = true;
                self.code_language = lang;
                self.code_buffer.clear();
            }
            Tag::List(start) => {
                let ordered = start.is_some();
                self.list_stack.push(ordered);
                if ordered {
                    if let Some(n) = start {
                        if n == 1 {
                            self.output.push_str("<ol>");
                        } else {
                            write!(self.output, r#"<ol start="{n}">"#).unwrap();
                        }
                    }
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
            Tag::Table(alignments) => {
                self.table_alignments = alignments.to_vec();
                self.output.push_str("<table>");
            }
            Tag::TableHead => {
                self.in_table_head = true;
                self.table_cell_index = 0;
                self.output.push_str("<thead><tr>");
            }
            Tag::TableRow => {
                self.table_cell_index = 0;
                self.output.push_str("<tr>");
            }
            Tag::TableCell => {
                let align_style = self
                    .table_alignments
                    .get(self.table_cell_index)
                    .and_then(|a| match a {
                        Alignment::Left => Some(" style=\"text-align:left\""),
                        Alignment::Center => Some(" style=\"text-align:center\""),
                        Alignment::Right => Some(" style=\"text-align:right\""),
                        Alignment::None => None,
                    })
                    .unwrap_or("");

                if self.in_table_head {
                    write!(self.output, "<th{align_style}>").unwrap();
                } else {
                    write!(self.output, "<td{align_style}>").unwrap();
                }
            }
            Tag::Emphasis => {
                if self.current_heading_level.is_some() {
                    self.heading_html.push_str("<em>");
                } else {
                    self.output.push_str("<em>");
                }
            }
            Tag::Strong => {
                if self.current_heading_level.is_some() {
                    self.heading_html.push_str("<strong>");
                } else {
                    self.output.push_str("<strong>");
                }
            }
            Tag::Strikethrough => {
                if self.current_heading_level.is_some() {
                    self.heading_html.push_str("<del>");
                } else {
                    self.output.push_str("<del>");
                }
            }
            Tag::Link { dest_url, .. } => {
                if self.current_heading_level.is_some() {
                    write!(
                        self.heading_html,
                        r#"<a href="{}">"#,
                        escape_html(&dest_url)
                    )
                    .unwrap();
                } else {
                    write!(self.output, r#"<a href="{}">"#, escape_html(&dest_url)).unwrap();
                }
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                // Start collecting alt text; image will be closed in end_tag
                self.in_image = true;
                self.image_alt.clear();
                write!(self.output, r#"<img src="{}""#, escape_html(&dest_url)).unwrap();
                if !title.is_empty() {
                    write!(self.output, r#" title="{}""#, escape_html(&title)).unwrap();
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
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
                } else if let Some(level_num) = self.current_heading_level.take() {
                    // Take ownership of heading text and HTML to avoid borrow conflicts
                    let heading_text = std::mem::take(&mut self.heading_text);
                    let heading_html = std::mem::take(&mut self.heading_html);

                    // Generate ID from heading text
                    let id = self.generate_heading_id(&heading_text);

                    // Adjust level if we extracted a title
                    let adjusted_level =
                        if self.extract_title && self.seen_first_h1 && level_num > 1 {
                            level_num - 1
                        } else {
                            level_num
                        };

                    // Add to ToC (plain text title)
                    self.toc.push(TocEntry {
                        level: adjusted_level,
                        title: heading_text.trim().to_string(),
                        id: id.clone(),
                    });

                    // Render heading with ID and inline formatting
                    write!(
                        self.output,
                        r#"<h{adjusted_level} id="{id}">{}</h{adjusted_level}>"#,
                        heading_html.trim()
                    )
                    .unwrap();
                } else {
                    // Fallback - shouldn't happen
                    let level_num = heading_level_to_num(level);
                    write!(self.output, "</h{level_num}>").unwrap();
                }
            }
            TagEnd::BlockQuote(_) => {
                self.output.push_str("</blockquote>");
            }
            TagEnd::CodeBlock => {
                let highlighted = self
                    .highlighter
                    .highlight(&self.code_buffer, self.code_language.as_deref());
                self.output.push_str(&highlighted);
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
            TagEnd::FootnoteDefinition | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
            TagEnd::Image => {
                // Close the image tag with collected alt text
                write!(self.output, r#" alt="{}">"#, escape_html(&self.image_alt)).unwrap();
                self.in_image = false;
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
                self.table_cell_index += 1;
            }
            TagEnd::Emphasis => {
                if self.current_heading_level.is_some() {
                    self.heading_html.push_str("</em>");
                } else {
                    self.output.push_str("</em>");
                }
            }
            TagEnd::Strong => {
                if self.current_heading_level.is_some() {
                    self.heading_html.push_str("</strong>");
                } else {
                    self.output.push_str("</strong>");
                }
            }
            TagEnd::Strikethrough => {
                if self.current_heading_level.is_some() {
                    self.heading_html.push_str("</del>");
                } else {
                    self.output.push_str("</del>");
                }
            }
            TagEnd::Link => {
                if self.current_heading_level.is_some() {
                    self.heading_html.push_str("</a>");
                } else {
                    self.output.push_str("</a>");
                }
            }
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_first_h1 {
            // Capture text for title
            self.h1_text.push_str(text);
        } else if self.in_code_block {
            // Buffer code for syntax highlighting
            self.code_buffer.push_str(text);
        } else if self.in_image {
            // Capture alt text for image
            self.image_alt.push_str(text);
        } else if self.current_heading_level.is_some() {
            // Capture heading text for ToC and HTML for rendering
            self.heading_text.push_str(text);
            self.heading_html.push_str(&escape_html(text));
        } else {
            self.output.push_str(&escape_html(text));
        }
    }

    fn inline_code(&mut self, code: &str) {
        if self.current_heading_level.is_some() {
            // Buffer plain text for ToC and HTML for rendering
            self.heading_text.push_str(code);
            write!(self.heading_html, "<code>{}</code>", escape_html(code)).unwrap();
        } else {
            write!(self.output, "<code>{}</code>", escape_html(code)).unwrap();
        }
    }

    fn raw_html(&mut self, html: &str) {
        // Pass through HTML as-is
        self.output.push_str(html);
    }

    fn soft_break(&mut self) {
        if self.in_code_block {
            self.code_buffer.push('\n');
        } else {
            self.output.push('\n');
        }
    }

    fn hard_break(&mut self) {
        self.output.push_str("<br>");
    }

    fn horizontal_rule(&mut self) {
        self.output.push_str("<hr>");
    }

    fn task_list_marker(&mut self, checked: bool) {
        if checked {
            self.output
                .push_str(r#"<input type="checkbox" checked disabled> "#);
        } else {
            self.output.push_str(r#"<input type="checkbox" disabled> "#);
        }
    }

    /// Generate a unique ID for a heading.
    fn generate_heading_id(&mut self, text: &str) -> String {
        let base_id = slugify(text);
        let count = self.heading_counts.entry(base_id.clone()).or_insert(0);
        *count += 1;

        if *count == 1 {
            base_id
        } else {
            format!("{base_id}-{}", *count - 1)
        }
    }
}

impl Default for HtmlRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert heading level enum to number.
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

/// Convert text to URL-safe slug.
///
/// Converts to lowercase, replaces whitespace/dashes/underscores with single dashes,
/// and removes other non-alphanumeric characters.
fn slugify(text: &str) -> String {
    let mut result = String::new();
    let mut last_was_dash = true; // Prevents leading dash

    for c in text.trim().chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && (c.is_whitespace() || c == '-' || c == '_') {
            result.push('-');
            last_was_dash = true;
        }
    }

    // Remove trailing dash if present
    if result.ends_with('-') {
        result.pop();
    }

    result
}

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#x27;"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::{Options, Parser};

    fn render(markdown: &str) -> HtmlRenderResult {
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
        let parser = Parser::new_ext(markdown, options);
        HtmlRenderer::new().render(parser)
    }

    fn render_with_title(markdown: &str) -> HtmlRenderResult {
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
        let parser = Parser::new_ext(markdown, options);
        HtmlRenderer::new().with_title_extraction().render(parser)
    }

    #[test]
    fn test_basic_paragraph() {
        let result = render("Hello, world!");
        assert_eq!(result.html, "<p>Hello, world!</p>");
        assert!(result.toc.is_empty());
    }

    #[test]
    fn test_heading_with_id() {
        let result = render("## Section Title");
        assert_eq!(result.html, r#"<h2 id="section-title">Section Title</h2>"#);
        assert_eq!(result.toc.len(), 1);
        assert_eq!(result.toc[0].level, 2);
        assert_eq!(result.toc[0].title, "Section Title");
        assert_eq!(result.toc[0].id, "section-title");
    }

    #[test]
    fn test_duplicate_heading_ids() {
        let result = render("## FAQ\n\nContent\n\n## FAQ\n\nMore content\n\n## FAQ");
        assert_eq!(result.toc.len(), 3);
        assert_eq!(result.toc[0].id, "faq");
        assert_eq!(result.toc[1].id, "faq-1");
        assert_eq!(result.toc[2].id, "faq-2");
    }

    #[test]
    fn test_title_extraction() {
        let markdown = "# My Title\n\nSome content\n\n## Section\n\n### Subsection";
        let result = render_with_title(markdown);

        assert_eq!(result.title, Some("My Title".to_string()));
        assert!(!result.html.contains("My Title"));
        assert!(result.html.contains(r#"<h1 id="section">Section</h1>"#)); // H2 -> H1
        assert!(
            result
                .html
                .contains(r#"<h2 id="subsection">Subsection</h2>"#)
        ); // H3 -> H2

        // ToC should have adjusted levels
        assert_eq!(result.toc.len(), 2);
        assert_eq!(result.toc[0].level, 1);
        assert_eq!(result.toc[1].level, 2);
    }

    #[test]
    fn test_code_block_syntax_highlighting() {
        let result = render("```rust\nfn main() {}\n```");
        // Should contain syntax-highlighted output
        assert!(result.html.contains("<pre"));
        assert!(result.html.contains("fn"));
    }

    #[test]
    fn test_inline_code() {
        let result = render("Use `println!` macro");
        assert!(result.html.contains("<code>println!</code>"));
    }

    #[test]
    fn test_heading_with_inline_code() {
        let result = render("## Install `npm`");
        assert_eq!(
            result.html,
            r#"<h2 id="install-npm">Install <code>npm</code></h2>"#
        );
        assert_eq!(result.toc.len(), 1);
        assert_eq!(result.toc[0].title, "Install npm");
        assert_eq!(result.toc[0].id, "install-npm");
    }

    #[test]
    fn test_heading_with_emphasis() {
        let result = render("## *Important* Section");
        assert_eq!(
            result.html,
            r#"<h2 id="important-section"><em>Important</em> Section</h2>"#
        );
        assert_eq!(result.toc[0].title, "Important Section");
    }

    #[test]
    fn test_heading_with_strong() {
        let result = render("## **Bold** Title");
        assert_eq!(
            result.html,
            r#"<h2 id="bold-title"><strong>Bold</strong> Title</h2>"#
        );
        assert_eq!(result.toc[0].title, "Bold Title");
    }

    #[test]
    fn test_heading_with_link() {
        let result = render("## See [Docs](https://example.com)");
        assert_eq!(
            result.html,
            r#"<h2 id="see-docs">See <a href="https://example.com">Docs</a></h2>"#
        );
        assert_eq!(result.toc[0].title, "See Docs");
    }

    #[test]
    fn test_blockquote() {
        let result = render("> Note: Important");
        assert!(result.html.contains("<blockquote>"));
        assert!(result.html.contains("</blockquote>"));
    }

    #[test]
    fn test_links() {
        let result = render("[Rust](https://rust-lang.org)");
        assert!(
            result
                .html
                .contains(r#"<a href="https://rust-lang.org">Rust</a>"#)
        );
    }

    #[test]
    fn test_images() {
        let result = render("![Alt text](image.png)");
        assert!(
            result
                .html
                .contains(r#"<img src="image.png" alt="Alt text">"#)
        );
    }

    #[test]
    fn test_images_with_title() {
        let result = render(r#"![Alt text](image.png "Image title")"#);
        assert!(
            result
                .html
                .contains(r#"<img src="image.png" title="Image title" alt="Alt text">"#)
        );
    }

    #[test]
    fn test_strikethrough() {
        let result = render("~~deleted~~");
        assert!(result.html.contains("<del>deleted</del>"));
    }

    #[test]
    fn test_table() {
        let result = render("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(result.html.contains("<table>"));
        assert!(result.html.contains("<thead>"));
        assert!(result.html.contains("<th>"));
        assert!(result.html.contains("<tbody>"));
        assert!(result.html.contains("<td>"));
    }

    #[test]
    fn test_table_alignment() {
        let result = render("| Left | Center | Right |\n|:-----|:------:|------:|\n| a | b | c |");
        assert!(
            result
                .html
                .contains(r#"<th style="text-align:left">Left</th>"#)
        );
        assert!(
            result
                .html
                .contains(r#"<th style="text-align:center">Center</th>"#)
        );
        assert!(
            result
                .html
                .contains(r#"<th style="text-align:right">Right</th>"#)
        );
        assert!(
            result
                .html
                .contains(r#"<td style="text-align:left">a</td>"#)
        );
        assert!(
            result
                .html
                .contains(r#"<td style="text-align:center">b</td>"#)
        );
        assert!(
            result
                .html
                .contains(r#"<td style="text-align:right">c</td>"#)
        );
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("What's New?"), "whats-new");
        assert_eq!(slugify("  Spaces  "), "spaces");
        assert_eq!(slugify("Multiple   Spaces"), "multiple-spaces");
        assert_eq!(slugify("kebab-case"), "kebab-case");
        assert_eq!(slugify("snake_case"), "snake-case");
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html(r#""quoted""#), "&quot;quoted&quot;");
    }
}
