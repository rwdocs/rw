//! Generic markdown renderer with pluggable backend.
//!
//! See the [crate-level documentation](crate) for an overview and examples.

use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::Arc;

use rw_diagrams::DiagramRouter;
use rw_parser::Parser;
use rw_sections::Sections;

use crate::backend::RenderBackend;
use crate::config::{FileResolver, ImageSrc, LinkResolver, RenderConfig, TitleResolver};
use crate::file::FilesBase;
use crate::pass::RenderPass;
use crate::toc::TocEntry;
use crate::walker::{Paused, Walker};

/// Output produced by [`RenderPass::finish`] and [`MarkdownRenderer::render`].
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .render("# Welcome\n\nHello **world**.");
///
/// assert_eq!(result.title.as_deref(), Some("Welcome"));
/// assert!(result.html.contains("<strong>world</strong>"));
/// assert!(result.warnings.is_empty());
/// ```
#[derive(Debug)]
pub struct RenderResult {
    /// Rendered markup produced by the [`RenderBackend`], in whatever format
    /// it writes.
    pub html: String,
    /// The plain text of the H1 that opens the page, below nothing but
    /// frontmatter and HTML comments; `None` when there is none or it shows no
    /// text.
    pub title: Option<String>,
    /// Table-of-contents entries, one per heading but the H1 that opens the
    /// page.
    pub toc: Vec<TocEntry>,
    /// Warnings generated during conversion (e.g., diagram provider warnings,
    /// unclosed container directives).
    pub warnings: Vec<String>,
    /// The warning for each local image the [`FileResolver`] refused, and for
    /// each diagram file image not alone in its paragraph, also in
    /// [`warnings`](Self::warnings).
    pub image_refusals: Vec<String>,
    /// Canonical section refs (`"kind:namespace/name"`) this render referenced,
    /// via prose links (markdown + wikilinks) and diagram `$link`s. Deduped and
    /// deterministically ordered. Empty when the page references no sections.
    pub section_refs: BTreeSet<String>,
    /// Project-relative paths of the files this render's local images name,
    /// as the [`FileResolver`] placed them, except diagram files. Deduped and
    /// sorted. Empty without a resolver.
    pub files: BTreeSet<String>,
}

/// Generic markdown renderer with pluggable backend.
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
/// let renderer = MarkdownRenderer::<HtmlBackend>::new();
///
/// let result = renderer.render("# Guide\n\nSee [setup](setup.md).");
/// assert_eq!(result.title.as_deref(), Some("Guide"));
/// assert!(result.html.contains(r#"href="setup.md""#));
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
            config: RenderConfig::default(),
            _backend: PhantomData,
        }
    }

    /// Leave the title H1 (see [`RenderResult::title`]) out of the output,
    /// and shift every heading level below it up by one, for a target that
    /// shows the title apart from the page body.
    #[must_use]
    pub fn with_title_dropped(mut self) -> Self {
        self.config.drop_title = true;
        self
    }

    /// Set the page's path, which current-section wikilinks (`[[::path]]`)
    /// resolve against.
    #[must_use]
    pub fn with_base_path(mut self, path: impl Into<String>) -> Self {
        self.config.base_path = Some(path.into());
        self
    }

    /// Set the page's source file, as the resolvers name files.
    ///
    /// Relative markdown links and images resolve against its directory, as
    /// in `CommonMark`, once a [link resolver](Self::with_link_resolver) or
    /// [file resolver](Self::with_file_resolver) is set.
    #[must_use]
    pub fn with_source_path(mut self, path: impl Into<String>) -> Self {
        self.config.source_path = Some(path.into());
        self
    }

    /// Set the resolver that leads a link to a markdown file (`*.md`), or a
    /// wikilink, to the page published from that file.
    ///
    /// A markdown link the resolver places no page for stays as written. A
    /// wikilink with no page file keeps its url.
    #[must_use]
    pub fn with_link_resolver(mut self, resolver: impl LinkResolver + 'static) -> Self {
        self.config.link_resolver = Some(Box::new(resolver));
        self
    }

    /// Set the resolver that places the file a local image names in the
    /// project, for [`RenderResult::files`].
    ///
    /// A relative path resolves against the [source
    /// path](Self::with_source_path)'s directory, and not at all without one;
    /// a path with a leading `/` resolves against the project root, as GitHub
    /// reads a README. A path with a scheme or a host (`https:`, `data:`,
    /// `//host`) is no file. A `\` separates like `/`, as in a browser. Query
    /// and fragment are not part of the file; percent-escapes are, decoded,
    /// but an encoded `/` or `\` is refused rather than made a separator. A
    /// refused image warns. Resolving alone changes no `src`; see
    /// [`with_files_base`](Self::with_files_base) and
    /// [`with_image_src`](Self::with_image_src).
    #[must_use]
    pub fn with_file_resolver(mut self, resolver: impl FileResolver + 'static) -> Self {
        self.config.file_resolver = Some(Box::new(resolver));
        self
    }

    /// Serve resolved images from under `base`, a url prefix without a
    /// trailing slash: an image the [file resolver](Self::with_file_resolver)
    /// places at `docs/img/a b.png` gets `src="{base}/docs/img/a%20b.png"`,
    /// keeping the query and fragment it was written with. Refused and
    /// external images keep their `src`.
    #[must_use]
    pub fn with_files_base(self, base: impl Into<String>) -> Self {
        self.with_image_src(FilesBase(base.into()))
    }

    /// Set what picks the `src` of each image whose file the [file
    /// resolver](Self::with_file_resolver) places, in place of a [files
    /// base](Self::with_files_base). Refused and external images keep their
    /// `src`.
    #[must_use]
    pub fn with_image_src(mut self, image_src: impl ImageSrc + 'static) -> Self {
        self.config.image_src = Some(Box::new(image_src));
        self
    }

    /// Set the section registry for wikilink resolution and link annotation.
    ///
    /// Resolved internal links also get `data-section-ref` and
    /// `data-section-path` attributes. Without sections, wikilinks render as
    /// broken links; see the [crate-level wikilink documentation](crate#wikilinks).
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
    /// code block. So does an image alone in its paragraph whose file the
    /// [file resolver](Self::with_file_resolver) places and whose
    /// [`diagram_language`](rw_diagrams::diagram_language) `router` claims.
    ///
    /// With no router every fence is an ordinary code block, and a diagram
    /// file image its alt text, with a warning.
    #[must_use]
    pub fn with_diagram_languages(mut self, router: Arc<dyn DiagramRouter>) -> Self {
        self.config.diagram_router = Some(router);
        self
    }

    /// Render `markdown` without resolving its diagrams: each diagram fence
    /// renders as a [`RenderBackend::code_block`], and each diagram file image
    /// as a paragraph of its alt text. A caller that resolves them uses
    /// [`begin`](Self::begin).
    #[must_use]
    pub fn render(&self, markdown: &str) -> RenderResult {
        self.begin(markdown).finish(&[])
    }

    /// Walk `markdown`, reserving a hole at each diagram, and hand back the
    /// [`RenderPass`] carrying those diagrams as requests.
    ///
    /// [`RenderPass::finish`] turns the caller's resolutions into markup
    /// through the backend and assembles the page.
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

// Must stay `Send + Sync` to be shared across request handlers; checked in
// every build, not only under `cargo test`.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MarkdownRenderer<crate::HtmlBackend>>();
};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::file::resolvers::PngsInside;
    use crate::link::Pages;
    use crate::{HtmlBackend, SearchDocumentBackend};
    use rw_sections::{Namespace, Section};

    fn domain(name: &str) -> (String, Section) {
        (
            format!("domains/{name}"),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: name.to_owned(),
            },
        )
    }

    fn render_html(markdown: &str) -> RenderResult {
        MarkdownRenderer::<HtmlBackend>::new().render(markdown)
    }

    /// A renderer for the page published from `source_path`, which knows no
    /// other page.
    fn linking_from(source_path: &str) -> MarkdownRenderer<HtmlBackend> {
        MarkdownRenderer::<HtmlBackend>::new()
            .with_source_path(source_path)
            .with_link_resolver(Pages(&[]))
    }

    #[test]
    fn test_html_basic_paragraph() {
        let result = render_html("Hello, world!");
        assert_eq!(result.html, "<p>Hello, world!</p>");
    }

    /// Pages that trip pulldown-cmark's empty tight paragraph panic.
    #[test]
    fn test_html_refdef_alone_in_a_tight_item_renders() {
        assert_eq!(
            render_html("- [r]: http://x\n      \n- b\n\n[r]\n").html,
            r#"<ul><li></li><li>b</li></ul><p><a href="http://x">r</a></p>"#
        );
        assert_eq!(
            render_html("10. ![a](x.puml)\n > -    [r]:\n  x.puml\n         ").html,
            r#"<ol start="10"><li><img src="x.puml" alt="a"></li></ol><blockquote><ul><li></li></ul></blockquote>"#
        );
        assert_eq!(
            render_html(" \t:::note\n>1) [r]: x.puml 'T'\n    ").html,
            "<pre><code>:::note\n</code></pre><blockquote><ol><li></li></ol></blockquote>"
        );
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
        let result = render_html(markdown);

        assert_eq!(result.title, Some("My Title".to_owned()));
        // H1 is still rendered in HTML mode
        assert!(result.html.contains(r#"<h1 id="my-title">My Title</h1>"#));
        // ToC excludes title but includes other headings
        assert_eq!(result.toc.len(), 1);
        assert_eq!(result.toc[0].level, 2);
    }

    /// Two H1s: only the first is the title; the second renders as a normal
    /// heading with a `toc` entry.
    #[test]
    fn test_second_h1_is_not_the_title() {
        let result = render_html("# First\n\ntext\n\n# Second");

        assert_eq!(result.title.as_deref(), Some("First"));
        assert_eq!(result.toc.len(), 1, "toc: {:?}", result.toc);
        assert_eq!(result.toc[0].level, 1);
        assert_eq!(result.toc[0].title, "Second");
    }

    #[test]
    fn test_h1_after_other_content_is_not_the_title() {
        for markdown in ["Intro\n\n# Late", "## Before\n\n# Late", "> # Late"] {
            let result = render_html(markdown);

            assert_eq!(result.title, None, "{markdown:?}");
            assert!(
                result.toc.iter().any(|e| e.level == 1 && e.title == "Late"),
                "{markdown:?}: {:?}",
                result.toc
            );
        }
    }

    /// An H1 that shows no text is no title, but it still opens the page, so
    /// it is no TOC entry either.
    #[test]
    fn test_blank_opening_h1_is_neither_title_nor_toc_entry() {
        for markdown in ["# ![logo](logo.png)\n\n## A", "#\n\n## A"] {
            let result = render_html(markdown);

            assert_eq!(result.title, None, "{markdown:?}");
            assert_eq!(result.toc.len(), 1, "{markdown:?}: {:?}", result.toc);
            assert_eq!(result.toc[0].title, "A");
        }
    }

    #[test]
    fn test_h1_below_only_comments_is_the_title() {
        let result = render_html("<!-- x -->\n\n<!--\ny\n-->\n# T\n\n## A");

        assert_eq!(result.title.as_deref(), Some("T"));
        assert_eq!(result.toc.len(), 1, "toc: {:?}", result.toc);
        assert_eq!(result.toc[0].title, "A");
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
        let result = render_html("[![alt](pic.png)](https://example.com)");
        assert!(
            result
                .html
                .contains(r#"<a href="https://example.com"><img src="pic.png" alt="alt"></a>"#),
            "got: {}",
            result.html,
        );
    }

    fn render_files<B: RenderBackend>(markdown: &str) -> RenderResult {
        MarkdownRenderer::<B>::new()
            .with_source_path("guide/page.md")
            .with_file_resolver(PngsInside)
            .render(markdown)
    }

    #[test]
    fn images_anywhere_on_a_page_name_their_files() {
        let markdown = "\
# ![h](heading.png)

[![l](link.png)](https://example.com) ![r][ref]

| a |
|---|
| ![t](table.png) |

> [!NOTE]
> ![n](alert.png)

::::tabs
:::tab[A]
![tab](tab.png)
:::
::::

- ![i](item.png)
- ![again](link.png)

[ref]: reference.png
";
        let result = render_files::<HtmlBackend>(markdown);
        assert_eq!(
            Vec::from_iter(&result.files),
            [
                "source:guide/alert.png",
                "source:guide/heading.png",
                "source:guide/item.png",
                "source:guide/link.png",
                "source:guide/reference.png",
                "source:guide/tab.png",
                "source:guide/table.png",
            ],
        );
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }

    #[test]
    fn resolving_image_files_leaves_the_output_as_it_was() {
        let markdown = "\
![a](a%20b.png?v=1#x \"t\") ![b](/logo.png) ![c](../../c.png) ![d](d.bmp)
![e](https://example.com/e.png) ![f](a%2Fb.png)
";
        let plain = MarkdownRenderer::<HtmlBackend>::new()
            .with_source_path("guide/page.md")
            .render(markdown);
        let resolved = render_files::<HtmlBackend>(markdown);
        assert_eq!(resolved.html, plain.html);

        let search = render_files::<SearchDocumentBackend>(markdown);
        assert_eq!(search.files, resolved.files);
    }

    #[test]
    fn an_image_in_alt_text_names_no_file() {
        let result = render_files::<HtmlBackend>("![a ![b](inner.png)](outer.png)");
        assert_eq!(Vec::from_iter(&result.files), ["source:guide/outer.png"]);
    }

    #[test]
    fn refused_images_are_listed_apart_and_stay_in_place_among_the_warnings() {
        let result = render_files::<HtmlBackend>(
            "![a](../../a.png)\n\n:::tab[Lone]\nx\n:::\n\n![b](b.bmp) ![c](c.png) ![b](b.bmp)",
        );
        assert_eq!(
            result.warnings,
            [
                "image `../../a.png` is outside the project",
                "`:::tab` outside a `::::tabs` group; its content is rendered without tab chrome",
                "image `b.bmp` is not a supported image type",
                "image `b.bmp` is not a supported image type",
            ]
        );
        assert_eq!(
            result.image_refusals,
            [
                "image `../../a.png` is outside the project",
                "image `b.bmp` is not a supported image type",
                "image `b.bmp` is not a supported image type",
            ]
        );
    }

    #[test]
    fn resolved_images_are_served_from_the_files_base() {
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .with_source_path("guide/page.md")
            .with_file_resolver(PngsInside)
            .with_files_base("/_api/files")
            .render(
                "![a](img\\a%20b.png?v=1#x \"t\") ![b](/logo.png) ![c](../../c.png) ![d](d.bmp) \
                 ![e](https://example.com/e.png) ![f](a%2Fb.png)",
            );
        assert_eq!(
            result.html,
            "<p>\
             <img src=\"/_api/files/source:guide/img/a%20b.png?v=1#x\" title=\"t\" alt=\"a\"> \
             <img src=\"/_api/files/project:logo.png\" alt=\"b\"> \
             <img src=\"../../c.png\" alt=\"c\"> <img src=\"d.bmp\" alt=\"d\"> \
             <img src=\"https://example.com/e.png\" alt=\"e\"> <img src=\"a%2Fb.png\" alt=\"f\">\
             </p>"
        );
    }

    #[test]
    fn a_files_base_without_a_file_resolver_serves_nothing() {
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .with_source_path("guide/page.md")
            .with_files_base("/_api/files")
            .render("![a](a.png)");
        assert_eq!(result.html, r#"<p><img src="a.png" alt="a"></p>"#);
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
        let result = linking_from("base/path/index.md").render("[Link](./page.md)");
        assert!(result.html.contains(r#"href="/base/path/page""#));
    }

    #[test]
    fn links_lead_to_the_page_a_file_is_published_as() {
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .with_source_path("README.md")
            .with_link_resolver(Pages(&[("README.md", ""), ("a/README.md", "a")]))
            .render(
                "[a](README.md#intro) [b](/README?x=1) [c](README-old.md) [d](a/README.md) \
                 [e](b/README.md) [f](a/README/x.md)",
            );

        for href in [
            "/#intro",
            "/README?x=1",
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

    fn render_leaf(markdown: &str, source_path: &str) -> RenderResult {
        linking_from(source_path).render(markdown)
    }

    #[test]
    fn test_leaf_sibling_link_resolves_against_parent_dir() {
        let result = render_leaf("[x](./inbox.md)", "specs/notif.md");
        assert!(
            result.html.contains(r#"href="/specs/inbox""#),
            "Expected href=\"/specs/inbox\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_leaf_parent_link() {
        let result = render_leaf("[x](../x.md)", "specs/notif.md");
        assert!(
            result.html.contains(r#"href="/x""#),
            "Expected href=\"/x\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_leaf_subdir_link() {
        let result = render_leaf("[x](sub/y.md)", "specs/notif.md");
        assert!(
            result.html.contains(r#"href="/specs/sub/y""#),
            "Expected href=\"/specs/sub/y\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_leaf_link_with_fragment() {
        let result = render_leaf("[x](./page.md#frag)", "specs/notif.md");
        assert!(
            result.html.contains(r#"href="/specs/page#frag""#),
            "Expected href=\"/specs/page#frag\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_leaf_root_level_sibling() {
        // Leaf `guide.md` at docs root (URL `/guide`); sibling lives at root.
        let result = render_leaf("[x](./sibling.md)", "guide.md");
        assert!(
            result.html.contains(r#"href="/sibling""#),
            "Expected href=\"/sibling\", got: {}",
            result.html
        );
    }

    #[test]
    fn test_index_sibling_link_unchanged() {
        let result = linking_from("specs/notif/index.md").render("[x](./inbox.md)");
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

    /// The opener of an unrecognized container already warned about its
    /// name, so its implicit close at EOF must not warn again.
    #[test]
    fn unclosed_unknown_container_warns_only_about_its_name() {
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
        assert_eq!(
            result.warnings,
            ["unknown container directive ':::foo'"],
            "the name warns; the implicit close of a literal container adds nothing"
        );
    }

    #[test]
    fn test_wikilink_in_heading_contributes_to_toc_and_slug() {
        // Wikilink display text feeds the TOC title and the slug id too, not
        // only the rendered heading.
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

    #[test]
    fn section_ref_emits_data_attributes_on_cross_section_link() {
        let sections = Arc::new(Sections::new(HashMap::from([
            domain("billing"),
            (
                "domains/billing/systems/pay".to_owned(),
                Section {
                    kind: "system".to_owned(),
                    namespace: Namespace::default(),
                    name: "pay".to_owned(),
                },
            ),
        ])));
        let renderer = linking_from("domains/billing/systems/pay/api/index.md")
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
        let sections = Arc::new(Sections::new(HashMap::from([domain("billing")])));
        let renderer =
            linking_from("domains/billing/overview/index.md").with_sections(Arc::clone(&sections));
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
        let sections = Arc::new(Sections::new(HashMap::from([domain("billing")])));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("domains/billing".to_owned())
            .with_sections(sections);
        let result = renderer.render("[Google](https://google.com)");
        assert!(!result.html.contains("data-section-ref"));
        assert!(result.html.contains(r#"href="https://google.com""#));
    }

    #[test]
    fn section_ref_preserves_fragment() {
        let sections = Arc::new(Sections::new(HashMap::from([domain("billing")])));
        let renderer =
            linking_from("domains/search/overview/index.md").with_sections(Arc::clone(&sections));
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
        let sections = Arc::new(Sections::new(HashMap::from([domain("billing")])));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_source_path("domains/search/index.md")
            .with_link_resolver(Pages(&[("domains/billing/index.md", "domains/billing")]))
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
            HashMap::from([domain("billing")]),
            Namespace::default(),
        ));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("domains/search".to_owned())
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
        let renderer = linking_from("domains/billing/index.md");
        let result = renderer.render("[Use Cases](./use-cases.md)");
        // No sections configured — no data attributes
        assert!(!result.html.contains("data-section-ref"));
        assert!(result.html.contains(r#"href="/domains/billing/use-cases""#));
    }

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
            domain("billing"),
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
            HashMap::from([domain("billing")]),
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
    fn wikilink_display_text_opening_with_an_escape() {
        let result = render_wikilink(r"[[domain:billing::overview|\*Check\*]]");
        assert!(
            result.html.contains(">*Check*</a>"),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_escaped_pipe_is_not_part_of_the_target() {
        for md in [
            r"[[domain:billing::overview\|Docs]]",
            "| h |\n|---|\n| [[domain:billing::overview\\|Docs]] |",
        ] {
            let html = render_wikilink(md).html;
            assert!(
                html.contains(r#"<a href="/domains/billing/overview""#),
                "{md}: {html}"
            );
            assert!(html.contains(">Docs</a>"), "{md}: {html}");
        }
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
        let result = render_wikilink_with_base("[[::overview]]", "domains/billing");
        assert!(
            result.html.contains(r#"href="/domains/billing/overview""#),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_to_a_folder_page_file_leads_to_the_folder() {
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .with_wikilinks()
            .with_sections(wikilink_sections())
            .with_base_path("domains/billing")
            .with_link_resolver(Pages(&[
                ("domains/billing/api/README.md", "domains/billing/api"),
                ("domains/billing/index.md", "domains/billing"),
            ]))
            .render("[[::api/README#auth]] [[::README]] [[::index]] [[domain:billing::index?a]]");

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
        assert!(
            result.html.contains(
                r#"href="/domains/billing" data-section-ref="domain:default/billing">billing<"#
            ),
            "html: {}",
            result.html
        );
        assert!(
            result.html.contains(
                r#"href="/domains/billing?a" data-section-ref="domain:default/billing" data-section-path="?a">"#
            ),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_current_section_root() {
        let result = render_wikilink_with_base("[[::]]", "domains/billing");
        assert!(
            result.html.contains(r#"href="/domains/billing""#),
            "html: {}",
            result.html
        );
    }

    #[test]
    fn wikilink_current_section_root_with_fragment() {
        let result = render_wikilink_with_base("[[::#intro]]", "domains/billing/api");
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

    #[test]
    fn a_leading_bom_is_not_content() {
        for markdown in [
            "\u{feff}# Hello\n\nBody",
            "\u{feff}---\ntitle: T\n---\n# Hello\n\nBody",
        ] {
            let result = render_html(markdown);
            assert_eq!(
                result.html, "<h1 id=\"hello\">Hello</h1><p>Body</p>",
                "{markdown:?}"
            );
            assert_eq!(result.title.as_deref(), Some("Hello"), "{markdown:?}");
        }
    }

    #[test]
    fn a_block_below_a_comment_is_markdown_not_frontmatter() {
        let result = render_html("<!-- x -->\n---\ntitle: T\n---\n# Hello\n\nBody");
        assert_eq!(
            result.html,
            "<!-- x -->\n<hr><h2 id=\"title-t\">title: T</h2><h1 id=\"hello\">Hello</h1><p>Body</p>"
        );
        assert_eq!(result.title, None);
    }

    #[test]
    fn dashes_below_the_start_are_markdown_not_metadata() {
        let result = render_html("Intro\n\n---\nShown\n---\n\nMore");
        assert_eq!(
            result.html,
            "<p>Intro</p><hr><h2 id=\"shown\">Shown</h2><p>More</p>"
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
