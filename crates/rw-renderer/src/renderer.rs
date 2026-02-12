//! Generic markdown renderer with pluggable backend.

use std::collections::HashMap;
use std::fmt::Write;
use std::marker::PhantomData;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::backend::{AlertKind, RenderBackend};
use crate::code_block::{CodeBlockProcessor, ProcessResult, parse_fence_info};
use crate::directive::DirectiveProcessor;
use crate::state::{CodeBlockState, HeadingState, ImageState, TableState, TocEntry, escape_html};
use crate::util::heading_level_to_num;
use crate::util::relative_path;

/// Result of rendering markdown.
#[derive(Debug)]
pub struct RenderResult {
    /// Rendered HTML/XHTML content.
    pub html: String,
    /// Title extracted from first H1 heading (if `extract_title` was enabled).
    pub title: Option<String>,
    /// Table of contents entries.
    pub toc: Vec<TocEntry>,
    /// Warnings generated during conversion (e.g., unresolved includes).
    pub warnings: Vec<String>,
}

/// Generic markdown renderer with pluggable backend.
///
/// Uses the [`RenderBackend`] trait to delegate format-specific rendering
/// while handling common elements (tables, lists, inline formatting) generically.
///
/// # Code Block Processors
///
/// Custom code block processing can be added via [`with_processor`](Self::with_processor).
/// Processors are checked in order; the first returning a non-`PassThrough` result wins.
///
/// # Directive Processing
///
/// The renderer supports `CommonMark` directives via [`with_directives`](Self::with_directives).
/// When a directive processor is configured, [`render_markdown`](Self::render_markdown)
/// will preprocess the input to expand directives before pulldown-cmark parsing,
/// and post-process the output to transform intermediate elements.
pub struct MarkdownRenderer<B: RenderBackend> {
    output: String,
    list_stack: Vec<bool>,
    code: CodeBlockState,
    table: TableState,
    image: ImageState,
    heading: HeadingState,
    base_path: Option<String>,
    pending_image: Option<(String, String)>,
    processors: Vec<Box<dyn CodeBlockProcessor>>,
    code_block_index: usize,
    pending_attrs: HashMap<String, String>,
    gfm: bool,
    /// Stack of alert kinds for nested blockquotes (regular blockquote uses None).
    alert_stack: Vec<Option<AlertKind>>,
    /// Optional directive processor for `CommonMark` directives.
    directives: Option<DirectiveProcessor>,
    /// Produce relative links instead of absolute (for static site builds).
    relative_links: bool,
    _backend: PhantomData<B>,
}

impl<B: RenderBackend> MarkdownRenderer<B> {
    /// Create a new renderer with GFM enabled by default.
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
            processors: Vec::new(),
            code_block_index: 0,
            pending_attrs: HashMap::new(),
            gfm: true,
            alert_stack: Vec::new(),
            directives: None,
            relative_links: false,
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

    /// Enable or disable GitHub Flavored Markdown features.
    ///
    /// GFM is enabled by default. When enabled, the parser supports:
    /// - Tables
    /// - Strikethrough (`~~text~~`)
    /// - Task lists (`- [ ] item`)
    #[must_use]
    pub fn with_gfm(mut self, enabled: bool) -> Self {
        self.gfm = enabled;
        self
    }

    /// Configure directive processing for `CommonMark` directives.
    ///
    /// When a directive processor is set, [`render_markdown`](Self::render_markdown)
    /// will:
    /// 1. Preprocess the input to expand directives (inline, leaf, container)
    /// 2. Parse and render the preprocessed markdown
    /// 3. Post-process the output to transform intermediate elements
    ///
    /// # Example
    ///
    /// ```
    /// use rw_renderer::{HtmlBackend, MarkdownRenderer, TabsDirective};
    /// use rw_renderer::directive::DirectiveProcessor;
    ///
    /// let processor = DirectiveProcessor::new()
    ///     .with_container(TabsDirective::new());
    ///
    /// let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
    ///     .with_directives(processor);
    ///
    /// let result = renderer.render_markdown(r#":::tab[A]
    /// Content A
    /// :::tab[B]
    /// Content B
    /// :::"#);
    ///
    /// assert!(result.html.contains(r#"role="tablist""#));
    /// ```
    #[must_use]
    pub fn with_directives(mut self, processor: DirectiveProcessor) -> Self {
        self.directives = Some(processor);
        self
    }

    /// Enable relative link output instead of absolute paths.
    ///
    /// When enabled, resolved links that start with `/` are converted to
    /// relative paths based on the current page's `base_path`. This is needed
    /// for static site builds where pages are served as plain HTML files.
    ///
    /// Default: `false` (absolute paths for SPA navigation).
    #[must_use]
    pub fn with_relative_links(mut self, enabled: bool) -> Self {
        self.relative_links = enabled;
        self
    }

    /// Get parser options based on GFM configuration.
    #[must_use]
    pub fn parser_options(&self) -> Options {
        if self.gfm {
            Options::ENABLE_TABLES
                | Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_GFM
        } else {
            Options::empty()
        }
    }

    /// Create a configured parser for the given markdown text.
    #[must_use]
    pub fn create_parser<'a>(&self, markdown: &'a str) -> Parser<'a> {
        Parser::new_ext(markdown, self.parser_options())
    }

    /// Render markdown text directly using configured parser options.
    ///
    /// If a directive processor is configured via [`with_directives`](Self::with_directives),
    /// this method will:
    /// 1. Preprocess the input to expand directives
    /// 2. Parse and render the preprocessed markdown
    /// 3. Post-process the output to transform intermediate elements
    pub fn render_markdown(&mut self, markdown: &str) -> RenderResult {
        // Phase 1: Preprocess directives (if configured)
        let preprocessed = if let Some(ref mut processor) = self.directives {
            processor.process(markdown)
        } else {
            markdown.to_owned()
        };

        // Phase 2: Parse and render
        let mut result = self.render(self.create_parser(&preprocessed));

        // Phase 3: Post-process directives (if configured)
        if let Some(ref mut processor) = self.directives {
            processor.post_process(&mut result.html);
            result.warnings.extend(processor.warnings());
        }

        result
    }

    /// Add a code block processor.
    ///
    /// Processors are checked in order when a code block is encountered.
    /// The first processor returning a non-`PassThrough` result wins.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_renderer::{
    ///     CodeBlockProcessor, HtmlBackend,
    ///     MarkdownRenderer, ProcessResult,
    /// };
    ///
    /// struct TestProcessor;
    ///
    /// impl CodeBlockProcessor for TestProcessor {
    ///     fn process(
    ///         &mut self,
    ///         language: &str,
    ///         _attrs: &HashMap<String, String>,
    ///         _source: &str,
    ///         index: usize,
    ///     ) -> ProcessResult {
    ///         if language == "test" {
    ///             ProcessResult::Placeholder(format!("{{{{TEST_{index}}}}}"))
    ///         } else {
    ///             ProcessResult::PassThrough
    ///         }
    ///     }
    /// }
    ///
    /// let renderer = MarkdownRenderer::<HtmlBackend>::new()
    ///     .with_processor(TestProcessor);
    /// ```
    #[must_use]
    pub fn with_processor<P: CodeBlockProcessor + 'static>(mut self, processor: P) -> Self {
        self.processors.push(Box::new(processor));
        self
    }

    /// Get all warnings from all processors.
    ///
    /// Returns an iterator over warnings from all processors.
    /// If you need a `Vec`, call `.collect()` on the result.
    pub fn processor_warnings(&self) -> impl Iterator<Item = String> + '_ {
        self.processors.iter().flat_map(|p| p.warnings()).cloned()
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
    ///
    /// Automatically calls `post_process` on all registered processors
    /// to replace placeholders with rendered content.
    pub fn render<'a, I>(&mut self, events: I) -> RenderResult
    where
        I: Iterator<Item = Event<'a>>,
    {
        for event in events {
            self.process_event(event);
        }

        let mut html = std::mem::take(&mut self.output);
        for processor in &mut self.processors {
            processor.post_process(&mut html);
        }

        RenderResult {
            html,
            title: self.heading.take_title(),
            toc: self.heading.take_toc(),
            warnings: self.processor_warnings().collect(),
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
            Tag::BlockQuote(kind) => {
                if let Some(bq_kind) = kind {
                    let alert_kind = AlertKind::from(bq_kind);
                    self.alert_stack.push(Some(alert_kind));
                    B::alert_start(alert_kind, &mut self.output);
                } else {
                    self.alert_stack.push(None);
                    B::blockquote_start(&mut self.output);
                }
            }
            Tag::CodeBlock(kind) => {
                let (lang, attrs) = match kind {
                    CodeBlockKind::Fenced(ref info) if !info.is_empty() => {
                        let (lang, attrs) = parse_fence_info(info);
                        (if lang.is_empty() { None } else { Some(lang) }, attrs)
                    }
                    _ => (None, HashMap::new()),
                };
                self.pending_attrs = attrs;
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
                self.table.start(alignments);
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
            Tag::Strikethrough => self.push_inline("<s>"),
            Tag::Link { dest_url, .. } => {
                let href = B::transform_link(&dest_url, self.base_path.as_deref());
                let href = if self.relative_links && href.starts_with('/') {
                    let from = format!("/{}", self.base_path.as_deref().unwrap_or(""));
                    relative_path(&from, &href).into()
                } else {
                    href
                };
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
            TagEnd::BlockQuote(_) => match self.alert_stack.pop() {
                Some(Some(alert_kind)) => {
                    B::alert_end(alert_kind, &mut self.output);
                }
                _ => {
                    B::blockquote_end(&mut self.output);
                }
            },
            TagEnd::CodeBlock => {
                let (lang, content) = self.code.end();
                let attrs = std::mem::take(&mut self.pending_attrs);
                let index = self.code_block_index;
                self.code_block_index += 1;

                // Try processors in order, fall back to normal code block rendering
                let processed = lang.as_ref().is_some_and(|lang_str| {
                    self.processors.iter_mut().any(|processor| {
                        match processor.process(lang_str, &attrs, &content, index) {
                            ProcessResult::Placeholder(placeholder) => {
                                self.output.push_str(&placeholder);
                                true
                            }
                            ProcessResult::Inline(html) => {
                                self.output.push_str(&html);
                                true
                            }
                            ProcessResult::PassThrough => false,
                        }
                    })
                });

                if !processed {
                    B::code_block(lang.as_deref(), &content, &mut self.output);
                }
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
            TagEnd::Strikethrough => self.push_inline("</s>"),
            TagEnd::Link => self.push_inline("</a>"),
            TagEnd::Superscript => self.push_inline("</sup>"),
            TagEnd::Subscript => self.push_inline("</sub>"),
        }
    }

    fn text(&mut self, text: &str) {
        if self.code.is_active() {
            self.code.push_str(text);
        } else if self.image.is_active() {
            self.image.push_str(text);
        } else if self.heading.is_in_first_h1() {
            self.heading.push_text(text);
        } else if self.heading.is_active() {
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
    use crate::HtmlBackend;
    use crate::code_block::ExtractedCodeBlock;
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

    fn render_with_base_path(markdown: &str, base_path: &str) -> RenderResult {
        let options = Options::ENABLE_TABLES;
        let parser = Parser::new_ext(markdown, options);
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .render(parser)
    }

    fn render_with_tasklists(markdown: &str) -> RenderResult {
        let options = Options::ENABLE_TASKLISTS;
        let parser = Parser::new_ext(markdown, options);
        MarkdownRenderer::<HtmlBackend>::new().render(parser)
    }

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

        assert_eq!(result.title, Some("My Title".to_owned()));
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
    fn test_note_alert() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render_markdown("> [!NOTE]\n> This is a **note**.");
        assert!(result.html.contains("alert-note"));
        assert!(result.html.contains("<strong>note</strong>"));
    }

    #[test]
    fn test_tip_alert() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render_markdown("> [!TIP]\n> This is a tip.");
        assert!(result.html.contains("alert-tip"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_important_alert() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render_markdown("> [!IMPORTANT]\n> Critical information.");
        assert!(result.html.contains("alert-important"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_warning_alert() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render_markdown("> [!WARNING]\n> Be careful!");
        assert!(result.html.contains("alert-warning"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_caution_alert() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render_markdown("> [!CAUTION]\n> Dangerous operation.");
        assert!(result.html.contains("alert-caution"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_alert_with_list() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result =
            renderer.render_markdown("> [!WARNING]\n> Be careful:\n> - Item 1\n> - Item 2");
        assert!(result.html.contains("alert-warning"));
        assert!(result.html.contains("<ul>"));
        assert!(result.html.contains("<li>"));
    }

    #[test]
    fn test_regular_blockquote_unchanged() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render_markdown("> Just a regular quote");
        assert!(result.html.contains("<blockquote>"));
        assert!(!result.html.contains("alert"));
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
        let result = render_with_base_path("[Link](./page.md)", "base/path");
        assert!(result.html.contains(r#"href="/base/path/page""#));
    }

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
        assert!(result.html.contains("<s>deleted</s>"));
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
        let result = render_with_tasklists("- [ ] Unchecked\n- [x] Checked");
        assert!(result.html.contains(r#"<input type="checkbox" disabled>"#));
        assert!(
            result
                .html
                .contains(r#"<input type="checkbox" checked disabled>"#)
        );
    }

    #[test]
    fn test_default_renderer() {
        let parser = Parser::new("Hello");
        let mut renderer = MarkdownRenderer::<HtmlBackend>::default();
        let result = renderer.render(parser);
        assert_eq!(result.html, "<p>Hello</p>");
    }

    // Code block processor tests

    struct PlaceholderProcessor {
        extracted: Vec<ExtractedCodeBlock>,
    }

    impl PlaceholderProcessor {
        fn new() -> Self {
            Self {
                extracted: Vec::new(),
            }
        }
    }

    impl CodeBlockProcessor for PlaceholderProcessor {
        fn process(
            &mut self,
            language: &str,
            attrs: &HashMap<String, String>,
            source: &str,
            index: usize,
        ) -> ProcessResult {
            if language == "diagram" {
                self.extracted.push(ExtractedCodeBlock {
                    index,
                    language: language.to_owned(),
                    source: source.to_owned(),
                    attrs: attrs.clone(),
                });
                ProcessResult::Placeholder(format!("{{{{DIAGRAM_{index}}}}}"))
            } else {
                ProcessResult::PassThrough
            }
        }

        fn extracted(&self) -> &[ExtractedCodeBlock] {
            &self.extracted
        }
    }

    struct InlineProcessor;

    impl CodeBlockProcessor for InlineProcessor {
        fn process(
            &mut self,
            language: &str,
            _attrs: &HashMap<String, String>,
            source: &str,
            _index: usize,
        ) -> ProcessResult {
            if language == "inline-test" {
                ProcessResult::Inline(format!("<div class=\"inline\">{source}</div>"))
            } else {
                ProcessResult::PassThrough
            }
        }
    }

    #[test]
    fn test_processor_passthrough() {
        let markdown = "```rust\nfn main() {}\n```";
        let parser = Parser::new(markdown);
        let mut renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_processor(PlaceholderProcessor::new());
        let result = renderer.render(parser);

        // Should render as normal code block
        assert!(result.html.contains(r#"class="language-rust""#));
        assert!(result.html.contains("fn main() {}"));
    }

    #[test]
    fn test_processor_placeholder() {
        let markdown = "```diagram\nA -> B\n```";
        let parser = Parser::new(markdown);
        let mut renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_processor(PlaceholderProcessor::new());
        let result = renderer.render(parser);

        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert!(!result.html.contains("<pre>"));
    }

    #[test]
    fn test_processor_inline() {
        let markdown = "```inline-test\ncontent\n```";
        let parser = Parser::new(markdown);
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_processor(InlineProcessor);
        let result = renderer.render(parser);

        assert!(result.html.contains(r#"<div class="inline">content"#));
        assert!(!result.html.contains("<pre>"));
    }

    #[test]
    fn test_processor_with_attrs() {
        let markdown = "```diagram format=png theme=dark\nA -> B\n```";
        let parser = Parser::new(markdown);
        let mut renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_processor(PlaceholderProcessor::new());
        let result = renderer.render(parser);

        assert!(result.html.contains("{{DIAGRAM_0}}"));
    }

    #[test]
    fn test_multiple_processors() {
        let markdown =
            "```diagram\nA -> B\n```\n\n```inline-test\nhello\n```\n\n```rust\nfn main() {}\n```";
        let parser = Parser::new(markdown);
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_processor(PlaceholderProcessor::new())
            .with_processor(InlineProcessor);
        let result = renderer.render(parser);

        // First processor handles diagram
        assert!(result.html.contains("{{DIAGRAM_0}}"));
        // Second processor handles inline-test
        assert!(result.html.contains(r#"<div class="inline">hello"#));
        // Neither handles rust, so normal code block
        assert!(result.html.contains(r#"class="language-rust""#));
    }

    #[test]
    fn test_processor_multiple_code_blocks() {
        let markdown = "```diagram\nA -> B\n```\n\n```diagram\nC -> D\n```";
        let parser = Parser::new(markdown);
        let mut renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_processor(PlaceholderProcessor::new());
        let result = renderer.render(parser);

        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert!(result.html.contains("{{DIAGRAM_1}}"));
    }

    #[test]
    fn test_processor_code_block_without_language() {
        let markdown = "```\nplain text\n```";
        let parser = Parser::new(markdown);
        let mut renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_processor(PlaceholderProcessor::new());
        let result = renderer.render(parser);

        // Should render as normal code block without language class
        assert!(result.html.contains("<pre><code>"));
        assert!(result.html.contains("plain text"));
    }

    struct WarningProcessor {
        warnings: Vec<String>,
    }

    impl WarningProcessor {
        fn new(warnings: Vec<String>) -> Self {
            Self { warnings }
        }
    }

    impl CodeBlockProcessor for WarningProcessor {
        fn process(
            &mut self,
            _language: &str,
            _attrs: &HashMap<String, String>,
            _source: &str,
            _index: usize,
        ) -> ProcessResult {
            ProcessResult::PassThrough
        }

        fn warnings(&self) -> &[String] {
            &self.warnings
        }
    }

    #[test]
    fn test_render_result_includes_warnings() {
        let markdown = "Hello";
        let parser = Parser::new(markdown);
        let mut renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_processor(WarningProcessor::new(vec![
                "warning 1".into(),
                "warning 2".into(),
            ]));
        let result = renderer.render(parser);

        assert_eq!(result.warnings.len(), 2);
        assert_eq!(result.warnings[0], "warning 1");
        assert_eq!(result.warnings[1], "warning 2");
    }

    #[test]
    fn test_render_result_empty_warnings_by_default() {
        let result = render_html("Hello");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_render_markdown_convenience() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render_markdown("# Hello\n\n**World**");
        assert!(result.html.contains("<h1"));
        assert!(result.html.contains("<strong>World</strong>"));
    }

    #[test]
    fn test_gfm_enabled_by_default() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render_markdown("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(result.html.contains("<table>"));
    }

    #[test]
    fn test_gfm_disabled() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_gfm(false);
        let result = renderer.render_markdown("| A | B |\n|---|---|\n| 1 | 2 |");
        // Tables not rendered when GFM disabled
        assert!(!result.html.contains("<table>"));
    }

    #[test]
    fn test_parser_options_with_gfm() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let options = renderer.parser_options();
        assert!(options.contains(Options::ENABLE_TABLES));
        assert!(options.contains(Options::ENABLE_STRIKETHROUGH));
        assert!(options.contains(Options::ENABLE_TASKLISTS));
        assert!(options.contains(Options::ENABLE_GFM));
    }

    #[test]
    fn test_parser_options_without_gfm() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new().with_gfm(false);
        let options = renderer.parser_options();
        assert!(!options.contains(Options::ENABLE_TABLES));
        assert!(!options.contains(Options::ENABLE_STRIKETHROUGH));
        assert!(!options.contains(Options::ENABLE_TASKLISTS));
        assert!(!options.contains(Options::ENABLE_GFM));
    }

    #[test]
    fn test_create_parser() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let parser = renderer.create_parser("# Hello");
        let events: Vec<_> = parser.collect();
        // Should produce heading events
        assert!(!events.is_empty());
    }

    // Directive integration tests

    #[test]
    fn test_with_directives_tabs() {
        use crate::TabsDirective;
        use crate::directive::DirectiveProcessor;

        let processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_directives(processor);

        let result = renderer.render_markdown(
            r":::tab[macOS]
Install with Homebrew.
:::tab[Linux]
Install with apt.
:::",
        );

        // Should have accessible tab structure
        assert!(result.html.contains(r#"role="tablist""#));
        assert!(result.html.contains(r#"role="tab""#));
        assert!(result.html.contains(r#"role="tabpanel""#));
        assert!(result.html.contains("macOS"));
        assert!(result.html.contains("Linux"));
    }

    #[test]
    fn test_with_directives_inline() {
        use crate::directive::{
            DirectiveArgs, DirectiveContext, DirectiveOutput, DirectiveProcessor, InlineDirective,
        };

        struct KbdDirective;

        impl InlineDirective for KbdDirective {
            fn name(&self) -> &'static str {
                "kbd"
            }

            fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
                DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content))
            }
        }

        let processor = DirectiveProcessor::new().with_inline(KbdDirective);

        let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_directives(processor);

        let result = renderer.render_markdown("Press :kbd[Ctrl+C] to copy.");

        assert!(result.html.contains("<kbd>Ctrl+C</kbd>"));
    }

    #[test]
    fn test_directives_warnings_included() {
        use crate::TabsDirective;
        use crate::directive::DirectiveProcessor;

        let processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_directives(processor);

        // Unclosed tabs should produce warning
        let result = renderer.render_markdown(":::tab[Test]\nContent");

        assert!(result.warnings.iter().any(|w| w.contains("unclosed")));
    }

    #[test]
    fn test_relative_links_converts_absolute_to_relative() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("a/b")
            .with_relative_links(true);
        // ../other.md resolves to /a/other, then relative_path("/a/b", "/a/other") = "other"
        let result = renderer.render_markdown("[link](../other.md)");
        assert!(result.html.contains(r#"href="other""#));
    }

    #[test]
    fn test_relative_links_disabled_keeps_absolute() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_base_path("a/b");
        let result = renderer.render_markdown("[link](../other.md)");
        assert!(result.html.contains(r#"href="/a/other""#));
    }

    #[test]
    fn test_relative_links_external_unchanged() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("a/b")
            .with_relative_links(true);
        let result = renderer.render_markdown("[link](https://example.com)");
        assert!(result.html.contains(r#"href="https://example.com""#));
    }

    #[test]
    fn test_relative_links_fragment_unchanged() {
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("a/b")
            .with_relative_links(true);
        let result = renderer.render_markdown("[link](#section)");
        assert!(result.html.contains(r##"href="#section""##));
    }
}
