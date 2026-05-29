//! Generic markdown renderer with pluggable backend.
//!
//! See the [crate-level documentation](crate) for an overview and examples.

use std::marker::PhantomData;
use std::sync::Arc;

use rw_sections::Sections;

use crate::backend::RenderBackend;
use crate::config::{RenderConfig, TitleResolver};
use crate::pipeline::Pipeline;
use crate::toc::TocEntry;

/// Output produced by [`MarkdownRenderer::render`].
///
/// Contains the rendered markup, an optional page title extracted from the
/// first H1 heading, table-of-contents entries for heading navigation, and
/// any warnings emitted by code block processors or directives.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .render("# Welcome\n\nHello **world**.", Pipeline::new());
///
/// assert_eq!(result.title.as_deref(), Some("Welcome"));
/// assert!(result.html.contains("<strong>world</strong>"));
/// assert!(result.warnings.is_empty());
/// ```
#[derive(Debug)]
pub struct RenderResult {
    /// Rendered markup produced by the [`RenderBackend`].
    ///
    /// Named `html` because [`HtmlBackend`](crate::HtmlBackend) is the primary
    /// backend, but the actual format depends on `B`: [`HtmlBackend`](crate::HtmlBackend)
    /// produces HTML5, while the downstream Confluence backend produces XHTML.
    pub html: String,
    /// Title extracted from the first H1 heading when
    /// [`with_title_extraction`](MarkdownRenderer::with_title_extraction) is enabled.
    pub title: Option<String>,
    /// Table-of-contents entries, one per heading (excluding the title heading).
    pub toc: Vec<TocEntry>,
    /// Warnings generated during conversion (e.g., unresolved includes,
    /// unclosed container directives).
    pub warnings: Vec<String>,
}

/// Generic markdown renderer with pluggable backend.
///
/// Walks pulldown-cmark events and produces HTML or XHTML depending on the
/// [`RenderBackend`] implementation (`B`). Common elements (tables, lists,
/// inline formatting) are handled generically; format-specific elements are
/// delegated to `B`.
///
/// The entry point is [`render`](Self::render): it accepts raw markdown and a
/// [`Pipeline`], and runs the full pipeline — block-directive preprocessing,
/// parse + event walk (with inline-directive expansion), and directive
/// post-processing.
///
/// # Code block processors and directives
///
/// Per-render extensions (code block processors, directive processor) are
/// bundled in a [`Pipeline`] passed to [`render`](Self::render). Build a fresh
/// [`Pipeline`] for each render call.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
///
/// let renderer = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .with_base_path("/docs/guide");
///
/// let result = renderer.render("# Guide\n\nSee [setup](setup.md).", Pipeline::new());
/// assert_eq!(result.title.as_deref(), Some("Guide"));
/// assert!(result.html.contains(r#"href="/docs/guide/setup""#));
/// ```
pub struct MarkdownRenderer<B: RenderBackend> {
    config: RenderConfig,
    _backend: PhantomData<B>,
}

impl<B: RenderBackend> MarkdownRenderer<B> {
    /// Create a new renderer with GFM enabled by default.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: RenderConfig::new(),
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
        self.config.extract_title = true;
        self
    }

    /// Set base path for resolving relative links (URL path with leading `/`).
    ///
    /// Only used by HTML backend. Confluence backend ignores this.
    #[must_use]
    pub fn with_base_path(mut self, path: impl Into<String>) -> Self {
        self.config.base_path = Some(path.into());
        self
    }

    /// Set the origin (source directory name) for files outside `source_dir`.
    ///
    /// When set, relative links starting with this prefix (e.g., `docs/guide.md`)
    /// have the prefix stripped before resolution, so the link resolves correctly
    /// within URL space where `source_dir` is the root.
    #[must_use]
    pub fn with_origin(mut self, origin: impl Into<String>) -> Self {
        let mut prefix = origin.into();
        prefix.push('/');
        self.config.origin_prefix = Some(prefix);
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
        self.config.gfm = enabled;
        self
    }

    /// Set the section registry for wikilink resolution and link annotation.
    ///
    /// [`Sections`] maps section refs (e.g., `"domain:default/billing"`) to
    /// filesystem paths, allowing the renderer to resolve `[[domain:billing::overview]]`
    /// to a concrete URL. When set, resolved internal links also get
    /// `data-section-ref` and `data-section-path` attributes on the anchor
    /// element so host applications can build cross-entity navigation.
    ///
    /// Without this, wikilinks cannot resolve to URLs and render as broken
    /// links (`class="rw-broken-link"`). See the
    /// [crate-level wikilink documentation](crate#wikilinks) for the full
    /// degradation behavior.
    #[must_use]
    pub fn with_sections(mut self, sections: Arc<Sections>) -> Self {
        if sections.is_empty() {
            self.config.sections = None;
        } else {
            self.config.sections = Some(sections);
        }
        self
    }

    /// Enable `[[wikilink]]` syntax for section-stable internal links.
    ///
    /// When enabled, the pulldown-cmark parser recognizes `[[target]]` and
    /// `[[target|display text]]` syntax. Links are resolved through
    /// [`Sections`] (see [`with_sections`](Self::with_sections)) and display
    /// text is looked up via [`with_title_resolver`](Self::with_title_resolver).
    /// Each piece degrades gracefully when omitted — see the
    /// [crate-level wikilink documentation](crate#wikilinks) for details.
    /// Without [`Sections`], all wikilinks render as broken links.
    /// Without a [`TitleResolver`], display text falls back to the last path
    /// segment. Without this method, `[[...]]` is not parsed at all.
    #[must_use]
    pub fn with_wikilinks(mut self, enabled: bool) -> Self {
        self.config.wikilinks = enabled;
        self
    }

    /// Set a title resolver for wikilink display text.
    ///
    /// When a wikilink has no explicit display text (`[[target]]` vs.
    /// `[[target|text]]`), the renderer calls the resolver to look up a
    /// human-readable page title. If the resolver returns `None`, the
    /// renderer falls back to the last path segment of the resolved URL.
    ///
    /// Optional — without this, display text falls back to the last path
    /// segment (e.g., `[[domain:billing::overview]]` displays as "overview")
    /// or the section name for root links.
    #[must_use]
    pub fn with_title_resolver(mut self, resolver: impl TitleResolver + 'static) -> Self {
        self.config.title_resolver = Some(Box::new(resolver));
        self
    }

    /// Renders raw markdown to the configured backend, applying the supplied
    /// [`Pipeline`]'s extensions.
    ///
    /// This is the entry point. It runs the full pipeline:
    ///
    /// 1. **Preprocess** — expands block-level directives via
    ///    `pipeline.directives` (if `Some`).
    /// 2. **Parse & render** — feeds the (preprocessed) markdown through
    ///    pulldown-cmark and the backend; inline directives expand during
    ///    the event walk.
    /// 3. **Post-process** — transforms intermediate directive elements and
    ///    replaces code-block placeholders.
    ///
    /// The supplied `Pipeline` is consumed: build a fresh one per render.
    pub fn render(&self, markdown: &str, mut pipeline: Pipeline) -> RenderResult {
        // Phase 1: block-level directive preprocessing.
        let preprocessed = if let Some(processor) = pipeline.directives.as_mut() {
            processor.process(markdown)
        } else {
            markdown.to_owned()
        };

        // Phase 2: parse and walk. Inline-directive expansion happens inside
        // the walker (see `Walker::flush_text`), so there's no second parse.
        let parser = self.config.create_parser(&preprocessed);
        let mut result = {
            let mut walker = crate::walker::Walker::<B>::new(
                &self.config,
                &mut pipeline.processors,
                pipeline.directives.as_mut(),
            );
            for event in parser {
                walker.process_event(event);
            }
            walker.flush_text_buffer();
            walker.finish()
        };

        // Phase 3: post-process directive markers if a directive processor
        // is configured. Code-block-processor warnings already landed on
        // `result.warnings` inside `walker.finish()`.
        if let Some(processor) = pipeline.directives.as_mut() {
            processor.post_process(&mut result.html);
            result.warnings.extend(processor.warnings());
        }

        result
    }
}

impl<B: RenderBackend> Default for MarkdownRenderer<B> {
    fn default() -> Self {
        Self::new()
    }
}

// Compile-time contract: this fires in every build (not only `cargo test`),
// so a future change that breaks the auto-trait shape — e.g., adding an `Rc`
// to `RenderConfig` or making a directive handler `!Send` — fails the build
// instead of slipping past test-gated assertions.
//
// `MarkdownRenderer<B>` must stay `Send + Sync` so it can be parked in an
// `Arc` and used by many request handlers. `Pipeline` must stay `Send` so a
// caller can build it on one thread and hand it to a render running on
// another; it is intentionally not `Sync` because directive handlers are
// `Send`-only (each document gets its own handler).
const _: fn() = || {
    fn assert_send<T: Send>() {}
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MarkdownRenderer<crate::HtmlBackend>>();
    assert_send::<crate::Pipeline>();
};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::HtmlBackend;
    use crate::code_block::{CodeBlockProcessor, ExtractedCodeBlock, ProcessResult};
    use rw_sections::{Namespace, Section};

    fn render_html(markdown: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new().render(markdown, Pipeline::new())
    }

    fn render_html_with_title(markdown: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_title_extraction()
            .render(markdown, Pipeline::new())
    }

    fn render_with_base_path(markdown: &str, base_path: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .render(markdown, Pipeline::new())
    }

    fn render_with_origin(markdown: &str, base_path: &str, origin: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .with_origin(origin)
            .render(markdown, Pipeline::new())
    }

    fn render_with_tasklists(markdown: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new().render(markdown, Pipeline::new())
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
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!NOTE]\n> This is a **note**.", Pipeline::new());
        assert!(result.html.contains("alert-note"));
        assert!(result.html.contains("<strong>note</strong>"));
    }

    #[test]
    fn test_tip_alert() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!TIP]\n> This is a tip.", Pipeline::new());
        assert!(result.html.contains("alert-tip"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_important_alert() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!IMPORTANT]\n> Critical information.", Pipeline::new());
        assert!(result.html.contains("alert-important"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_warning_alert() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!WARNING]\n> Be careful!", Pipeline::new());
        assert!(result.html.contains("alert-warning"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_caution_alert() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!CAUTION]\n> Dangerous operation.", Pipeline::new());
        assert!(result.html.contains("alert-caution"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_alert_with_list() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "> [!WARNING]\n> Be careful:\n> - Item 1\n> - Item 2",
            Pipeline::new(),
        );
        assert!(result.html.contains("alert-warning"));
        assert!(result.html.contains("<ul>"));
        assert!(result.html.contains("<li>"));
    }

    #[test]
    fn test_regular_blockquote_unchanged() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> Just a regular quote", Pipeline::new());
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
    fn test_image_alt_with_bold_no_stray_markup() {
        // `**bold alt**` inside alt text must not leak `<strong></strong>`
        // into surrounding HTML, and the alt attribute must still carry
        // the formatted text.
        let result = render_html("![**bold alt**](pic.png)");
        assert_eq!(result.html, r#"<p><img src="pic.png" alt="bold alt"></p>"#,);
    }

    #[test]
    fn test_image_alt_with_emphasis_no_stray_markup() {
        let result = render_html("![*emphasized*](pic.png)");
        assert_eq!(
            result.html,
            r#"<p><img src="pic.png" alt="emphasized"></p>"#,
        );
    }

    #[test]
    fn test_image_alt_with_strikethrough_no_stray_markup() {
        let result = render_html("![~~struck~~](pic.png)");
        assert_eq!(result.html, r#"<p><img src="pic.png" alt="struck"></p>"#,);
    }

    #[test]
    fn test_image_alt_with_inline_code_preserves_text() {
        // Inline code inside alt text must contribute its content to the
        // alt attribute and must not leak a `<code>` element outside `<img>`.
        let result = render_html("![alt with `code` text](pic.png)");
        assert_eq!(
            result.html,
            r#"<p><img src="pic.png" alt="alt with code text"></p>"#,
        );
    }

    #[test]
    fn test_image_alt_with_raw_html_drops_tags() {
        // Raw HTML inside alt text contributes its visible text but the
        // tags themselves do not leak outside the `<img>`.
        let result = render_html("![pre <span>html</span> post](pic.png)");
        assert_eq!(
            result.html,
            r#"<p><img src="pic.png" alt="pre html post"></p>"#,
        );
    }

    #[test]
    fn test_image_alt_with_link_no_stray_markup() {
        let result = render_html("![text [link](https://example.com) more](pic.png)");
        assert_eq!(
            result.html,
            r#"<p><img src="pic.png" alt="text link more"></p>"#,
        );
    }

    #[test]
    fn test_image_inside_heading_stays_inside() {
        // An image inside a heading must land inside the `<h*>` element,
        // not before it.
        let result = render_html("# Heading with ![icon](icon.png) in it");
        assert_eq!(
            result.html,
            r#"<h1 id="heading-with-in-it">Heading with <img src="icon.png" alt="icon"> in it</h1>"#,
        );
    }

    #[test]
    fn test_image_inside_heading_with_formatted_alt() {
        let result = render_html("## See ![**Logo**](logo.png) here");
        assert_eq!(
            result.html,
            r#"<h2 id="see-here">See <img src="logo.png" alt="Logo"> here</h2>"#,
        );
    }

    #[test]
    fn test_image_alt_with_html_entity_preserves_decoded_character() {
        // `&amp;`, `&#8211;`, etc. are decoded by pulldown-cmark into `Text`
        // events before reaching `raw_html`, so the resulting glyphs survive
        // into the alt attribute (and get re-escaped by the backend).
        let result = render_html("![alt &amp; more](pic.png)");
        assert_eq!(
            result.html,
            r#"<p><img src="pic.png" alt="alt &amp; more"></p>"#,
        );
    }

    #[test]
    fn test_image_alt_with_soft_break_collapses_to_space() {
        // A soft break inside alt text becomes a single space, not a newline
        // or `<br>` — matches CommonMark's plain-text projection rule.
        let result = render_html("![alt\ntext](pic.png)");
        assert_eq!(result.html, r#"<p><img src="pic.png" alt="alt text"></p>"#,);
    }

    #[test]
    fn test_image_alt_with_hard_break_collapses_to_space() {
        // A hard break (`\\\n` or two trailing spaces + newline) inside alt
        // text collapses to a single space — and no `<br>` leaks outside the
        // `<img>`.
        let result = render_html("![alt\\\ntext](pic.png)");
        assert_eq!(result.html, r#"<p><img src="pic.png" alt="alt text"></p>"#,);
    }

    #[test]
    fn test_image_inside_link_is_unaffected() {
        // Regression: image-in-link continues to render correctly.
        let result = render_html("[![alt](pic.png)](https://example.com)");
        assert!(
            result
                .html
                .contains(r#"<a href="https://example.com"><img src="pic.png" alt="alt"></a>"#),
            "got: {}",
            result.html,
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
        let result = render_with_base_path("[Link](./page.md)", "/base/path");
        assert!(result.html.contains(r#"href="/base/path/page""#));
    }

    #[test]
    fn test_origin_strips_source_dir_from_links() {
        let result = render_with_origin("[Guide](docs/guide.md)", "/", "docs");
        assert!(
            result.html.contains(r#"href="/guide""#),
            "Expected href=\"/guide\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_origin_strips_source_dir_from_nested_links() {
        let result = render_with_origin("[Config](docs/sub/config.md)", "/", "docs");
        assert!(
            result.html.contains(r#"href="/sub/config""#),
            "Expected href=\"/sub/config\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_origin_preserves_links_without_prefix() {
        let result = render_with_origin("[Other](other/page.md)", "/", "docs");
        assert!(
            result.html.contains(r#"href="/other/page""#),
            "Expected href=\"/other/page\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_origin_preserves_external_links() {
        let result = render_with_origin("[Ext](https://example.com)", "/", "docs");
        assert!(result.html.contains(r#"href="https://example.com""#));
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
        let renderer = MarkdownRenderer::<HtmlBackend>::default();
        let result = renderer.render("Hello", Pipeline::new());
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
                self.extracted.push(ExtractedCodeBlock::new(
                    index,
                    language.to_owned(),
                    source.to_owned(),
                    attrs.clone(),
                ));
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
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            markdown,
            Pipeline::new().with_processor(PlaceholderProcessor::new()),
        );

        // Should render as normal code block
        assert!(result.html.contains(r#"class="language-rust""#));
        assert!(result.html.contains("fn main() {}"));
    }

    #[test]
    fn test_processor_placeholder() {
        let markdown = "```diagram\nA -> B\n```";
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            markdown,
            Pipeline::new().with_processor(PlaceholderProcessor::new()),
        );

        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert!(!result.html.contains("<pre>"));
    }

    #[test]
    fn test_processor_inline() {
        let markdown = "```inline-test\ncontent\n```";
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .render(markdown, Pipeline::new().with_processor(InlineProcessor));

        assert!(result.html.contains(r#"<div class="inline">content"#));
        assert!(!result.html.contains("<pre>"));
    }

    #[test]
    fn test_processor_with_attrs() {
        let markdown = "```diagram format=png theme=dark\nA -> B\n```";
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            markdown,
            Pipeline::new().with_processor(PlaceholderProcessor::new()),
        );

        assert!(result.html.contains("{{DIAGRAM_0}}"));
    }

    #[test]
    fn test_multiple_processors() {
        let markdown =
            "```diagram\nA -> B\n```\n\n```inline-test\nhello\n```\n\n```rust\nfn main() {}\n```";
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            markdown,
            Pipeline::new()
                .with_processor(PlaceholderProcessor::new())
                .with_processor(InlineProcessor),
        );

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
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            markdown,
            Pipeline::new().with_processor(PlaceholderProcessor::new()),
        );

        assert!(result.html.contains("{{DIAGRAM_0}}"));
        assert!(result.html.contains("{{DIAGRAM_1}}"));
    }

    #[test]
    fn test_processor_code_block_without_language() {
        let markdown = "```\nplain text\n```";
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            markdown,
            Pipeline::new().with_processor(PlaceholderProcessor::new()),
        );

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
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            markdown,
            Pipeline::new().with_processor(WarningProcessor::new(vec![
                "warning 1".into(),
                "warning 2".into(),
            ])),
        );

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
    fn test_render_convenience() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("# Hello\n\n**World**", Pipeline::new());
        assert!(result.html.contains("<h1"));
        assert!(result.html.contains("<strong>World</strong>"));
    }

    #[test]
    fn test_gfm_enabled_by_default() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("| A | B |\n|---|---|\n| 1 | 2 |", Pipeline::new());
        assert!(result.html.contains("<table>"));
    }

    #[test]
    fn test_gfm_disabled() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new().with_gfm(false);
        let result = renderer.render("| A | B |\n|---|---|\n| 1 | 2 |", Pipeline::new());
        // Tables not rendered when GFM disabled
        assert!(!result.html.contains("<table>"));
    }

    // Directive integration tests

    #[test]
    fn test_with_directives_tabs() {
        use crate::TabsDirective;
        use crate::directive::DirectiveProcessor;

        let processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let result = renderer.render(
            r":::tab[macOS]
Install with Homebrew.
:::tab[Linux]
Install with apt.
:::",
            Pipeline::new().with_directives(processor),
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
                DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
            }
        }

        let processor = DirectiveProcessor::new().with_inline(KbdDirective);

        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let result = renderer.render(
            "Press :kbd[Ctrl+C] to copy.",
            Pipeline::new().with_directives(processor),
        );

        assert!(result.html.contains("<kbd>Ctrl+C</kbd>"));
    }

    #[test]
    fn test_inline_directive_after_punctuation_colon_still_expands() {
        // Issue #390: a punctuation colon earlier on the line (`Note:`) used to
        // blind the scanner to the real :kbd directive that followed.
        use crate::directive::{
            DirectiveArgs, DirectiveContext, DirectiveOutput, DirectiveProcessor, InlineDirective,
        };

        struct KbdDirective;
        impl InlineDirective for KbdDirective {
            fn name(&self) -> &'static str {
                "kbd"
            }
            fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
                DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
            }
        }

        let processor = DirectiveProcessor::new().with_inline(KbdDirective);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let result = renderer.render(
            "Note: press :kbd[Ctrl+C] to copy.",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result.html.contains("<kbd>Ctrl+C</kbd>"),
            "expected :kbd to expand after a non-directive colon; got: {}",
            result.html,
        );
    }

    #[test]
    fn test_inline_directive_inside_code_span_not_expanded() {
        use crate::directive::DirectiveProcessor;

        struct KbdDirective;
        impl crate::directive::InlineDirective for KbdDirective {
            fn name(&self) -> &'static str {
                "kbd"
            }
            fn process(
                &mut self,
                args: crate::directive::DirectiveArgs,
                _ctx: &crate::directive::DirectiveContext,
            ) -> crate::directive::DirectiveOutput {
                crate::directive::DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
            }
        }

        let processor = DirectiveProcessor::new().with_inline(KbdDirective);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let result = renderer.render(
            "Use `:kbd[Ctrl+C]` to copy.",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result.html.contains("<code>:kbd[Ctrl+C]</code>"),
            "expected literal directive syntax inside <code>; got: {}",
            result.html,
        );
        assert!(
            !result.html.contains("<kbd>"),
            "directive should NOT have been expanded inside the code span; got: {}",
            result.html,
        );
    }

    #[test]
    fn test_inline_directive_outside_code_span_still_expands() {
        use crate::directive::DirectiveProcessor;

        struct KbdDirective;
        impl crate::directive::InlineDirective for KbdDirective {
            fn name(&self) -> &'static str {
                "kbd"
            }
            fn process(
                &mut self,
                args: crate::directive::DirectiveArgs,
                _ctx: &crate::directive::DirectiveContext,
            ) -> crate::directive::DirectiveOutput {
                crate::directive::DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
            }
        }

        let processor = DirectiveProcessor::new().with_inline(KbdDirective);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let result = renderer.render(
            "Use :kbd[Ctrl+C] not `:kbd[Esc]`.",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result.html.contains("<kbd>Ctrl+C</kbd>"),
            "directive outside code span should expand; got: {}",
            result.html,
        );
        assert!(
            result.html.contains("<code>:kbd[Esc]</code>"),
            "directive inside code span should stay literal; got: {}",
            result.html,
        );
    }

    #[test]
    fn test_inline_directive_in_indented_code_block_not_expanded() {
        use crate::directive::DirectiveProcessor;

        struct KbdDirective;
        impl crate::directive::InlineDirective for KbdDirective {
            fn name(&self) -> &'static str {
                "kbd"
            }
            fn process(
                &mut self,
                args: crate::directive::DirectiveArgs,
                _ctx: &crate::directive::DirectiveContext,
            ) -> crate::directive::DirectiveOutput {
                crate::directive::DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
            }
        }

        let processor = DirectiveProcessor::new().with_inline(KbdDirective);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let result = renderer.render(
            "paragraph\n\n    :kbd[X]\n",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            !result.html.contains("<kbd>"),
            "directive inside indented code block should not expand; got: {}",
            result.html,
        );
    }

    #[test]
    fn test_plain_code_span_unaffected() {
        use crate::directive::DirectiveProcessor;

        struct KbdDirective;
        impl crate::directive::InlineDirective for KbdDirective {
            fn name(&self) -> &'static str {
                "kbd"
            }
            fn process(
                &mut self,
                args: crate::directive::DirectiveArgs,
                _ctx: &crate::directive::DirectiveContext,
            ) -> crate::directive::DirectiveOutput {
                crate::directive::DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
            }
        }

        let processor = DirectiveProcessor::new().with_inline(KbdDirective);
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let result = renderer.render(
            "Plain `code` no directive.",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result.html.contains("<code>code</code>"),
            "plain code span should render normally; got: {}",
            result.html,
        );
    }

    #[test]
    fn test_with_directives_status() {
        use crate::StatusDirective;
        use crate::directive::DirectiveProcessor;

        let processor = DirectiveProcessor::new().with_inline(StatusDirective::new());
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let result = renderer.render(
            "Billing is :status[On Track]{color=green} this quarter.",
            Pipeline::new().with_directives(processor),
        );

        assert!(
            result
                .html
                .contains(r#"<span class="status status-green">On Track</span>"#),
            "got: {}",
            result.html
        );
    }

    #[test]
    fn test_directives_warnings_included() {
        use crate::TabsDirective;
        use crate::directive::DirectiveProcessor;

        let processor = DirectiveProcessor::new().with_container(TabsDirective::new());

        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        // Unclosed tabs should produce warning
        let result = renderer.render(
            ":::tab[Test]\nContent",
            Pipeline::new().with_directives(processor),
        );

        assert!(result.warnings.iter().any(|w| w.contains("unclosed")));
    }

    #[test]
    fn test_frontmatter_terminator_does_not_swallow_body() {
        // Frontmatter must terminate at `---` and not bleed into body
        // parsing. The stronger contract — that no directive handler runs
        // on frontmatter content — is covered by
        // test_frontmatter_does_not_invoke_registered_directives.
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "---\ntitle: hello\n---\n\n# Body\n\nParagraph.\n",
            Pipeline::new().with_directives(crate::directive::DirectiveProcessor::new()),
        );
        assert!(result.html.contains("<h1"), "body heading should render");
        assert!(
            result.html.contains("Body"),
            "body heading text should render"
        );
        assert!(
            result.html.contains("Paragraph"),
            "body paragraph should render"
        );
        assert!(
            !result.html.contains("title:"),
            "frontmatter keys should not appear in body"
        );
    }

    #[test]
    fn test_frontmatter_does_not_invoke_registered_directives() {
        // Frontmatter text must not invoke registered directive handlers —
        // they may have side effects (warnings, post-process replacements,
        // I/O). The metadata short-circuit lives in process_event's Event::Text
        // arm; flush_text would otherwise dispatch to handlers regardless of
        // active scope.
        use std::sync::{Arc, Mutex};

        use crate::directive::{
            DirectiveArgs, DirectiveContext, DirectiveOutput, DirectiveProcessor, InlineDirective,
        };

        struct CountingDirective {
            calls: Arc<Mutex<usize>>,
        }

        impl InlineDirective for CountingDirective {
            fn name(&self) -> &str {
                "track"
            }
            fn process(
                &mut self,
                _args: DirectiveArgs,
                _ctx: &DirectiveContext,
            ) -> DirectiveOutput {
                *self.calls.lock().unwrap() += 1;
                DirectiveOutput::Html(String::new())
            }
        }

        let calls = Arc::new(Mutex::new(0));
        let processor = DirectiveProcessor::new().with_inline(CountingDirective {
            calls: Arc::clone(&calls),
        });
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let _ = renderer.render(
            "---\ntitle: hit me :track[here]\n---\n\n# Body :track[here]\n",
            Pipeline::new().with_directives(processor),
        );

        // Exactly one invocation expected — from the body heading, not from frontmatter.
        assert_eq!(
            *calls.lock().unwrap(),
            1,
            "directive handler should be invoked once (from body), not from frontmatter"
        );
    }

    #[test]
    fn test_wikilink_in_heading_contributes_to_toc_and_slug() {
        // Wikilink display text inside a heading must contribute to both the
        // rendered HTML and the plain-text shadow used for the TOC entry
        // title and the slug id. Otherwise `## See [[overview]]` produces a
        // visible "See Overview" heading but a TOC entry "See" and an anchor
        // id of "see".
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_wikilinks(true)
            .with_sections(wikilink_sections())
            .with_title_resolver(StaticTitleResolver);
        let result = renderer.render("## See [[domain:billing::overview]]\n", Pipeline::new());

        assert_eq!(result.toc.len(), 1);
        assert_eq!(result.toc[0].title, "See Overview");
        assert_eq!(result.toc[0].id, "see-overview");
        assert!(
            result.html.contains(r#"<h2 id="see-overview">"#),
            "expected heading with id=see-overview, got: {}",
            result.html
        );
    }

    // section_ref integration tests

    #[test]
    fn section_ref_emits_data_attributes_on_cross_section_link() {
        let sections = Arc::new(Sections::new(HashMap::from([
            (
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    namespace: Namespace::default(),
                    name: "billing".to_owned(),
                },
            ),
            (
                "domains/billing/systems/pay".to_owned(),
                Section {
                    kind: "system".to_owned(),
                    namespace: Namespace::default(),
                    name: "pay".to_owned(),
                },
            ),
        ])));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/billing/systems/pay/api".to_owned())
            .with_sections(Arc::clone(&sections));
        let result = renderer.render("[Billing](../../../overview.md)", Pipeline::new());
        // Link resolves to /domains/billing/overview, which is in domain:default/billing (different section)
        assert!(
            result
                .html
                .contains(r#"data-section-ref="domain:default/billing""#)
        );
        assert!(result.html.contains(r#"data-section-path="overview""#));
        // href should still be the original resolved path
        assert!(result.html.contains(r#"href="/domains/billing/overview""#));
    }

    #[test]
    fn section_ref_annotates_same_section_link() {
        let sections = Arc::new(Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )])));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/billing/overview".to_owned())
            .with_sections(Arc::clone(&sections));
        let result = renderer.render("[Use Cases](./use-cases.md)", Pipeline::new());
        // Link resolves within same section — data attributes ARE present
        assert!(
            result
                .html
                .contains(r#"data-section-ref="domain:default/billing""#)
        );
        assert!(
            result
                .html
                .contains(r#"data-section-path="overview/use-cases""#)
        );
    }

    #[test]
    fn section_ref_no_attributes_on_external_link() {
        let sections = Arc::new(Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )])));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/billing".to_owned())
            .with_sections(sections);
        let result = renderer.render("[Google](https://google.com)", Pipeline::new());
        assert!(!result.html.contains("data-section-ref"));
        assert!(result.html.contains(r#"href="https://google.com""#));
    }

    #[test]
    fn section_ref_preserves_fragment() {
        let sections = Arc::new(Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )])));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/search/overview".to_owned())
            .with_sections(Arc::clone(&sections));
        let result = renderer.render(
            "[Billing API](../../billing/api.md#endpoints)",
            Pipeline::new(),
        );
        assert!(
            result
                .html
                .contains(r#"href="/domains/billing/api#endpoints""#)
        );
        assert!(
            result
                .html
                .contains(r#"data-section-ref="domain:default/billing""#)
        );
        assert!(result.html.contains(r#"data-section-path="api""#));
    }

    #[test]
    fn section_ref_empty_section_path_omits_attribute() {
        let sections = Arc::new(Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )])));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/search".to_owned())
            .with_sections(Arc::clone(&sections));
        let result = renderer.render("[Billing](../billing/index.md)", Pipeline::new());
        // Link resolves to /domains/billing (exact section root)
        assert!(
            result
                .html
                .contains(r#"data-section-ref="domain:default/billing""#)
        );
        // No data-section-path when targeting the section root
        assert!(!result.html.contains("data-section-path"));
    }

    #[test]
    fn section_ref_no_attributes_without_sections_configured() {
        let renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_base_path("/domains/billing".to_owned());
        let result = renderer.render("[Use Cases](./use-cases.md)", Pipeline::new());
        // No sections configured — no data attributes
        assert!(!result.html.contains("data-section-ref"));
        assert!(result.html.contains(r#"href="/domains/billing/use-cases""#));
    }

    // Wikilink tests

    struct StaticTitleResolver;

    impl TitleResolver for StaticTitleResolver {
        fn resolve_title(&self, path: &str) -> Option<String> {
            match path {
                "domains/billing" => Some("Billing Domain".to_owned()),
                "domains/billing/overview" => Some("Overview".to_owned()),
                "domains/billing/api/auth" => Some("Authentication API".to_owned()),
                _ => None,
            }
        }
    }

    fn wikilink_sections() -> Arc<Sections> {
        use rw_sections::{Namespace, Section};
        Arc::new(Sections::new(HashMap::from([
            (
                String::new(),
                Section {
                    kind: "section".to_owned(),
                    namespace: Namespace::default(),
                    name: "root".to_owned(),
                },
            ),
            (
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    namespace: Namespace::default(),
                    name: "billing".to_owned(),
                },
            ),
        ])))
    }

    fn render_wikilink(markdown: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_wikilinks(true)
            .with_sections(wikilink_sections())
            .with_title_resolver(StaticTitleResolver)
            .render(markdown, Pipeline::new())
    }

    fn render_wikilink_with_base(markdown: &str, base: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_wikilinks(true)
            .with_sections(wikilink_sections())
            .with_base_path(base)
            .with_title_resolver(StaticTitleResolver)
            .render(markdown, Pipeline::new())
    }

    #[test]
    fn wikilink_resolved_with_section_ref() {
        let result = render_wikilink("[[domain:billing::overview]]");
        assert!(
            result
                .html
                .contains(r#"<a href="/domains/billing/overview""#),
            "html: {}",
            result.html
        );
        assert!(
            result
                .html
                .contains(r#"data-section-ref="domain:default/billing""#),
            "html: {}",
            result.html
        );
        assert!(
            result.html.contains(r#"data-section-path="overview""#),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_display_text_from_title_resolver() {
        let result = render_wikilink("[[domain:billing::overview]]");
        assert!(
            result.html.contains(">Overview</a>"),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_explicit_display_text() {
        let result = render_wikilink("[[domain:billing::overview|Check this out]]");
        assert!(
            result.html.contains(">Check this out</a>"),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_section_root() {
        let result = render_wikilink("[[domain:billing]]");
        assert!(
            result.html.contains(r#"<a href="/domains/billing""#),
            "html: {}",
            result.html
        );
        assert!(
            result.html.contains(">Billing Domain</a>"),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_section_root_no_section_path_attr() {
        let result = render_wikilink("[[domain:billing]]");
        assert!(
            !result.html.contains("data-section-path"),
            "section root should not have data-section-path: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_with_fragment() {
        let result = render_wikilink("[[domain:billing::overview#pricing]]");
        assert!(
            result
                .html
                .contains(r#"href="/domains/billing/overview#pricing""#),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_fragment_only() {
        let result = render_wikilink("[[#heading]]");
        assert!(
            result.html.contains(r##"href="#heading""##),
            "html: {}",
            result.html
        );
        assert!(
            result.html.contains(">heading</a>"),
            "fragment display text should strip # prefix: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_fragment_only_with_hyphens() {
        let result = render_wikilink("[[#some-long-heading]]");
        assert!(
            result.html.contains(">some long heading</a>"),
            "fragment display text should convert hyphens to spaces: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_current_section() {
        let result = render_wikilink_with_base("[[::overview]]", "/domains/billing");
        assert!(
            result.html.contains(r#"href="/domains/billing/overview""#),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_current_section_root() {
        let result = render_wikilink_with_base("[[::]]", "/domains/billing");
        assert!(
            result.html.contains(r#"href="/domains/billing""#),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_broken_link() {
        let result = render_wikilink("[[nonexistent:unknown::page]]");
        assert!(
            result.html.contains(r#"class="rw-broken-link""#),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_broken_link_display_text() {
        let result = render_wikilink("[[nonexistent:unknown::page]]");
        assert!(
            result.html.contains(">nonexistent:unknown::page</a>"),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_name_only_defaults_to_section_kind() {
        let result = render_wikilink("[[root]]");
        assert!(
            result
                .html
                .contains(r#"data-section-ref="section:default/root""#),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_title_fallback_to_subpath() {
        let result = render_wikilink("[[domain:billing::unknown-page]]");
        assert!(
            result.html.contains(">unknown-page</a>"),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_title_fallback_deep_subpath() {
        let result = render_wikilink("[[domain:billing::api/auth]]");
        assert!(
            result.html.contains(">Authentication API</a>"),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn frontmatter_does_not_appear_in_rendered_output() {
        let markdown = "---\ntitle: My Page\nauthor: Alice\n---\n\n# Hello\n\nSome content.";
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(markdown, Pipeline::new());
        // Frontmatter should not appear as an <hr> or paragraph
        assert!(
            !result.html.contains("<hr"),
            "frontmatter rendered as <hr>: {}",
            result.html
        );
        assert!(
            !result.html.contains("title: My Page"),
            "frontmatter content leaked into output: {}",
            result.html
        );
        assert!(
            !result.html.contains("author: Alice"),
            "frontmatter content leaked into output: {}",
            result.html
        );
        // The actual page content should still render
        assert!(
            result.html.contains("<h1"),
            "h1 heading missing: {}",
            result.html
        );
        assert!(
            result.html.contains("Some content"),
            "page content missing: {}",
            result.html
        );
    }

    /// Reused renderer must reset per-render state — HTML mode heading IDs.
    ///
    /// Pre-refactor, calling `render` twice on the same renderer
    /// would carry `HeadingAccumulator::id_counts` across the boundary, so
    /// the second render's heading IDs got "-1" suffixes. The fix is
    /// structural: each render constructs a fresh `Walker` (and fresh
    /// `HeadingAccumulator`).
    #[test]
    fn test_reused_renderer_resets_heading_ids_html_mode() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new().with_title_extraction();
        let md = "# My Title\n\n## Section\n\nbody";

        let r1 = renderer.render(md, Pipeline::new());
        let r2 = renderer.render(md, Pipeline::new());

        assert_eq!(r1.title, r2.title, "title must match across renders");
        assert_eq!(r1.toc, r2.toc, "TOC must match across renders");
        // Full HTML equality catches leakage of any per-render scratch field,
        // not just id_counts — list_stack, alert_stack, scopes, etc.
        assert_eq!(
            r1.html, r2.html,
            "reused renderer must produce identical HTML for identical input"
        );
        // Diagnostic-friendly negative assertions: the bug-shaped HTML must not appear.
        assert!(
            !r2.html.contains(r#"id="my-title-1""#),
            "second render leaked stale id-count: {}",
            r2.html
        );
        assert!(
            !r2.html.contains(r#"id="section-1""#),
            "second render leaked stale id-count: {}",
            r2.html
        );
    }

    /// Reused renderer must reset per-render state — `TITLE_AS_METADATA = true`
    /// backends (Confluence, SearchDocument).
    ///
    /// Pre-refactor, `HeadingAccumulator::seen_first_h1` stayed true across
    /// renders, so the second render's first H1 was no longer recognized
    /// as the title-extracted heading and `result.title` came back as `None`.
    /// `HtmlBackend` doesn't exhibit this because its first-H1 detection
    /// uses `self.title.is_none()` (cleared by `take_title`); this test uses
    /// `SearchDocumentBackend` (which sets `TITLE_AS_METADATA = true`, same
    /// as the downstream `ConfluenceBackend`).
    #[test]
    fn test_reused_renderer_resets_title_confluence_mode() {
        use crate::SearchDocumentBackend;

        let renderer = MarkdownRenderer::<SearchDocumentBackend>::new().with_title_extraction();
        let md = "# Page Title\n\nbody content";

        let r1 = renderer.render(md, Pipeline::new());
        let r2 = renderer.render(md, Pipeline::new());

        // Full HTML equality catches body-level per-render state leaks
        // beyond the title-extraction bug.
        assert_eq!(
            r1.html, r2.html,
            "reused renderer must produce identical body for identical input"
        );

        assert_eq!(
            r1.title.as_deref(),
            Some("Page Title"),
            "first render must extract title in Confluence mode"
        );
        assert_eq!(
            r2.title.as_deref(),
            Some("Page Title"),
            "second render's title must be extracted, not None — Confluence-mode \
             seen_first_h1 reset bug"
        );
    }

    /// Reused renderer must reset per-render state — code-block index.
    ///
    /// Pre-refactor, `Walker::code_block_index` grew monotonically across
    /// renders, so a counting processor would see indices 2,3 on the
    /// second render of a two-block document instead of 0,1. The two-block
    /// document distinguishes "doesn't reset" from "doesn't increment"
    /// (a single-block test would pass for the wrong reason).
    #[test]
    fn test_reused_renderer_resets_code_block_index() {
        struct CountingProcessor;
        impl CodeBlockProcessor for CountingProcessor {
            fn process(
                &mut self,
                _language: &str,
                _attrs: &HashMap<String, String>,
                _source: &str,
                index: usize,
            ) -> ProcessResult {
                ProcessResult::Inline(format!("<p>BLOCK_{index}</p>"))
            }
        }

        let md = "```a\nfirst\n```\n\n```b\nsecond\n```";
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let r1 = renderer.render(md, Pipeline::new().with_processor(CountingProcessor));
        let r2 = renderer.render(md, Pipeline::new().with_processor(CountingProcessor));

        // Full HTML equality catches per-render state leaks beyond the
        // code-block-index bug (e.g., list_stack, alert_stack, text_buffer).
        assert_eq!(
            r1.html, r2.html,
            "reused renderer must produce identical HTML for identical input"
        );

        // Both renders must see 0 and 1 — structural property of a two-block doc.
        assert!(
            r1.html.contains("BLOCK_0"),
            "r1 missing BLOCK_0: {}",
            r1.html
        );
        assert!(
            r1.html.contains("BLOCK_1"),
            "r1 missing BLOCK_1: {}",
            r1.html
        );
        assert!(
            r2.html.contains("BLOCK_0"),
            "r2 missing BLOCK_0: {}",
            r2.html
        );
        assert!(
            r2.html.contains("BLOCK_1"),
            "r2 missing BLOCK_1: {}",
            r2.html
        );
        // Only the second render can expose the monotonic-index bug (r1 only
        // has two blocks, so it can never produce BLOCK_2/3 regardless).
        assert!(
            !r2.html.contains("BLOCK_2"),
            "r2 leaked BLOCK_2: {}",
            r2.html
        );
        assert!(
            !r2.html.contains("BLOCK_3"),
            "r2 leaked BLOCK_3: {}",
            r2.html
        );
    }

    /// A panic inside a processor unwinds through `Walker`, which is dropped
    /// on the stack. The façade's `RenderConfig` and the renderer's own
    /// scratch state are untouched, so subsequent renders work cleanly.
    ///
    /// Scope limit: the panicking processor itself stays in the renderer's
    /// processor list with whatever internal state it had — the renderer
    /// can't fix the processor's invariants. This test uses two distinct
    /// processors gated on different languages so the second render
    /// doesn't re-invoke the broken one.
    #[test]
    fn test_panic_in_processor_does_not_poison_renderer() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        struct ExplodingProcessor;
        impl CodeBlockProcessor for ExplodingProcessor {
            fn process(
                &mut self,
                language: &str,
                _attrs: &HashMap<String, String>,
                _source: &str,
                _index: usize,
            ) -> ProcessResult {
                assert!(language != "explode", "intentional panic for test",);
                ProcessResult::PassThrough
            }
        }

        struct SafeProcessor;
        impl CodeBlockProcessor for SafeProcessor {
            fn process(
                &mut self,
                language: &str,
                _attrs: &HashMap<String, String>,
                _source: &str,
                _index: usize,
            ) -> ProcessResult {
                if language == "safe" {
                    ProcessResult::Inline("<p>safe</p>".to_owned())
                } else {
                    ProcessResult::PassThrough
                }
            }
        }

        let renderer = MarkdownRenderer::<HtmlBackend>::new().with_title_extraction();

        // First render hits the panicking processor. AssertUnwindSafe is
        // needed because Pipeline carries `Vec<Box<dyn CodeBlockProcessor>>`,
        // which doesn't implement UnwindSafe by default — we explicitly accept
        // that risk because the whole point of this test is to verify recovery.
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            renderer.render(
                "# Boom\n\n```explode\n```",
                Pipeline::new()
                    .with_processor(ExplodingProcessor)
                    .with_processor(SafeProcessor),
            )
        }));
        assert!(panicked.is_err(), "exploding processor must panic");

        // Second render must work cleanly and produce a coherent result.
        let r = renderer.render(
            "# Page\n\n```safe\n```\n\n## Section",
            Pipeline::new().with_processor(SafeProcessor),
        );

        assert_eq!(
            r.title.as_deref(),
            Some("Page"),
            "renderer scratch must be clean: title extraction works again"
        );
        assert!(
            r.html.contains(r#"id="page""#),
            "renderer scratch must be clean: heading id is 'page', not stale 'page-1'"
        );
        assert!(
            r.html.contains(r#"id="section""#),
            "renderer scratch must be clean: 'section' id is fresh"
        );
        assert!(
            r.html.contains("<p>safe</p>"),
            "safe processor must still produce output: {}",
            r.html
        );
    }

    /// Wikilink-bearing document renders identically across renderer reuse.
    ///
    /// Spec-style test: under well-formed event streams pulldown-cmark
    /// emits the `WikiLink` raw-target Text event immediately after the
    /// tag opens, so `skip_wikilink_text` is consumed back to `false`
    /// within the same render — this test would pass even pre-refactor.
    /// Its value is documenting that the Walker-construction-per-render
    /// guarantee covers wikilink paths, so future changes to the
    /// wikilink event handling can't accidentally introduce reuse-
    /// dependent state.
    #[test]
    fn test_wikilink_input_renders_identically_across_renderer_reuse() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new().with_wikilinks(true);
        // Without sections, all wikilinks render as broken links —
        // exercises the skip_wikilink_text path identically.
        let md = "Body with a [[target]] link inside.";

        let r1 = renderer.render(md, Pipeline::new());
        let r2 = renderer.render(md, Pipeline::new());

        assert_eq!(
            r1.html, r2.html,
            "reused renderer must produce identical HTML for wikilink input"
        );
    }

    #[test]
    fn shared_renderer_renders_concurrently() {
        use std::sync::Arc;
        use std::thread;

        let renderer: Arc<MarkdownRenderer<HtmlBackend>> =
            Arc::new(MarkdownRenderer::new().with_title_extraction());

        let r1 = Arc::clone(&renderer);
        let r2 = Arc::clone(&renderer);

        let t1 = thread::spawn(move || r1.render("# Thread One\n\nHello.", Pipeline::new()));
        let t2 = thread::spawn(move || r2.render("# Thread Two\n\nWorld.", Pipeline::new()));

        let res1 = t1.join().expect("thread 1 panicked");
        let res2 = t2.join().expect("thread 2 panicked");

        assert_eq!(res1.title.as_deref(), Some("Thread One"));
        assert_eq!(res2.title.as_deref(), Some("Thread Two"));
        assert!(res1.html.contains("Hello"));
        assert!(res2.html.contains("World"));
    }

    // Task 9: Warning isolation test
    #[test]
    fn fresh_pipeline_yields_fresh_warnings_per_render() {
        use crate::TabsDirective;
        use crate::directive::DirectiveProcessor;

        // Markdown with an unclosed :::tab container — emits one warning per
        // render via DirectiveProcessor::finalize.
        let md = ":::tab[A]\nbody";

        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let make_pipeline = || {
            Pipeline::new()
                .with_directives(DirectiveProcessor::new().with_container(TabsDirective::new()))
        };

        let r1 = renderer.render(md, make_pipeline());
        let r2 = renderer.render(md, make_pipeline());

        // Each render emits exactly one warning. If processor state leaked
        // across renders (the pre-refactor bug), r2 would see r1's warning
        // plus its own.
        assert_eq!(r1.warnings.len(), 1, "r1 warnings: {:?}", r1.warnings);
        assert_eq!(r2.warnings.len(), 1, "r2 warnings: {:?}", r2.warnings);
        assert_eq!(r1.warnings, r2.warnings);
        assert!(
            r1.warnings[0].contains("unclosed container directive"),
            "unexpected warning: {}",
            r1.warnings[0]
        );
    }
}
