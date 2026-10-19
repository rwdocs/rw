//! Generic markdown renderer with pluggable backend.
//!
//! See the [crate-level documentation](crate) for an overview and examples.

use std::collections::{BTreeSet, HashSet};
use std::marker::PhantomData;
use std::sync::Arc;

use rw_diagrams::{DiagramRouter, Resolutions};
use rw_parser::Parser;
use rw_sections::Sections;

use crate::backend::RenderBackend;
use crate::config::{RenderConfig, TitleResolver};
use crate::pass::RenderPass;
use crate::toc::TocEntry;
use crate::walker::{Paused, Walker};

/// Output produced by [`RenderPass::finish`] and [`MarkdownRenderer::render`].
///
/// Contains the rendered markup, an optional page title extracted from the
/// first H1 heading, table-of-contents entries for heading navigation, and
/// any warnings raised while resolving diagrams or rendering directives.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .render("# Welcome\n\nHello **world**.");
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
    /// Warnings generated during conversion (e.g., diagram provider warnings,
    /// unclosed container directives).
    pub warnings: Vec<String>,
    /// Canonical section refs (`"kind:namespace/name"`) this render referenced,
    /// via prose links (markdown + wikilinks) and diagram `$link`s. Deduped and
    /// deterministically ordered. Empty when the page references no sections.
    pub section_refs: BTreeSet<String>,
}

/// Generic markdown renderer with pluggable backend.
///
/// Tokenizes markdown and interprets the result into HTML or XHTML depending
/// on the [`RenderBackend`] implementation (`B`). Common elements (tables,
/// lists, inline formatting) are handled generically; format-specific elements
/// are delegated to `B`.
///
/// The entry points are [`begin`](Self::begin), which walks the document and
/// hands back a [`RenderPass`] to resolve its diagrams against, and
/// [`render`](Self::render), for a caller with no diagrams to resolve.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer};
///
/// let renderer = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .with_base_path("/docs/guide");
///
/// let result = renderer.render("# Guide\n\nSee [setup](setup.md).");
/// assert_eq!(result.title.as_deref(), Some("Guide"));
/// assert!(result.html.contains(r#"href="/docs/guide/setup""#));
/// ```
pub struct MarkdownRenderer<B: RenderBackend> {
    config: RenderConfig,
    _backend: PhantomData<B>,
}

impl<B: RenderBackend> MarkdownRenderer<B> {
    /// Create a new renderer. GFM features (tables, strikethrough, task
    /// lists, alerts) are always enabled.
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
    /// Relative markdown links are resolved against it only by the HTML
    /// backend; current-section wikilinks (`[[::path]]`) use it in every
    /// backend.
    #[must_use]
    pub fn with_base_path(mut self, path: impl Into<String>) -> Self {
        self.config.base_path = Some(path.into());
        self
    }

    /// Set whether the current page's URL denotes a directory (`true`, from
    /// `index.md` or the root/README homepage) rather than a single file
    /// (`false`, a leaf `name.md`). Defaults to `true`.
    ///
    /// A leaf page resolves relative links against its *containing directory*
    /// (`CommonMark` semantics): `./sibling.md` is a sibling of the source file,
    /// not a child of it. Setting this `false` drops the page's own URL slug
    /// from the link base. Only affects the HTML backend's relative-link
    /// resolution; wikilink resolution is unaffected.
    #[must_use]
    pub fn with_is_dir(mut self, is_dir: bool) -> Self {
        self.config.is_dir = is_dir;
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

    /// Set the url paths (no leading slash) whose page a `README.md` serves.
    ///
    /// A link or wikilink to `<path>/README` then leads to `/<path>`, keeping
    /// its fragment and query.
    #[must_use]
    pub fn with_readme_pages(mut self, pages: Arc<HashSet<String>>) -> Self {
        self.config.readme_pages = Some(pages);
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
    /// When enabled, the parser recognizes `[[target]]` and
    /// `[[target|display text]]` syntax. Links are resolved through
    /// [`Sections`] (see [`with_sections`](Self::with_sections)) and display
    /// text is looked up via [`with_title_resolver`](Self::with_title_resolver).
    /// Each piece degrades gracefully when omitted — see the
    /// [crate-level wikilink documentation](crate#wikilinks).
    #[must_use]
    pub fn with_wikilinks(mut self) -> Self {
        self.config.wikilinks = true;
        self
    }

    /// Set a title resolver for wikilink display text.
    ///
    /// When a wikilink has no explicit display text (`[[target]]` vs.
    /// `[[target|text]]`), the renderer calls the resolver to look up a
    /// human-readable page title. Without a resolver, or when it returns
    /// `None`, display text falls back to the last path segment (e.g.,
    /// `[[domain:billing::overview]]` displays as "overview") or the section
    /// name for root links.
    #[must_use]
    pub fn with_title_resolver(mut self, resolver: impl TitleResolver + 'static) -> Self {
        self.config.title_resolver = Some(Box::new(resolver));
        self
    }

    /// Treat fences whose language `router` claims as diagrams: each reserves a
    /// hole and appears in [`RenderPass::requests`] instead of rendering as a
    /// code block.
    ///
    /// A predicate rather than a fixed language set, because a provider matches
    /// prefixes (`kroki-mermaid`) and only it knows what it serves. With no
    /// router configured every fence is an ordinary code block.
    #[must_use]
    pub fn with_diagram_languages(mut self, router: Arc<dyn DiagramRouter>) -> Self {
        self.config.diagram_router = Some(router);
        self
    }

    /// Render `markdown` without resolving its diagrams: each diagram fence
    /// renders as a [`RenderBackend::code_block`]. A caller that resolves
    /// them uses [`begin`](Self::begin).
    #[must_use]
    pub fn render(&self, markdown: &str) -> RenderResult {
        self.begin(markdown).finish(&Resolutions::new())
    }

    /// Walk `markdown`, reserving a hole at each diagram fence, and hand back
    /// the [`RenderPass`] carrying those fences as requests.
    ///
    /// The caller resolves them however it likes — over HTTP, from a cache, not
    /// at all — and [`RenderPass::finish`] turns each resolution into markup
    /// through the backend and assembles the page. Nothing here does I/O, and
    /// the pass exposes no way to read the half-built markup: it is only
    /// obtainable through `finish`.
    #[must_use]
    pub fn begin(&self, markdown: &str) -> RenderPass<'_, B> {
        RenderPass::new(&self.config, self.walk(markdown))
    }

    /// Tokenize and interpret `markdown`, stopping short of assembly.
    fn walk(&self, markdown: &str) -> Paused {
        let mut parser = Parser::new(markdown, self.config.wikilinks, B::TOKENIZE_DIRECTIVES);
        let mut walker = Walker::<B>::new(&self.config);
        while let Some(event) = parser.next() {
            walker.handle(event);
        }
        walker.pause()
    }
}

impl<B: RenderBackend> Default for MarkdownRenderer<B> {
    fn default() -> Self {
        Self::new()
    }
}

// Compile-time contract: this fires in every build (not only `cargo test`),
// so a future change that breaks the auto-trait shape — e.g., adding an `Rc`
// to `RenderConfig` — fails the build instead of slipping past test-gated
// assertions.
//
// `MarkdownRenderer<B>` must stay `Send + Sync` so it can be parked in an
// `Arc` and used by many request handlers.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MarkdownRenderer<crate::HtmlBackend>>();
};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::HtmlBackend;
    use rw_sections::{Namespace, Section};

    fn render_html(markdown: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new().render(markdown)
    }

    fn render_html_with_title(markdown: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_title_extraction()
            .render(markdown)
    }

    fn render_with_base_path(markdown: &str, base_path: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .render(markdown)
    }

    fn render_with_origin(markdown: &str, base_path: &str, origin: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .with_origin(origin)
            .render(markdown)
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

    /// Two H1s: only the first is the title; the second renders as a normal
    /// heading with a `toc` entry. Pins the `!seen_first_h1` half of the
    /// HTML-mode title condition — without it every later H1 would re-capture
    /// the title and vanish from the `toc`.
    #[test]
    fn test_second_h1_is_not_the_title() {
        let result = render_html_with_title("# First\n\ntext\n\n# Second");

        assert_eq!(result.title.as_deref(), Some("First"));
        assert_eq!(result.toc.len(), 1, "toc: {:?}", result.toc);
        assert_eq!(result.toc[0].level, 1);
        assert_eq!(result.toc[0].title, "Second");
    }

    #[test]
    fn test_html_code_block() {
        let result = render_html("```rust\nfn main() {}\n```");
        assert!(result.html.contains(r#"class="language-rust""#));
        assert!(result.html.contains("fn main() {}"));
    }

    #[test]
    fn test_alerts() {
        for (marker, class) in [
            ("NOTE", "alert-note"),
            ("TIP", "alert-tip"),
            ("IMPORTANT", "alert-important"),
            ("WARNING", "alert-warning"),
            ("CAUTION", "alert-caution"),
        ] {
            let result = render_html(&format!("> [!{marker}]\n> This is **bold**."));
            assert!(result.html.contains(class), "{marker}: {}", result.html);
            assert!(
                result.html.contains(r#"<svg class="alert-icon""#),
                "{marker}: {}",
                result.html
            );
            assert!(
                result.html.contains("<strong>bold</strong>"),
                "{marker}: {}",
                result.html
            );
        }
    }

    #[test]
    fn test_alert_with_list() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!WARNING]\n> Be careful:\n> - Item 1\n> - Item 2");
        assert!(result.html.contains("alert-warning"));
        assert!(result.html.contains("<ul>"));
        assert!(result.html.contains("<li>"));
    }

    #[test]
    fn test_regular_blockquote_unchanged() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> Just a regular quote");
        assert!(result.html.contains("<blockquote>"));
        assert!(result.html.contains("</blockquote>"));
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
    fn test_html_table_has_scroll_wrapper() {
        let result = render_html("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(
            result.html.contains(
                r#"<div class="table-wrap" role="group" tabindex="0" aria-label="Table"><table>"#
            ),
            "missing scroll wrapper, got: {}",
            result.html,
        );
        assert!(
            result.html.contains("</tbody></table></div>"),
            "missing wrapper close, got: {}",
            result.html,
        );
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
    fn links_to_a_readme_lead_to_the_folder_it_serves() {
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/")
            .with_readme_pages(Arc::new(HashSet::from([String::new(), "a".to_owned()])))
            .render(
                "[a](README.md#intro) [b](/README?x=1) [c](README-old.md) [d](a/README.md) \
                 [e](b/README.md) [f](a/README/x.md)",
            );

        for href in [
            "/#intro",
            "/?x=1",
            "/README-old",
            "/a",
            "/b/README",
            "/a/README/x",
        ] {
            assert!(
                result.html.contains(&format!(r#"href="{href}""#)),
                "expected href={href:?} in: {}",
                result.html
            );
        }
    }

    #[test]
    fn test_origin_preserves_external_links() {
        let result = render_with_origin("[Ext](https://example.com)", "/", "docs");
        assert!(result.html.contains(r#"href="https://example.com""#));
    }

    fn render_leaf(markdown: &str, base_path: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .with_is_dir(false)
            .render(markdown)
    }

    #[test]
    fn test_leaf_sibling_link_resolves_against_parent_dir() {
        let result = render_leaf("[x](./inbox.md)", "/specs/notif");
        assert!(
            result.html.contains(r#"href="/specs/inbox""#),
            "Expected href=\"/specs/inbox\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_leaf_parent_link() {
        let result = render_leaf("[x](../x.md)", "/specs/notif");
        assert!(
            result.html.contains(r#"href="/x""#),
            "Expected href=\"/x\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_leaf_subdir_link() {
        let result = render_leaf("[x](sub/y.md)", "/specs/notif");
        assert!(
            result.html.contains(r#"href="/specs/sub/y""#),
            "Expected href=\"/specs/sub/y\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_leaf_link_with_fragment() {
        let result = render_leaf("[x](./page.md#frag)", "/specs/notif");
        assert!(
            result.html.contains(r#"href="/specs/page#frag""#),
            "Expected href=\"/specs/page#frag\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_leaf_root_level_sibling() {
        // Leaf `guide.md` at docs root (URL `/guide`); sibling lives at root.
        let result = render_leaf("[x](./sibling.md)", "/guide");
        assert!(
            result.html.contains(r#"href="/sibling""#),
            "Expected href=\"/sibling\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_index_sibling_link_unchanged() {
        // Default (is_dir = true): base is the page's own dir.
        let result = render_with_base_path("[x](./inbox.md)", "/specs/notif");
        assert!(
            result.html.contains(r#"href="/specs/notif/inbox""#),
            "Expected href=\"/specs/notif/inbox\", got: {}",
            result.html
        );
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
        let result = render_html("- [ ] Unchecked\n- [x] Checked");
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
        let result = renderer.render("Hello");
        assert_eq!(result.html, "<p>Hello</p>");
    }

    /// The parser synthesizes an `implicit` close for the unclosed `:::foo` at
    /// EOF, and an implicit close of a *literal* (unrecognized) container must
    /// not warn: the opener already rendered as plain prose, so there is
    /// nothing for the author to fix. Only the tab built-ins warn on implicit
    /// closes — this pins the boundary of that guard.
    #[test]
    fn unclosed_unknown_container_renders_literally_without_warning() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(":::foo\n\nbody");

        assert!(
            result.html.contains(":::foo"),
            "the opener renders literally: {}",
            result.html
        );
        assert!(result.html.contains("body"), "got: {}", result.html);
        assert!(
            !result.html.contains("<p>:::</p>"),
            "a synthesized close renders nothing: {}",
            result.html
        );
        assert!(
            result.warnings.is_empty(),
            "an implicit close of a literal container must not warn: {:?}",
            result.warnings
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
            .with_wikilinks()
            .with_sections(wikilink_sections())
            .with_title_resolver(StaticTitleResolver);
        let result = renderer.render("## See [[domain:billing::overview]]\n");

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
        let result = renderer.render("[Billing](../../../overview.md)");
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
        let result = renderer.render("[Use Cases](./use-cases.md)");
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
        let result = renderer.render("[Google](https://google.com)");
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
        let result = renderer.render("[Billing API](../../billing/api.md#endpoints)");
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
        let result = renderer.render("[Billing](../billing/index.md)");
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
    fn section_ref_query_on_section_root() {
        let sections = Arc::new(Sections::with_implicit_root(
            HashMap::from([(
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    namespace: Namespace::default(),
                    name: "billing".to_owned(),
                },
            )]),
            Namespace::default(),
        ));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/search".to_owned())
            .with_sections(sections);
        let result = renderer.render("[Billing](/domains/billing?tab=x)");
        assert!(
            result.html.contains(
                r#"href="/domains/billing?tab=x" data-section-ref="domain:default/billing" data-section-path="?tab=x""#
            ),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn section_ref_no_attributes_without_sections_configured() {
        let renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_base_path("/domains/billing".to_owned());
        let result = renderer.render("[Use Cases](./use-cases.md)");
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
            .with_wikilinks()
            .with_sections(wikilink_sections())
            .with_title_resolver(StaticTitleResolver)
            .render(markdown)
    }

    fn render_wikilink_with_base(markdown: &str, base: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_wikilinks()
            .with_sections(wikilink_sections())
            .with_base_path(base)
            .with_title_resolver(StaticTitleResolver)
            .render(markdown)
    }

    #[test]
    fn collects_referenced_section_refs_from_prose_links() {
        let sections = Arc::new(Sections::with_implicit_root(
            HashMap::from([(
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    namespace: Namespace::default(),
                    name: "billing".to_owned(),
                },
            )]),
            Namespace::default(),
        ));

        // Two links to the same section (must dedup), one external (must be
        // ignored), one fragment-only (no section).
        let md = "[a](/domains/billing/api) [b](/domains/billing/other) \
                  [ext](https://example.com) [frag](#top)";
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .with_sections(Arc::clone(&sections))
            .render(md);

        let refs: Vec<&str> = result.section_refs.iter().map(String::as_str).collect();
        assert_eq!(refs, ["domain:default/billing"]);
    }

    #[test]
    fn collects_referenced_section_refs_from_wikilinks() {
        // A resolved wikilink contributes its section ref to the set, exactly
        // like a markdown link.
        let result = render_wikilink("[[domain:billing::overview]]");
        let refs: Vec<&str> = result.section_refs.iter().map(String::as_str).collect();
        assert_eq!(refs, ["domain:default/billing"]);
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
    fn wikilink_to_a_readme_leads_to_the_folder_it_serves() {
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .with_wikilinks()
            .with_sections(wikilink_sections())
            .with_base_path("/domains/billing")
            .with_readme_pages(Arc::new(HashSet::from(["domains/billing/api".to_owned()])))
            .render("[[::api/README#auth]] [[::README]]");

        assert!(
            result.html.contains(
                r#"href="/domains/billing/api#auth" data-section-ref="domain:default/billing" data-section-path="api""#
            ),
            "html: {}",
            result.html
        );
        assert!(
            result.html.contains(r#"href="/domains/billing/README""#),
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
    fn wikilink_current_section_root_with_fragment() {
        let result = render_wikilink_with_base("[[::#intro]]", "/domains/billing/api");
        assert!(
            result.html.contains(r#"href="/domains/billing#intro""#),
            "html: {}",
            result.html
        );
        assert!(
            !result.html.contains("rw-broken-link"),
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
        let result = renderer.render(markdown);
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

    /// XML 1.0's `Char` production, which Confluence storage format enforces.
    fn is_xml_char(c: char) -> bool {
        matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}')
            || c >= '\u{10000}'
    }

    #[test]
    fn test_html_replaces_control_characters() {
        let markdown = "## Sec\u{1}tion\n\n\
            Text\u{0}\u{C} `co\u{2}de` [li\u{3}nk](<a\u{4}b>) ![al\u{5}t](<i\u{6}.png> \"t\u{1F}\")\n\n\
            ```py\u{7}\nblock\u{8}\u{1B}\n```\n";
        let result = render_html(markdown);

        let invalid: Vec<char> = result.html.chars().filter(|&c| !is_xml_char(c)).collect();
        assert!(invalid.is_empty(), "{invalid:?} in {}", result.html);
        for fragment in [
            "<h2 id=\"section\">Sec\u{FFFD}tion</h2>",
            "Text\u{FFFD}  ",
            "<code>co\u{FFFD}de</code>",
            "li\u{FFFD}nk</a>",
            "a\u{FFFD}b",
            "<img src=\"i\u{FFFD}.png\" title=\"t\u{FFFD}\" alt=\"al\u{FFFD}t\">",
            "<pre><code class=\"language-py\u{FFFD}\">block\u{FFFD}\u{FFFD}",
        ] {
            assert!(
                result.html.contains(fragment),
                "{fragment} in {}",
                result.html
            );
        }
    }
}
