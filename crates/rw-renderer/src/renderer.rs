//! Generic markdown renderer with pluggable backend.
//!
//! See the [crate-level documentation](crate) for an overview and examples.

use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::Arc;

use rw_diagrams::{DiagramRouter, Providers, ResolveContext};
use rw_parser::Parser;
use rw_sections::Sections;

use crate::backend::RenderBackend;
use crate::config::{RenderConfig, TitleResolver};
use crate::pass::RenderPass;
use crate::toc::TocEntry;
use crate::walker::{Paused, Walker};

/// Output produced by [`MarkdownRenderer::render`].
///
/// Contains the rendered markup, an optional page title extracted from the
/// first H1 heading, table-of-contents entries for heading navigation, and
/// any warnings raised while resolving diagrams or rendering directives.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Providers};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .render("# Welcome\n\nHello **world**.", &Providers::empty());
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
/// [`render`](Self::render), the one-call form of that round trip.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Providers};
///
/// let renderer = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .with_base_path("/docs/guide");
///
/// let result = renderer.render("# Guide\n\nSee [setup](setup.md).", &Providers::empty());
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
    /// Only used by HTML backend. Confluence backend ignores this.
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

    /// Render `markdown`, resolving any diagram fences through `providers`.
    ///
    /// The one-call form of [`begin`](Self::begin) + resolve +
    /// [`finish`](RenderPass::finish), for a caller with no per-page context to
    /// supply and no reason to inspect the requests first. A caller that writes
    /// diagram bytes to disk, or that needs to know whether a failure was
    /// transient before caching, uses the three-step form.
    pub fn render(&self, markdown: &str, providers: &Providers) -> RenderResult {
        let pass = self.begin(markdown);
        let resolutions = providers.resolve(pass.requests(), &ResolveContext::default());
        pass.finish(&resolutions)
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
        // `parser` and `walker` are disjoint locals, so the two `&mut`
        // borrows never conflict — which is what makes the lending
        // `next` usable without a `LendingIterator` trait.
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
    use crate::{Asset, DiagramContent, HtmlBackend};
    use rw_diagrams::{DiagramError, DiagramProvider, Resolved};
    use rw_sections::{Namespace, Section};

    fn render_html(markdown: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new().render(markdown, &Providers::empty())
    }

    fn render_html_with_title(markdown: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_title_extraction()
            .render(markdown, &Providers::empty())
    }

    fn render_with_base_path(markdown: &str, base_path: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .render(markdown, &Providers::empty())
    }

    fn render_with_origin(markdown: &str, base_path: &str, origin: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .with_origin(origin)
            .render(markdown, &Providers::empty())
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
    fn test_html_blockquote() {
        let result = render_html("> Note");
        assert!(result.html.contains("<blockquote>"));
        assert!(result.html.contains("</blockquote>"));
    }

    #[test]
    fn test_note_alert() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!NOTE]\n> This is a **note**.", &Providers::empty());
        assert!(result.html.contains("alert-note"));
        assert!(result.html.contains("<strong>note</strong>"));
    }

    #[test]
    fn test_tip_alert() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!TIP]\n> This is a tip.", &Providers::empty());
        assert!(result.html.contains("alert-tip"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_important_alert() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "> [!IMPORTANT]\n> Critical information.",
            &Providers::empty(),
        );
        assert!(result.html.contains("alert-important"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_warning_alert() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!WARNING]\n> Be careful!", &Providers::empty());
        assert!(result.html.contains("alert-warning"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_caution_alert() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> [!CAUTION]\n> Dangerous operation.", &Providers::empty());
        assert!(result.html.contains("alert-caution"));
        assert!(result.html.contains(r#"<svg class="alert-icon""#));
    }

    #[test]
    fn test_alert_with_list() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "> [!WARNING]\n> Be careful:\n> - Item 1\n> - Item 2",
            &Providers::empty(),
        );
        assert!(result.html.contains("alert-warning"));
        assert!(result.html.contains("<ul>"));
        assert!(result.html.contains("<li>"));
    }

    #[test]
    fn test_regular_blockquote_unchanged() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("> Just a regular quote", &Providers::empty());
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
    fn test_origin_preserves_external_links() {
        let result = render_with_origin("[Ext](https://example.com)", "/", "docs");
        assert!(result.html.contains(r#"href="https://example.com""#));
    }

    fn render_leaf(markdown: &str, base_path: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .with_is_dir(false)
            .render(markdown, &Providers::empty())
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
        let result = renderer.render("Hello", &Providers::empty());
        assert_eq!(result.html, "<p>Hello</p>");
    }

    // Diagram fences: the one-call `render` path

    /// A provider that claims `plantuml` and answers from the fence source
    /// without touching a network.
    ///
    /// A body starting with `warn ` resolves but reports the rest of the line as
    /// a provider warning; anything else comes back as an SVG echoing the
    /// source, so a test can see which fence filled which hole. A `handles` call
    /// for `explode` panics, which is how a test gets a panic *during* the walk.
    struct StubProvider;

    impl StubProvider {
        fn providers() -> Providers {
            Providers::empty().with(Arc::new(Self) as Arc<dyn DiagramProvider>)
        }
    }

    impl DiagramProvider for StubProvider {
        fn handles(&self, language: &str) -> bool {
            assert!(language != "explode", "intentional panic for test");
            language == "plantuml"
        }

        fn resolve(
            &self,
            requests: &[crate::DiagramRequest],
            _ctx: &ResolveContext<'_>,
        ) -> Vec<Result<Resolved, DiagramError>> {
            requests
                .iter()
                .map(|request| {
                    let source = request.source.trim();
                    Ok(Resolved {
                        asset: Asset::Inline(DiagramContent::Svg(format!("<svg>{source}</svg>"))),
                        size: None,
                        digest: "0".to_owned(),
                        warnings: source
                            .strip_prefix("warn ")
                            .map(|rest| Vec::from([rest.to_owned()]))
                            .unwrap_or_default(),
                    })
                })
                .collect()
        }
    }

    /// Build a renderer routing its fences through `providers`.
    fn diagram_renderer(providers: &Providers) -> MarkdownRenderer<HtmlBackend> {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_diagram_languages(Arc::new(providers.clone()) as Arc<dyn DiagramRouter>)
    }

    /// `render` is `begin` + resolve + `finish`, so the providers it is handed
    /// must actually be asked: a `render` that skipped the resolve step would
    /// fall back to `diagram_source` and still produce a plausible page.
    #[test]
    fn render_resolves_diagram_fences_through_the_providers_it_is_given() {
        let providers = StubProvider::providers();
        let result = diagram_renderer(&providers)
            .render("before\n\n```plantuml\nA -> B\n```\n\nafter\n", &providers);

        assert_eq!(
            result.html,
            concat!(
                "<p>before</p>",
                r#"<figure class="diagram" data-diagram-id="diagram-0">"#,
                "<rw-diagram><svg>A -> B</svg></rw-diagram></figure>",
                "<p>after</p>",
            ),
        );
    }

    /// A fence no provider claims is not a diagram: it renders as a code block,
    /// with syntax highlighting, exactly as it does with no router at all.
    #[test]
    fn an_unclaimed_fence_renders_as_a_code_block() {
        let providers = StubProvider::providers();
        let result = diagram_renderer(&providers).render("```rust\nfn main() {}\n```", &providers);

        assert!(
            result.html.contains(r#"class="language-rust""#),
            "got: {}",
            result.html
        );
        assert!(result.html.contains("fn main() {}"), "got: {}", result.html);
    }

    /// A fence with no info string has no language to route on, so the router is
    /// never consulted and the block renders plain.
    #[test]
    fn a_fence_without_a_language_renders_as_a_code_block() {
        let providers = StubProvider::providers();
        let result = diagram_renderer(&providers).render("```\nplain text\n```", &providers);

        assert!(result.html.contains("<pre><code>"), "got: {}", result.html);
        assert!(result.html.contains("plain text"), "got: {}", result.html);
    }

    #[test]
    fn test_render_result_empty_warnings_by_default() {
        let result = render_html("Hello");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_render_convenience() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("# Hello\n\n**World**", &Providers::empty());
        assert!(result.html.contains("<h1"));
        assert!(result.html.contains("<strong>World</strong>"));
    }

    #[test]
    fn test_gfm_tables_always_rendered() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render("| A | B |\n|---|---|\n| 1 | 2 |", &Providers::empty());
        assert!(result.html.contains("<table>"));
    }

    // Directive integration tests

    #[test]
    fn tabs_render_with_no_providers_configured() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        // Block directives are blank-line separated: each `:::` delimiter is
        // its own paragraph so pulldown-cmark emits it standalone.
        let result = renderer.render(
            r"::::tabs

:::tab[macOS]

Install with Homebrew.

:::

:::tab[Linux]

Install with apt.

:::

::::",
            &Providers::empty(),
        );

        // Should have accessible tab structure
        assert!(result.html.contains(r#"role="tablist""#));
        assert!(result.html.contains(r#"role="tab""#));
        assert!(result.html.contains(r#"role="tabpanel""#));
        assert!(result.html.contains("macOS"));
        assert!(result.html.contains("Linux"));
    }

    #[test]
    fn status_renders_with_no_providers_configured() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let result = renderer.render(
            "Billing is :status[On Track]{color=green} this quarter.",
            &Providers::empty(),
        );

        assert!(
            result
                .html
                .contains(r#"<span class="status status-green">On Track</span>"#),
            "got: {}",
            result.html
        );
    }

    /// The parser synthesizes an `implicit` close for the unclosed `:::foo` at
    /// EOF, and an implicit close of a *literal* (unrecognized) container must
    /// not warn: the opener already rendered as plain prose, so there is
    /// nothing for the author to fix. Only the tab built-ins warn on implicit
    /// closes — this pins the boundary of that guard.
    #[test]
    fn unclosed_unknown_container_renders_literally_without_warning() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(":::foo\n\nbody", &Providers::empty());

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
    fn test_directives_warnings_included() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        // Unclosed tabs should produce warning. Block directives are blank-line
        // separated, so the `:::tab` delimiter stands alone as its own paragraph.
        let result = renderer.render("::::tabs\n\n:::tab[Test]\n\nContent", &Providers::empty());

        assert!(result.warnings.iter().any(|w| w.contains("unclosed")));
    }

    #[test]
    fn test_frontmatter_terminator_does_not_swallow_body() {
        // Frontmatter must terminate at `---` and not bleed into body parsing.
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render(
            "---\ntitle: hello\n---\n\n# Body\n\nParagraph.\n",
            &Providers::empty(),
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
        let result = renderer.render("## See [[domain:billing::overview]]\n", &Providers::empty());

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
        let result = renderer.render("[Billing](../../../overview.md)", &Providers::empty());
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
        let result = renderer.render("[Use Cases](./use-cases.md)", &Providers::empty());
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
        let result = renderer.render("[Google](https://google.com)", &Providers::empty());
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
            &Providers::empty(),
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
        let result = renderer.render("[Billing](../billing/index.md)", &Providers::empty());
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
        let result = renderer.render("[Use Cases](./use-cases.md)", &Providers::empty());
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
            .render(markdown, &Providers::empty())
    }

    fn render_wikilink_with_base(markdown: &str, base: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_wikilinks(true)
            .with_sections(wikilink_sections())
            .with_base_path(base)
            .with_title_resolver(StaticTitleResolver)
            .render(markdown, &Providers::empty())
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
            .render(md, &Providers::empty());

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
        let result = renderer.render(markdown, &Providers::empty());
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

        let r1 = renderer.render(md, &Providers::empty());
        let r2 = renderer.render(md, &Providers::empty());

        assert_eq!(r1.title, r2.title, "title must match across renders");
        assert_eq!(r1.toc, r2.toc, "TOC must match across renders");
        // Full HTML equality catches leakage of any per-render scratch field,
        // not just id_counts — list depth, alert_stack, scopes, etc.
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
    /// backends (Confluence, `SearchDocument`).
    ///
    /// Pre-refactor, `HeadingAccumulator::seen_first_h1` stayed true across
    /// renders, so the second render's first H1 was no longer recognized
    /// as the title-extracted heading and `result.title` came back as `None`.
    /// Both modes detect the first H1 via `seen_first_h1` on the per-render
    /// accumulator; this test pins the Confluence-mode path with
    /// `SearchDocumentBackend` (which sets `TITLE_AS_METADATA = true`, same
    /// as the downstream `ConfluenceBackend`).
    #[test]
    fn test_reused_renderer_resets_title_confluence_mode() {
        use crate::SearchDocumentBackend;

        let renderer = MarkdownRenderer::<SearchDocumentBackend>::new().with_title_extraction();
        let md = "# Page Title\n\nbody content";

        let r1 = renderer.render(md, &Providers::empty());
        let r2 = renderer.render(md, &Providers::empty());

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
    /// renders, so the second render of a two-fence document numbered its
    /// diagrams 2 and 3 instead of 0 and 1. That index is what a diagram's hole
    /// is keyed by and what its warnings and error figures are numbered with, so
    /// a leak is user-visible. The two-fence document distinguishes "doesn't
    /// reset" from "doesn't increment" (a single-fence test would pass for the
    /// wrong reason).
    #[test]
    fn test_reused_renderer_resets_code_block_index() {
        // The leading `rust` fence makes the code-block index differ from the
        // diagram's position among diagrams, so the numbering below can only
        // come from `code_block_index`.
        let md = "```rust\nlet x = 1;\n```\n\n```plantuml\nwarn first\n```\n\n```plantuml\nwarn second\n```";
        let providers = StubProvider::providers();
        let renderer = diagram_renderer(&providers);

        let r1 = renderer.render(md, &providers);
        let r2 = renderer.render(md, &providers);

        // Full HTML equality catches per-render state leaks beyond the
        // code-block-index bug (e.g., list depth, alert_stack, scopes).
        assert_eq!(
            r1.html, r2.html,
            "reused renderer must produce identical HTML for identical input"
        );

        // Both renders must number the fences 1 and 2 — a structural property
        // of this document, and the only render whose numbering can expose the
        // monotonic-index bug is the second.
        assert_eq!(r1.warnings, ["diagram 1: first", "diagram 2: second"]);
        assert_eq!(
            r2.warnings, r1.warnings,
            "second render leaked a stale code-block index"
        );
    }

    /// A panic raised during the walk unwinds through `Walker`, which is dropped
    /// on the stack. The façade's `RenderConfig` and the renderer's own scratch
    /// state are untouched, so subsequent renders work cleanly.
    ///
    /// The router is consulted for every fence language during the walk, so a
    /// panicking `handles` is how this test panics mid-walk rather than during
    /// resolution, which happens after the walk has already finished.
    #[test]
    fn test_panic_during_the_walk_does_not_poison_renderer() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        let providers = StubProvider::providers();
        let renderer = diagram_renderer(&providers).with_title_extraction();

        // AssertUnwindSafe because the borrowed renderer is not `UnwindSafe` —
        // we explicitly accept that, since verifying recovery is the point.
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            renderer.render("# Boom\n\n```explode\n```", &providers)
        }));
        assert!(panicked.is_err(), "the router must panic on `explode`");

        // Second render must work cleanly and produce a coherent result.
        let r = renderer.render(
            "# Page\n\n```plantuml\nA -> B\n```\n\n## Section",
            &providers,
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
            r.html.contains("<svg>A -> B</svg>"),
            "the diagram must still resolve: {}",
            r.html
        );
    }

    /// Wikilink-bearing document renders identically across renderer reuse.
    ///
    /// Spec-style test: under well-formed event streams cmark emits the
    /// `WikiLink` raw-target Text event immediately after the tag opens, so
    /// the parser's `skip_wikilink_text` is consumed back to `false` within
    /// the same render — this test would pass even pre-refactor. Its value is
    /// documenting that the construct-both-halves-per-render guarantee covers
    /// wikilink paths, so future changes to the wikilink event handling can't
    /// accidentally introduce reuse-dependent state.
    #[test]
    fn test_wikilink_input_renders_identically_across_renderer_reuse() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new().with_wikilinks(true);
        // Without sections, all wikilinks render as broken links —
        // exercises the skip_wikilink_text path identically.
        let md = "Body with a [[target]] link inside.";

        let r1 = renderer.render(md, &Providers::empty());
        let r2 = renderer.render(md, &Providers::empty());

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

        let t1 = thread::spawn(move || r1.render("# Thread One\n\nHello.", &Providers::empty()));
        let t2 = thread::spawn(move || r2.render("# Thread Two\n\nWorld.", &Providers::empty()));

        let res1 = t1.join().expect("thread 1 panicked");
        let res2 = t2.join().expect("thread 2 panicked");

        assert_eq!(res1.title.as_deref(), Some("Thread One"));
        assert_eq!(res2.title.as_deref(), Some("Thread Two"));
        assert!(res1.html.contains("Hello"));
        assert!(res2.html.contains("World"));
    }

    #[test]
    fn each_render_starts_with_fresh_warnings() {
        // Markdown with an unclosed ::::tabs group (its one tab closes
        // normally) — emits one warning per render, raised as the walk renders
        // the parser's synthesized close. Tabs are a walker built-in. Block
        // directives are blank-line separated, so each delimiter stands alone.
        let md = "::::tabs\n\n:::tab[A]\n\nbody\n\n:::";

        let renderer = MarkdownRenderer::<HtmlBackend>::new();

        let r1 = renderer.render(md, &Providers::empty());
        let r2 = renderer.render(md, &Providers::empty());

        // Each render emits exactly one warning. If walk state leaked across
        // renders, r2 would see r1's warning plus its own.
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
