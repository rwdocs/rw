//! HTML renderer for pulldown-cmark events.
//!
//! Produces semantic HTML5 with table of contents generation.
//!
//! # Architecture
//!
//! The renderer uses a state machine pattern to track context during event processing:
//! - `CodeBlockState`: Tracks code block language and content buffering
//! - `TableState`: Tracks table headers, cell alignments, and current cell index
//! - `ImageState`: Captures alt text while inside image tags
//! - `HeadingState`: Handles title extraction, table of contents generation, and inline formatting
//!
//! The separation of state into focused structs makes the renderer easier to understand
//! and maintain compared to a flat collection of boolean flags.

use std::collections::HashMap;
use std::fmt::Write;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};

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

/// State for tracking code block rendering.
#[derive(Default)]
struct CodeBlockState {
    /// Whether we're inside a code block.
    active: bool,
    /// Language of current code block (e.g., "rust", "python").
    language: Option<String>,
    /// Buffer for code block content.
    buffer: String,
}

impl CodeBlockState {
    fn start(&mut self, language: Option<String>) {
        self.active = true;
        self.language = language;
        self.buffer.clear();
    }

    fn end(&mut self) -> (Option<String>, String) {
        self.active = false;
        (self.language.take(), std::mem::take(&mut self.buffer))
    }
}

/// State for tracking table rendering.
#[derive(Default)]
struct TableState {
    /// Whether we're inside the table header row.
    in_head: bool,
    /// Column alignments for current table.
    alignments: Vec<Alignment>,
    /// Current column index in table row.
    cell_index: usize,
}

impl TableState {
    fn start(&mut self, alignments: Vec<Alignment>) {
        self.alignments = alignments;
        self.in_head = false;
        self.cell_index = 0;
    }

    fn start_head(&mut self) {
        self.in_head = true;
        self.cell_index = 0;
    }

    fn end_head(&mut self) {
        self.in_head = false;
    }

    fn start_row(&mut self) {
        self.cell_index = 0;
    }

    fn next_cell(&mut self) {
        self.cell_index += 1;
    }

    fn current_alignment(&self) -> Option<&Alignment> {
        self.alignments.get(self.cell_index)
    }
}

/// State for tracking image alt text capture.
#[derive(Default)]
struct ImageState {
    /// Whether we're inside an image tag.
    active: bool,
    /// Buffer for alt text.
    alt_text: String,
}

impl ImageState {
    fn start(&mut self) {
        self.active = true;
        self.alt_text.clear();
    }

    fn end(&mut self) -> String {
        self.active = false;
        std::mem::take(&mut self.alt_text)
    }
}

/// State for tracking heading and title extraction.
struct HeadingState {
    /// Whether to extract title from first H1.
    extract_title: bool,
    /// Extracted title from first H1.
    title: Option<String>,
    /// Current heading level being processed (None if not in a heading).
    current_level: Option<u8>,
    /// Buffer for heading plain text (for table of contents and slug).
    text: String,
    /// Buffer for heading HTML (with inline formatting).
    html: String,
    /// Table of contents entries.
    toc: Vec<TocEntry>,
    /// Counter for generating unique heading IDs.
    id_counts: HashMap<String, usize>,
}

impl HeadingState {
    fn new(extract_title: bool) -> Self {
        Self {
            extract_title,
            title: None,
            current_level: None,
            text: String::new(),
            html: String::new(),
            toc: Vec::new(),
            id_counts: HashMap::new(),
        }
    }

    /// Check if we're currently inside any heading.
    fn is_active(&self) -> bool {
        self.current_level.is_some()
    }

    /// Start tracking a heading.
    fn start_heading(&mut self, level: u8) {
        self.current_level = Some(level);
        self.text.clear();
        self.html.clear();
    }

    /// Complete heading and generate table of contents entry.
    /// Returns (level, id, text, html) or None if not in a heading.
    fn complete_heading(&mut self) -> Option<(u8, String, String, String)> {
        let level = self.current_level.take()?;
        let text = std::mem::take(&mut self.text);
        let html = std::mem::take(&mut self.html);

        // Generate unique ID
        let id = self.generate_id(&text);

        // Extract title from first H1 (but still render it - no level shifting for HTML)
        let is_title = self.extract_title && level == 1 && self.title.is_none();
        if is_title {
            self.title = Some(text.trim().to_string());
        }

        // Add to ToC (but not the page title)
        if !is_title {
            self.toc.push(TocEntry {
                level,
                title: text.trim().to_string(),
                id: id.clone(),
            });
        }

        Some((level, id, text, html))
    }

    /// Generate a unique ID for a heading.
    fn generate_id(&mut self, text: &str) -> String {
        let base_id = slugify(text);
        let count = self.id_counts.entry(base_id.clone()).or_insert(0);
        *count += 1;

        if *count == 1 {
            base_id
        } else {
            format!("{base_id}-{}", *count - 1)
        }
    }
}

/// Renders pulldown-cmark events to semantic HTML5.
pub struct HtmlRenderer {
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
}

impl HtmlRenderer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            list_stack: Vec::new(),
            code: CodeBlockState::default(),
            table: TableState::default(),
            image: ImageState::default(),
            heading: HeadingState::new(false),
        }
    }

    /// Enable title extraction from first H1 heading.
    ///
    /// When enabled, the first H1 is extracted as title but still rendered.
    /// Subsequent headings keep their original levels.
    /// The title (first H1) is excluded from the table of contents.
    #[must_use]
    pub fn with_title_extraction(mut self) -> Self {
        self.heading = HeadingState::new(true);
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
            title: self.heading.title,
            toc: self.heading.toc,
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
                if !self.code.active {
                    self.output.push_str("<p>");
                }
            }
            Tag::Heading { level, .. } => {
                self.heading.start_heading(heading_level_to_num(level));
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
                self.code.start(lang);
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
                let align_style = self
                    .table
                    .current_alignment()
                    .and_then(|a| match a {
                        Alignment::Left => Some(" style=\"text-align:left\""),
                        Alignment::Center => Some(" style=\"text-align:center\""),
                        Alignment::Right => Some(" style=\"text-align:right\""),
                        Alignment::None => None,
                    })
                    .unwrap_or("");

                if self.table.in_head {
                    write!(self.output, "<th{align_style}>").unwrap();
                } else {
                    write!(self.output, "<td{align_style}>").unwrap();
                }
            }
            Tag::Emphasis => {
                if self.heading.is_active() {
                    self.heading.html.push_str("<em>");
                } else {
                    self.output.push_str("<em>");
                }
            }
            Tag::Strong => {
                if self.heading.is_active() {
                    self.heading.html.push_str("<strong>");
                } else {
                    self.output.push_str("<strong>");
                }
            }
            Tag::Strikethrough => {
                if self.heading.is_active() {
                    self.heading.html.push_str("<del>");
                } else {
                    self.output.push_str("<del>");
                }
            }
            Tag::Link { dest_url, .. } => {
                if self.heading.is_active() {
                    write!(
                        self.heading.html,
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
                self.image.start();
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
                if !self.code.active {
                    self.output.push_str("</p>");
                }
            }
            TagEnd::Heading(level) => {
                if let Some((heading_level, id, _text, html)) = self.heading.complete_heading() {
                    // Render heading with ID and inline formatting
                    write!(
                        self.output,
                        r#"<h{heading_level} id="{id}">{}</h{heading_level}>"#,
                        html.trim()
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
                let (lang, buffer) = self.code.end();
                if let Some(lang) = lang {
                    write!(
                        self.output,
                        r#"<pre><code class="language-{}">{}</code></pre>"#,
                        escape_html(&lang),
                        escape_html(&buffer)
                    )
                    .unwrap();
                } else {
                    write!(
                        self.output,
                        "<pre><code>{}</code></pre>",
                        escape_html(&buffer)
                    )
                    .unwrap();
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
            TagEnd::FootnoteDefinition | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
            TagEnd::Image => {
                // Close the image tag with collected alt text
                let alt_text = self.image.end();
                write!(self.output, r#" alt="{}">"#, escape_html(&alt_text)).unwrap();
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
                if self.table.in_head {
                    self.output.push_str("</th>");
                } else {
                    self.output.push_str("</td>");
                }
                self.table.next_cell();
            }
            TagEnd::Emphasis => {
                if self.heading.is_active() {
                    self.heading.html.push_str("</em>");
                } else {
                    self.output.push_str("</em>");
                }
            }
            TagEnd::Strong => {
                if self.heading.is_active() {
                    self.heading.html.push_str("</strong>");
                } else {
                    self.output.push_str("</strong>");
                }
            }
            TagEnd::Strikethrough => {
                if self.heading.is_active() {
                    self.heading.html.push_str("</del>");
                } else {
                    self.output.push_str("</del>");
                }
            }
            TagEnd::Link => {
                if self.heading.is_active() {
                    self.heading.html.push_str("</a>");
                } else {
                    self.output.push_str("</a>");
                }
            }
        }
    }

    fn text(&mut self, text: &str) {
        if self.code.active {
            // Buffer code for syntax highlighting
            self.code.buffer.push_str(text);
        } else if self.image.active {
            // Capture alt text for image
            self.image.alt_text.push_str(text);
        } else if self.heading.is_active() {
            // Capture heading text for ToC and HTML for rendering
            self.heading.text.push_str(text);
            self.heading.html.push_str(&escape_html(text));
        } else {
            self.output.push_str(&escape_html(text));
        }
    }

    fn inline_code(&mut self, code: &str) {
        if self.heading.is_active() {
            // Buffer plain text for ToC and HTML for rendering
            self.heading.text.push_str(code);
            write!(self.heading.html, "<code>{}</code>", escape_html(code)).unwrap();
        } else {
            write!(self.output, "<code>{}</code>", escape_html(code)).unwrap();
        }
    }

    fn raw_html(&mut self, html: &str) {
        // Pass through HTML as-is
        self.output.push_str(html);
    }

    fn soft_break(&mut self) {
        if self.code.active {
            self.code.buffer.push('\n');
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

        // Title is extracted from first H1
        assert_eq!(result.title, Some("My Title".to_string()));

        // H1 is still rendered in the output (unlike Confluence)
        assert!(result.html.contains(r#"<h1 id="my-title">My Title</h1>"#));
        assert!(result.html.contains(r#"<h2 id="section">Section</h2>"#));
        assert!(
            result
                .html
                .contains(r#"<h3 id="subsection">Subsection</h3>"#)
        );

        // ToC excludes page title, has original levels (no adjustment for HTML)
        assert_eq!(result.toc.len(), 2);
        assert_eq!(result.toc[0].level, 2);
        assert_eq!(result.toc[0].title, "Section");
        assert_eq!(result.toc[1].level, 3);
        assert_eq!(result.toc[1].title, "Subsection");
    }

    #[test]
    fn test_code_block() {
        let result = render("```rust\nfn main() {}\n```");
        assert_eq!(
            result.html,
            r#"<pre><code class="language-rust">fn main() {}
</code></pre>"#
        );
    }

    #[test]
    fn test_code_block_no_language() {
        let result = render("```\nplain code\n```");
        assert_eq!(result.html, "<pre><code>plain code\n</code></pre>");
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
