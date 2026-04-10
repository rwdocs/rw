//! Generic markdown renderer with pluggable backend.
//!
//! See the [crate-level documentation](crate) for an overview and examples.

use std::borrow::Cow;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use rw_sections::Sections;

use crate::backend::{AlertKind, RenderBackend};
use crate::code_block::{CodeBlockProcessor, ProcessResult, parse_fence_info};
use crate::directive::DirectiveProcessor;
use crate::state::{CodeBlockState, HeadingState, ImageState, TableState, TocEntry};
use crate::util::heading_level_to_num;

/// Output produced by [`MarkdownRenderer::render`] or [`MarkdownRenderer::render_markdown`].
///
/// Contains the rendered markup, an optional page title extracted from the
/// first H1 heading, table-of-contents entries for heading navigation, and
/// any warnings emitted by code block processors or directives.
///
/// # Examples
///
/// ```
/// use rw_renderer::{MarkdownRenderer, HtmlBackend};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .render_markdown("# Welcome\n\nHello **world**.");
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

/// Resolves page paths to their display titles for wikilink rendering.
///
/// When a wikilink like `[[domain:billing::overview]]` has no explicit display
/// text, the renderer calls this trait to look up a human-readable title.
/// If the resolver returns `None`, the renderer falls back to the last path
/// segment.
///
/// # Examples
///
/// ```
/// use rw_renderer::TitleResolver;
///
/// struct MapResolver(std::collections::HashMap<String, String>);
///
/// impl TitleResolver for MapResolver {
///     fn resolve_title(&self, path: &str) -> Option<String> {
///         self.0.get(path).cloned()
///     }
/// }
///
/// let mut titles = std::collections::HashMap::new();
/// titles.insert("domains/billing/overview".into(), "Billing Overview".into());
/// let resolver = MapResolver(titles);
///
/// assert_eq!(
///     resolver.resolve_title("domains/billing/overview"),
///     Some("Billing Overview".into()),
/// );
/// assert_eq!(resolver.resolve_title("unknown/page"), None);
/// ```
pub trait TitleResolver {
    /// Returns the display title for a page at `path`, or `None` if unknown.
    ///
    /// `path` is an absolute path without leading slash
    /// (e.g., `"domains/billing/overview"`).
    fn resolve_title(&self, path: &str) -> Option<String>;
}

/// Result of resolving a wikilink target.
enum WikilinkResolution {
    /// Successfully resolved to a concrete href with section metadata.
    Resolved {
        href: String,
        section_ref: String,
        section_name: String,
        subpath: String,
    },
    /// Fragment-only link (`#heading`) — same page, no section resolution.
    Fragment(String),
    /// Target could not be resolved — render as broken link.
    Broken { raw_target: String },
}

/// Generic markdown renderer with pluggable backend.
///
/// Walks pulldown-cmark events and produces HTML or XHTML depending on the
/// [`RenderBackend`] implementation (`B`). Common elements (tables, lists,
/// inline formatting) are handled generically; format-specific elements are
/// delegated to `B`.
///
/// The two main entry points are:
///
/// - [`render_markdown`](Self::render_markdown) — accepts raw markdown,
///   handles directive pre/post-processing automatically.
/// - [`render`](Self::render) — accepts a pre-built pulldown-cmark event
///   iterator (skips directive processing).
///
/// # Code block processors
///
/// Register processors via [`with_processor`](Self::with_processor).
/// Processors are checked in registration order; the first returning a
/// non-[`PassThrough`](ProcessResult::PassThrough) result wins.
///
/// # Directive processing
///
/// Configure via [`with_directives`](Self::with_directives). When set,
/// [`render_markdown`](Self::render_markdown) runs a three-phase pipeline:
/// preprocess directives → parse and render with pulldown-cmark → post-process
/// intermediate elements.
///
/// # Examples
///
/// ```
/// use rw_renderer::{MarkdownRenderer, HtmlBackend};
///
/// let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .with_base_path("/docs/guide");
///
/// let result = renderer.render_markdown("# Guide\n\nSee [setup](setup.md).");
/// assert_eq!(result.title.as_deref(), Some("Guide"));
/// assert!(result.html.contains(r#"href="/docs/guide/setup""#));
/// ```
#[allow(clippy::struct_excessive_bools)]
pub struct MarkdownRenderer<B: RenderBackend> {
    output: String,
    list_stack: Vec<bool>,
    code: CodeBlockState,
    table: TableState,
    image: ImageState,
    heading: HeadingState,
    base_path: Option<String>,
    /// Origin prefix (with trailing slash) for files outside `source_dir` (e.g., `"docs/"`).
    /// When set, relative links starting with this prefix have it stripped
    /// before resolution, so `docs/guide.md` resolves to `/guide` instead of `/docs/guide`.
    origin_prefix: Option<String>,
    pending_image: Option<(String, String)>,
    processors: Vec<Box<dyn CodeBlockProcessor>>,
    code_block_index: usize,
    pending_attrs: HashMap<String, String>,
    gfm: bool,
    /// Stack of alert kinds for nested blockquotes (regular blockquote uses None).
    alert_stack: Vec<Option<AlertKind>>,
    /// Optional directive processor for `CommonMark` directives.
    directives: Option<DirectiveProcessor>,
    /// Sections for annotating internal links.
    /// Shared via Arc because the map can be large (~500 entries) and is reused across renders.
    sections: Option<Arc<Sections>>,
    /// Whether we are currently inside a YAML metadata block (frontmatter).
    in_metadata_block: bool,
    /// Whether wikilink parsing and resolution is enabled.
    wikilinks: bool,
    /// Title resolver for wikilink display text.
    title_resolver: Option<Box<dyn TitleResolver>>,
    /// When true, skip the next `Event::Text`.
    ///
    /// pulldown-cmark emits a `Text` event containing the raw wikilink target
    /// after the `WikiLink` tag start. We suppress it because we render our own
    /// resolved display text in `start_tag` instead.
    skip_wikilink_text: bool,
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
            origin_prefix: None,
            pending_image: None,
            processors: Vec::new(),
            code_block_index: 0,
            pending_attrs: HashMap::new(),
            gfm: true,
            alert_stack: Vec::new(),
            directives: None,
            sections: None,
            in_metadata_block: false,
            wikilinks: false,
            title_resolver: None,
            skip_wikilink_text: false,
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

    /// Set base path for resolving relative links (URL path with leading `/`).
    ///
    /// Only used by HTML backend. Confluence backend ignores this.
    #[must_use]
    pub fn with_base_path(mut self, path: impl Into<String>) -> Self {
        self.base_path = Some(path.into());
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
        self.origin_prefix = Some(prefix);
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
            self.sections = None;
        } else {
            self.sections = Some(sections);
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
        self.wikilinks = enabled;
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
        self.title_resolver = Some(Box::new(resolver));
        self
    }

    /// Returns pulldown-cmark [`Options`] reflecting the current GFM and
    /// wikilink configuration.
    ///
    /// Useful when constructing a parser manually for [`render`](Self::render).
    #[must_use]
    pub fn parser_options(&self) -> Options {
        let mut opts = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
        if self.gfm {
            opts |= Options::ENABLE_TABLES
                | Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_GFM;
        }
        if self.wikilinks {
            opts |= Options::ENABLE_WIKILINKS;
        }
        opts
    }

    /// Creates a pulldown-cmark [`Parser`] with the renderer's current options.
    ///
    /// Use this with [`render`](Self::render) when you need a pre-built event
    /// iterator (e.g., to inspect or filter events before rendering).
    #[must_use]
    pub fn create_parser<'a>(&self, markdown: &'a str) -> Parser<'a> {
        Parser::new_ext(markdown, self.parser_options())
    }

    /// Renders raw markdown to HTML, handling directives automatically.
    ///
    /// This is the primary entry point. It runs the full pipeline:
    ///
    /// 1. **Preprocess** — expands directives (if configured via
    ///    [`with_directives`](Self::with_directives))
    /// 2. **Parse & render** — feeds the markdown through pulldown-cmark and
    ///    the backend
    /// 3. **Post-process** — transforms intermediate directive elements and
    ///    replaces code block placeholders
    ///
    /// Use [`render`](Self::render) instead when you already have a
    /// pulldown-cmark event iterator or need to skip directive processing.
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

    /// Get the appropriate output buffer for inline content.
    ///
    /// Returns the heading HTML buffer when inside a heading, otherwise the
    /// main output buffer. Use this when calling backend methods that need
    /// a `&mut String` target.
    fn inline_out(&mut self) -> &mut String {
        if self.heading.is_active() {
            self.heading.html_buffer()
        } else {
            &mut self.output
        }
    }

    /// Build ref data attributes for a resolved path, if applicable.
    ///
    /// Returns `None` for:
    /// - External or relative links (not starting with `/`)
    /// - Links not matching any section
    ///
    /// Returns `Some((section_ref_string, section_path))` for internal links matching a section.
    fn section_ref_attrs(&self, href: &str) -> Option<(String, String)> {
        if !href.starts_with('/') {
            return None;
        }
        let sp = self.sections.as_ref()?.find(href)?;
        Some((sp.section.to_string(), sp.path.to_owned()))
    }

    /// Strip the origin prefix from a URL if it matches.
    ///
    /// For files outside `source_dir` (e.g., README.md at the project root),
    /// relative links like `docs/guide.md` include the source directory name.
    /// This strips that prefix so the link resolves correctly in URL space.
    fn strip_origin<'a>(&self, url: &'a str) -> Cow<'a, str> {
        if let Some(prefix) = &self.origin_prefix
            && let Some(stripped) = url.strip_prefix(prefix.as_str())
        {
            return Cow::Owned(stripped.to_owned());
        }
        Cow::Borrowed(url)
    }

    /// Resolve a wikilink `dest_url` to a `WikilinkResolution`.
    fn resolve_wikilink(&self, dest_url: &str) -> WikilinkResolution {
        if let Some(fragment) = dest_url.strip_prefix('#') {
            return WikilinkResolution::Fragment(fragment.to_owned());
        }

        let resolved = self
            .sections
            .as_ref()
            .and_then(|s| s.resolve_refpath(dest_url, self.base_path.as_deref()));

        match resolved {
            Some((href, sp)) => WikilinkResolution::Resolved {
                href,
                section_ref: sp.section.to_string(),
                section_name: sp.section.name.clone(),
                subpath: sp.path.to_owned(),
            },
            None => WikilinkResolution::Broken {
                raw_target: dest_url.to_owned(),
            },
        }
    }

    /// Get display text for a resolved wikilink.
    fn wikilink_display_text(&self, resolution: &WikilinkResolution) -> String {
        match resolution {
            WikilinkResolution::Broken { raw_target } => raw_target.clone(),
            WikilinkResolution::Fragment(fragment) => fragment.replace('-', " "),
            WikilinkResolution::Resolved {
                href,
                subpath,
                section_name,
                ..
            } => {
                if let Some(resolver) = &self.title_resolver {
                    let path = href.strip_prefix('/').unwrap_or(href);
                    let path = match path.find('#') {
                        Some(pos) => &path[..pos],
                        None => path,
                    };
                    if let Some(title) = resolver.resolve_title(path) {
                        return title;
                    }
                }

                if !subpath.is_empty() {
                    // unwrap: rsplit always yields at least one element
                    return subpath.rsplit('/').next().unwrap().to_owned();
                }

                if !section_name.is_empty() {
                    return section_name.clone();
                }

                href.clone()
            }
        }
    }

    /// Renders pre-parsed pulldown-cmark events to the configured backend.
    ///
    /// Prefer [`render_markdown`](Self::render_markdown) for most use cases.
    /// This method is useful when you need to construct the parser yourself
    /// (e.g., with custom options) or when directive preprocessing is not needed.
    ///
    /// Automatically calls [`CodeBlockProcessor::post_process`] on all
    /// registered processors to replace placeholders with rendered content.
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
            Event::Text(text) => {
                if self.skip_wikilink_text {
                    self.skip_wikilink_text = false;
                    return;
                }
                if !self.in_metadata_block {
                    self.text(&text);
                }
            }
            Event::Code(code) => {
                if !self.in_metadata_block {
                    self.inline_code(&code);
                }
            }
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
                    B::paragraph_start(&mut self.output);
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
                B::list_start(start.is_some(), start, &mut self.output);
            }
            Tag::Item => {
                B::list_item_start(&mut self.output);
            }
            Tag::FootnoteDefinition(_) | Tag::HtmlBlock => {}
            Tag::MetadataBlock(_) => {
                self.in_metadata_block = true;
            }
            Tag::DefinitionList => {
                B::definition_list_start(&mut self.output);
            }
            Tag::DefinitionListTitle => {
                B::definition_title_start(&mut self.output);
            }
            Tag::DefinitionListDefinition => {
                B::definition_detail_start(&mut self.output);
            }
            Tag::Table(alignments) => {
                self.table.start(alignments);
                B::table_start(&mut self.output);
            }
            Tag::TableHead => {
                self.table.start_head();
                B::table_head_start(&mut self.output);
            }
            Tag::TableRow => {
                self.table.start_row();
                B::table_row_start(&mut self.output);
            }
            Tag::TableCell => {
                let alignment = self.table.current_alignment();
                let is_head = self.table.is_in_head();
                B::table_cell_start(is_head, alignment, &mut self.output);
            }
            Tag::Emphasis => {
                let out = self.inline_out();
                B::emphasis_start(out);
            }
            Tag::Strong => {
                let out = self.inline_out();
                B::strong_start(out);
            }
            Tag::Strikethrough => {
                let out = self.inline_out();
                B::strikethrough_start(out);
            }
            Tag::Link {
                link_type: LinkType::WikiLink { has_pothole },
                dest_url,
                ..
            } if self.wikilinks => {
                let resolution = self.resolve_wikilink(&dest_url);
                match &resolution {
                    WikilinkResolution::Resolved {
                        href,
                        section_ref,
                        subpath,
                        ..
                    } => {
                        let section_attrs = if section_ref.is_empty() {
                            None
                        } else {
                            Some((section_ref.as_str(), subpath.as_str()))
                        };
                        let out = self.inline_out();
                        B::link_start(href, section_attrs, out);
                    }
                    WikilinkResolution::Fragment(fragment) => {
                        let href = format!("#{fragment}");
                        let out = self.inline_out();
                        B::link_start(&href, None, out);
                    }
                    WikilinkResolution::Broken { .. } => {
                        let out = self.inline_out();
                        B::broken_link_start(out);
                    }
                }
                if !has_pothole {
                    let display = self.wikilink_display_text(&resolution);
                    self.skip_wikilink_text = true;
                    let out = self.inline_out();
                    B::text(&display, out);
                }
            }
            Tag::Link { dest_url, .. } => {
                let dest_url = self.strip_origin(&dest_url);
                let href = B::transform_link(&dest_url, self.base_path.as_deref());
                let section_ref = self.section_ref_attrs(&href);
                let section_attrs = section_ref.as_ref().map(|(r, p)| (r.as_str(), p.as_str()));
                let out = self.inline_out();
                B::link_start(&href, section_attrs, out);
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                // Start collecting alt text; image will be rendered in end_tag
                self.image.start();
                let dest_url = self.strip_origin(&dest_url);
                self.pending_image = Some((dest_url.into_owned(), title.to_string()));
            }
            Tag::Superscript => {
                let out = self.inline_out();
                B::superscript_start(out);
            }
            Tag::Subscript => {
                let out = self.inline_out();
                B::subscript_start(out);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if !self.code.is_active() {
                    B::paragraph_end(&mut self.output);
                }
            }
            TagEnd::Heading(_level) => {
                if self.heading.is_in_first_h1() {
                    self.heading.complete_first_h1();
                } else if let Some((level, id, _text, html)) = self.heading.complete_heading() {
                    B::heading_start(level, &id, &mut self.output);
                    self.output.push_str(html.trim());
                    B::heading_end(level, &mut self.output);
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
                B::list_end(ordered, &mut self.output);
            }
            TagEnd::Item => {
                B::list_item_end(&mut self.output);
            }
            TagEnd::FootnoteDefinition | TagEnd::HtmlBlock => {}
            TagEnd::MetadataBlock(_) => {
                self.in_metadata_block = false;
            }
            TagEnd::Image => {
                // Render image with collected alt text
                let alt = self.image.end();
                if let Some((src, title)) = self.pending_image.take() {
                    B::image(&src, &alt, &title, &mut self.output);
                }
            }
            TagEnd::DefinitionList => {
                B::definition_list_end(&mut self.output);
            }
            TagEnd::DefinitionListTitle => {
                B::definition_title_end(&mut self.output);
            }
            TagEnd::DefinitionListDefinition => {
                B::definition_detail_end(&mut self.output);
            }
            TagEnd::Table => {
                B::table_end(&mut self.output);
            }
            TagEnd::TableHead => {
                B::table_head_end(&mut self.output);
                self.table.end_head();
            }
            TagEnd::TableRow => {
                B::table_row_end(&mut self.output);
            }
            TagEnd::TableCell => {
                B::table_cell_end(self.table.is_in_head(), &mut self.output);
                self.table.next_cell();
            }
            TagEnd::Emphasis => {
                let out = self.inline_out();
                B::emphasis_end(out);
            }
            TagEnd::Strong => {
                let out = self.inline_out();
                B::strong_end(out);
            }
            TagEnd::Strikethrough => {
                let out = self.inline_out();
                B::strikethrough_end(out);
            }
            TagEnd::Link => {
                let out = self.inline_out();
                B::link_end(out);
            }
            TagEnd::Superscript => {
                let out = self.inline_out();
                B::superscript_end(out);
            }
            TagEnd::Subscript => {
                let out = self.inline_out();
                B::subscript_end(out);
            }
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
            B::text(text, self.heading.html_buffer());
        } else {
            B::text(text, &mut self.output);
        }
    }

    fn inline_code(&mut self, code: &str) {
        if self.heading.is_active() {
            self.heading.push_text(code);
            B::inline_code(code, self.heading.html_buffer());
        } else {
            B::inline_code(code, &mut self.output);
        }
    }

    fn raw_html(&mut self, html: &str) {
        if self.heading.is_active() {
            B::raw_html(html, self.heading.html_buffer());
        } else {
            B::raw_html(html, &mut self.output);
        }
    }

    fn soft_break(&mut self) {
        if self.code.is_active() {
            self.code.push_newline();
        } else if self.heading.is_active() {
            B::soft_break(self.heading.html_buffer());
        } else {
            B::soft_break(&mut self.output);
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
    use std::collections::HashMap;

    use super::*;
    use crate::HtmlBackend;
    use crate::code_block::ExtractedCodeBlock;
    use pulldown_cmark::{Options, Parser};
    use rw_sections::Section;

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

    fn render_with_origin(markdown: &str, base_path: &str, origin: &str) -> RenderResult {
        let options = Options::ENABLE_TABLES;
        let parser = Parser::new_ext(markdown, options);
        MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path(base_path)
            .with_origin(origin)
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
                DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
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

    // section_ref integration tests

    #[test]
    fn section_ref_emits_data_attributes_on_cross_section_link() {
        let sections = Arc::new(Sections::new(HashMap::from([
            (
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    name: "billing".to_owned(),
                },
            ),
            (
                "domains/billing/systems/pay".to_owned(),
                Section {
                    kind: "system".to_owned(),
                    name: "pay".to_owned(),
                },
            ),
        ])));
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/billing/systems/pay/api".to_owned())
            .with_sections(Arc::clone(&sections));
        let result = renderer.render_markdown("[Billing](../../../overview.md)");
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
                name: "billing".to_owned(),
            },
        )])));
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/billing/overview".to_owned())
            .with_sections(Arc::clone(&sections));
        let result = renderer.render_markdown("[Use Cases](./use-cases.md)");
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
                name: "billing".to_owned(),
            },
        )])));
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/billing".to_owned())
            .with_sections(sections);
        let result = renderer.render_markdown("[Google](https://google.com)");
        assert!(!result.html.contains("data-section-ref"));
        assert!(result.html.contains(r#"href="https://google.com""#));
    }

    #[test]
    fn section_ref_preserves_fragment() {
        let sections = Arc::new(Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                name: "billing".to_owned(),
            },
        )])));
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/search/overview".to_owned())
            .with_sections(Arc::clone(&sections));
        let result = renderer.render_markdown("[Billing API](../../billing/api.md#endpoints)");
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
                name: "billing".to_owned(),
            },
        )])));
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_base_path("/domains/search".to_owned())
            .with_sections(Arc::clone(&sections));
        let result = renderer.render_markdown("[Billing](../billing/index.md)");
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
        let mut renderer =
            MarkdownRenderer::<HtmlBackend>::new().with_base_path("/domains/billing".to_owned());
        let result = renderer.render_markdown("[Use Cases](./use-cases.md)");
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
        use rw_sections::Section;
        Arc::new(Sections::new(HashMap::from([
            (
                String::new(),
                Section {
                    kind: "section".to_owned(),
                    name: "root".to_owned(),
                },
            ),
            (
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    name: "billing".to_owned(),
                },
            ),
        ])))
    }

    fn render_wikilink(markdown: &str) -> RenderResult {
        let options = Options::ENABLE_WIKILINKS | Options::ENABLE_TABLES;
        let parser = Parser::new_ext(markdown, options);
        MarkdownRenderer::<HtmlBackend>::new()
            .with_wikilinks(true)
            .with_sections(wikilink_sections())
            .with_title_resolver(StaticTitleResolver)
            .render(parser)
    }

    fn render_wikilink_with_base(markdown: &str, base: &str) -> RenderResult {
        let options = Options::ENABLE_WIKILINKS | Options::ENABLE_TABLES;
        let parser = Parser::new_ext(markdown, options);
        MarkdownRenderer::<HtmlBackend>::new()
            .with_wikilinks(true)
            .with_sections(wikilink_sections())
            .with_base_path(base)
            .with_title_resolver(StaticTitleResolver)
            .render(parser)
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
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new();
        let result = renderer.render_markdown(markdown);
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
}
