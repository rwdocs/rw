//! Interpreter half of the render pipeline: turns rw
//! [`Event`]s into backend output.
//!
//! The [`Parser`](rw_parser::Parser) owns syntax, so nothing arriving here
//! needs re-scanning; the Walker owns meaning and the state that spans events.
//! A fresh `Walker` serves each render.

use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::mem;

use crate::backend::RenderBackend;
use crate::config::RenderConfig;
use crate::file;
use crate::holes::Holes;
use crate::include;
use crate::link;
use crate::renderer::RenderResult;
use crate::scope::{ImageAs, Scope};
use crate::status::{STATUS_NAME, StatusColor};
use crate::table::TableState;
use crate::tabs::{TAB_NAME, TABS_NAME, TabInfo};
use crate::toc::{HeadingAccumulator, TocEntry};
use crate::wikilink::{self, WikilinkResolution};
use rw_diagrams::{DiagramRequest, DiagramSource, diagram_language};
use rw_parser::DirectiveArgs;
use rw_parser::{BlockDirectivePayload, InlineMatch};
use rw_parser::{Event, LinkKind, LiteralCause, Parser, Tag, TagEnd};

/// What a container opener resolved to, recorded so the parser's matching close
/// renders correctly. Tab ids and labels live on the walker's `tab_open` stack.
enum ContainerOutcome {
    /// Unrecognized name: opener rendered literally; an explicit close renders
    /// its colons literally, a synthesized close renders nothing.
    Literal,
    /// The `::::tabs` group container; the close renders through the backend's
    /// tab methods.
    Group,
    /// A `:::tab` item inside an open group.
    Item,
    /// A `:::tab` with no enclosing `::::tabs` (content rendered unwrapped).
    LoneItem,
}

/// Why an `::include` line renders as written.
enum Refusal {
    /// Names no file, so the page warns once however many lines it refuses.
    General(&'static str),
    /// Names the file the line names; each such line warns.
    File(String),
}

/// An open tab group. `bar` is the hole its tab bar fills at group close,
/// once every label is known.
struct TabGroup {
    id: usize,
    tabs: Vec<TabInfo>,
    bar: usize,
}

/// Where a paragraph the parser marked `lone_image` stands.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LoneParagraph {
    /// None open, or its `<p>` is written.
    No,
    /// Its `<p>` waits for the image, which may be a diagram: a `<figure>`
    /// can't sit in a `<p>`.
    Held,
    /// Its image became a diagram, so it closes with no `</p>` either.
    Diagram,
}

/// A walked document that has not been assembled yet.
///
/// Every hole but the diagrams' is already filled.
pub(crate) struct Paused {
    /// The walk buffer: rendered markup with a gap at each reserved offset.
    pub(crate) html: String,
    /// Where each deferred fill belongs in `html`; all but the diagrams' are
    /// filled.
    pub(crate) holes: Holes,
    pub(crate) title: Option<String>,
    pub(crate) toc: Vec<TocEntry>,
    /// Warnings raised during the walk.
    pub(crate) warnings: Vec<String>,
    pub(crate) publish_warnings: Vec<String>,
    pub(crate) section_refs: BTreeSet<String>,
    pub(crate) files: BTreeSet<String>,
    pub(crate) includes: BTreeSet<String>,
    /// Every diagram fence and diagram file image, in document order. Empty
    /// unless a [`DiagramRouter`](rw_diagrams::DiagramRouter) was configured.
    pub(crate) diagrams: Vec<DiagramRequest>,
    /// Each diagram's hole and text alternative, by position in `diagrams`.
    pub(crate) diagram_holes: Vec<(usize, Option<String>)>,
}

impl Paused {
    /// Splice every fill into the walk buffer and fold the walk's state into a
    /// [`RenderResult`].
    ///
    /// `leading_warnings` go ahead of the walk's own. The order is
    /// user-visible.
    pub(crate) fn assemble<B: RenderBackend>(
        self,
        mut leading_warnings: Vec<String>,
    ) -> RenderResult {
        leading_warnings.extend(self.warnings);
        RenderResult {
            // Keep this the only step that transforms the buffer; see
            // `Walker::pause`.
            html: self.holes.assemble(self.html, B::raw_html),
            title: self.title,
            toc: self.toc,
            warnings: leading_warnings,
            publish_warnings: self.publish_warnings,
            section_refs: self.section_refs,
            files: self.files,
            includes: self.includes,
        }
    }
}

pub(crate) struct Walker<'r, B: RenderBackend> {
    cfg: &'r RenderConfig,
    /// The file relative images and `.md` links resolve from; starts as the
    /// page's source.
    source_path: Option<String>,
    warnings: Vec<String>,
    publish_warnings: Vec<String>,
    output: String,
    /// Where tab bars and diagrams go once known.
    holes: Holes,
    /// Every diagram the router claimed, in document order.
    diagrams: Vec<DiagramRequest>,
    /// Each diagram's hole and alt text. The alt stays out of the request's
    /// attrs: a provider warns about keys it does not know.
    diagram_holes: Vec<(usize, Option<String>)>,
    /// Open tab groups, innermost last: a `::::tabs` may nest in another's
    /// panel.
    tab_open: Vec<TabGroup>,
    next_group_id: usize,
    next_tab_id: usize,
    table: TableState,
    heading: HeadingAccumulator,
    /// One entry per open container directive, in nesting order. A
    /// `ContainerDirectiveEnd` with no entry here is a stray `:::`.
    container_outcomes: Vec<ContainerOutcome>,
    /// The previous heading's `(toc_text, rendered_html)` buffers, cleared for
    /// reuse; headings can't nest, so one pair suffices.
    spare_heading_buffers: Option<(String, String)>,
    /// Whether the open paragraph's `<p>` is held, or dropped, for a lone
    /// diagram image.
    lone: LoneParagraph,
    scopes: Vec<Scope>,
    /// Canonical section refs referenced by prose links in this document.
    section_refs: BTreeSet<String>,
    /// Project files referenced by local images in this document.
    files: BTreeSet<String>,
    /// Markdown files rendered in place of an `::include` line.
    includes: BTreeSet<String>,
    /// Whether the events come from an included file.
    in_include: bool,
    _backend: PhantomData<B>,
}

impl<'r, B: RenderBackend> Walker<'r, B> {
    pub(crate) fn new(cfg: &'r RenderConfig) -> Self {
        Self {
            cfg,
            source_path: cfg.source_path.clone(),
            warnings: Vec::new(),
            publish_warnings: Vec::new(),
            // Average page-sized documents fit without reallocating.
            output: String::with_capacity(4096),
            holes: Holes::default(),
            diagrams: Vec::new(),
            diagram_holes: Vec::new(),
            tab_open: Vec::new(),
            next_group_id: 0,
            next_tab_id: 0,
            table: TableState::default(),
            heading: HeadingAccumulator::new(cfg.drop_title),
            container_outcomes: Vec::new(),
            spare_heading_buffers: None,
            lone: LoneParagraph::No,
            scopes: Vec::new(),
            section_refs: BTreeSet::new(),
            files: BTreeSet::new(),
            includes: BTreeSet::new(),
            in_include: false,
            _backend: PhantomData,
        }
    }

    /// Consume the walker and hand over everything the walk produced, with the
    /// diagram holes still empty.
    ///
    /// # Do not add a step that rewrites the buffer
    ///
    /// The walk buffer is append-only until [`Paused::assemble`], including
    /// while a caller holds the [`RenderPass`](crate::RenderPass); see the
    /// Invariant section on [`holes`](crate::holes).
    pub(crate) fn pause(self) -> Paused {
        debug_assert!(
            self.scopes.is_empty(),
            "Walker::pause called with unclosed scopes (malformed event stream): {} scopes still open",
            self.scopes.len()
        );
        debug_assert!(
            self.container_outcomes.is_empty(),
            "Walker::pause called with unpaired container directives (malformed event stream): {} still open — the parser must synthesize a close for every open container at EOF",
            self.container_outcomes.len()
        );
        debug_assert!(
            self.tab_open.is_empty(),
            "Walker::pause called with a tab group still open: its bar hole would go unfilled ({} still open)",
            self.tab_open.len()
        );
        let Self {
            output: html,
            holes,
            mut heading,
            warnings,
            publish_warnings,
            section_refs,
            files,
            includes,
            diagrams,
            diagram_holes,
            ..
        } = self;

        Paused {
            html,
            holes,
            title: heading.take_title(),
            toc: heading.take_toc(),
            warnings,
            publish_warnings,
            section_refs,
            files,
            includes,
            diagrams,
            diagram_holes,
        }
    }

    /// Interpret one event.
    pub(crate) fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => {
                self.inline_code(&code);
            }
            Event::RawHtml(html) => self.raw_html(&html),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.horizontal_rule(),
            Event::TaskListMarker(checked) => self.task_list_marker(checked),
            Event::CodeBlock(mut payload) => {
                // Scopes are empty here, as `reserve_hole` requires: a code
                // block cannot occur inside a heading or alt text.
                if let Some(router) = self.cfg.diagram_router.as_deref()
                    && let Some(lang) = payload.language.as_deref()
                    && router.handles(lang)
                {
                    let slot = self.reserve_hole();
                    let mut attrs = std::mem::take(&mut payload.attrs.map);
                    self.diagram_holes.push((slot, take_alt(&mut attrs)));
                    self.diagrams.push(DiagramRequest {
                        // As authored: canonicalizing is the provider's job.
                        language: payload
                            .language
                            .take()
                            .expect("the router matched a language, so there is one"),
                        source: DiagramSource::Inline(std::mem::take(&mut payload.source)),
                        attrs,
                        id: payload.attrs.id.take(),
                    });
                } else {
                    if let Some(alt) = take_alt(&mut payload.attrs.map) {
                        B::code_block_alt(&alt, &mut self.output);
                    }
                    B::code_block(
                        payload.language.as_deref(),
                        &payload.source,
                        &mut self.output,
                    );
                }
            }
            Event::ContainerDirectiveStart {
                payload,
                colon_count,
            } => {
                if payload.name == TABS_NAME {
                    self.open_tab_group();
                } else if payload.name == TAB_NAME {
                    self.open_tab_item(&payload.args);
                } else {
                    self.warn_once(format!("unknown container directive ':::{}'", payload.name));
                    self.container_outcomes.push(ContainerOutcome::Literal);
                    self.emit_literal_line(colon_count, Some(&payload));
                }
            }
            Event::ContainerDirectiveEnd {
                name,
                colon_count,
                implicit,
            } => self.close_container(name.as_deref(), colon_count, implicit),
            Event::LeafDirective(payload) if payload.name == INCLUDE_NAME => {
                self.include(&payload);
            }
            Event::LeafDirective(payload) => {
                // `::before` or `::1` on a line of its own is likelier prose.
                if payload.is_unambiguous_leaf() {
                    self.warn_once(format!("unknown leaf directive '::{}'", payload.name));
                }
                self.emit_literal_line(2, Some(&payload));
            }
            Event::InlineDirective(payload) => {
                self.emit_inline_directive(&payload.name, &payload.args, &payload.raw);
            }
            Event::LiteralDirectiveLine { line, cause } => self.warn_literal_line(&line, cause),
        }
    }

    /// Push `warning` unless the page already has it, so an unknown name
    /// warns once however often it is used.
    fn warn_once(&mut self, warning: String) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    fn warn_literal_line(&mut self, line: &str, cause: LiteralCause) {
        let in_heading = self
            .scopes
            .iter()
            .any(|scope| matches!(scope, Scope::Heading { .. }));
        let (renders_as, fix) = match cause {
            LiteralCause::Glued if in_heading => (
                "a heading",
                "put a blank line before the `---` or `===` under it",
            ),
            LiteralCause::Glued => ("text", "put a blank line before it"),
            LiteralCause::TightItemStart => ("text", "put a blank line between the list's items"),
            LiteralCause::TextAfterName => (
                "text",
                "only a `[label]` and `{attributes}` may follow the name",
            ),
            LiteralCause::MarkupInGroup => ("text", "a directive label can't contain markup"),
        };
        self.warnings.push(format!(
            "directive line `{line}` renders as {renders_as}: {fix}"
        ));
    }

    /// Buffers for a heading's plain-text shadow and formatted HTML body.
    fn take_heading_buffers(&mut self) -> (String, String) {
        self.spare_heading_buffers
            .take()
            .unwrap_or_else(|| (String::with_capacity(64), String::with_capacity(128)))
    }

    /// Park a finished heading's spent buffers for the next heading.
    fn store_heading_buffers(&mut self, mut toc_text: String, mut rendered_html: String) {
        toc_text.clear();
        rendered_html.clear();
        self.spare_heading_buffers = Some((toc_text, rendered_html));
    }

    /// Render one inline directive: `:status`, or any other name literally as
    /// `raw`, its source slice, with a warning.
    fn emit_inline_directive(&mut self, name: &str, args: &DirectiveArgs, raw: &str) {
        if name == STATUS_NAME {
            let color = StatusColor::from(args.get("color").unwrap_or_default());
            self.with_markup_buffer(|out| B::status_open(color, out));
            self.text(args.content().trim());
            self.with_markup_buffer(B::status_close);
            return;
        }
        self.warn_once(format!("unknown inline directive ':{name}'"));
        self.text(raw);
    }

    /// Render a block directive line as an ordinary paragraph: its colons,
    /// then the rest of its line, if any.
    ///
    /// The line arrives outside any paragraph event pair, so this owes both
    /// tags. The Parser never split inline directives out of it, so they are
    /// split here.
    fn emit_literal_line(&mut self, colon_count: usize, line: Option<&BlockDirectivePayload>) {
        B::paragraph_start(&mut self.output);
        self.text(&":".repeat(colon_count));
        if let Some(line) = line {
            let rest = &line.after_colons;
            let mut from = 0;
            while let Some(InlineMatch { directive, range }) = line.inline_directive(from) {
                self.text(&rest[from..range.start]);
                self.emit_inline_directive(&directive.name, &directive.args, &rest[range.clone()]);
                from = range.end;
            }
            self.text(&rest[from..]);
        }
        B::paragraph_end(&mut self.output);
    }

    /// Render the file an `::include` line names in its place, or the line
    /// as written, with a warning, when it can't.
    fn include(&mut self, directive: &BlockDirectivePayload) {
        let (path, text) = match self.read_include(directive) {
            Ok(read) => read,
            Err(refusal) => {
                match refusal {
                    Refusal::General(warning) => {
                        let warning = warning.to_owned();
                        if !self.publish_warnings.contains(&warning) {
                            self.publish_warnings.push(warning.clone());
                        }
                        self.warn_once(warning);
                    }
                    Refusal::File(warning) => {
                        self.publish_warnings.push(warning.clone());
                        self.warnings.push(warning);
                    }
                }
                self.emit_literal_line(2, Some(directive));
                return;
            }
        };
        // The file is a document of its own: a `:::tab` in it can't join
        // a group the includer opened.
        let source_path = self.source_path.replace(path.clone());
        let tab_open = mem::take(&mut self.tab_open);
        self.in_include = true;
        let mut parser = Parser::fragment(&text, self.cfg.wikilinks, B::TOKENIZE_DIRECTIVES);
        while let Some(event) = parser.next() {
            self.handle(event);
        }
        self.in_include = false;
        self.tab_open = tab_open;
        self.source_path = source_path;
        self.includes.insert(path);
    }

    /// The project path and text of the file an `::include` line names, or
    /// why there is none.
    fn read_include(&self, directive: &BlockDirectivePayload) -> Result<(String, String), Refusal> {
        let (Some(reader), Some(source_path)) = (
            self.cfg.include_reader.as_deref(),
            self.source_path.as_deref(),
        ) else {
            return Err(Refusal::General("includes are not available here"));
        };
        if self.in_include {
            return Err(Refusal::General("nested includes are not supported yet"));
        }
        let src = directive.args.content().trim();
        if src.is_empty() {
            return Err(Refusal::General("include needs a path"));
        }
        if directive.after_colons.ends_with('}') {
            return Err(Refusal::File(format!(
                "include `{src}` takes no attributes"
            )));
        }
        if src.contains('#') {
            return Err(Refusal::File(format!(
                "include `{src}` can't name a section"
            )));
        }
        let path = include::project_path(source_path, src)
            .map_err(|e| Refusal::File(format!("include `{src}` {e}")))?;
        let text = reader
            .read(&path)
            .map_err(|e| Refusal::File(format!("include `{path}` {e}")))?;
        Ok((path, text))
    }

    /// Render a `ContainerDirectiveEnd` the parser paired: pop the outcome its
    /// opener recorded and emit the matching close.
    ///
    /// A close with no `name` is a stray `:::`: it pops nothing, even while a
    /// container is open, warns, and renders its colons literally.
    fn close_container(&mut self, name: Option<&str>, colon_count: usize, implicit: bool) {
        let outcome = name.and_then(|_| self.container_outcomes.pop());

        // A literal opener already warned about its name.
        if implicit
            && matches!(
                outcome,
                Some(ContainerOutcome::Group | ContainerOutcome::Item | ContainerOutcome::LoneItem)
            )
        {
            self.warnings.push(format!(
                "unclosed container directive :::{} (missing closing :::)",
                name.unwrap_or_default()
            ));
        }

        match outcome {
            Some(ContainerOutcome::Literal) => {
                if !implicit {
                    self.emit_literal_line(colon_count, None);
                }
            }
            Some(ContainerOutcome::Group) => {
                if let Some(group) = self.tab_open.pop() {
                    if group.tabs.is_empty() {
                        self.warnings
                            .push("`::::tabs` group has no `:::tab` items".to_owned());
                    }
                    B::tabs_close(&mut self.output);
                    // Fills go through `B::raw_html`, so a backend whose
                    // `raw_html` is a no-op drops the bar labels.
                    let mut bar = String::new();
                    B::tabs_open(group.id, &group.tabs, &mut bar);
                    self.holes.fill(group.bar, bar);
                }
            }
            Some(ContainerOutcome::Item) => {
                B::tab_panel_close(&mut self.output);
            }
            Some(ContainerOutcome::LoneItem) => {}
            None => {
                self.warnings
                    .push("stray ::: with no opening directive".to_owned());
                self.emit_literal_line(colon_count, None);
            }
        }
    }

    /// Open a `::::tabs` group, reserving its tab-bar hole before the panels.
    /// Scopes are empty here, as `reserve_hole` requires.
    fn open_tab_group(&mut self) {
        let id = self.next_group_id;
        self.next_group_id += 1;
        let bar = self.reserve_hole();
        self.tab_open.push(TabGroup {
            id,
            tabs: Vec::new(),
            bar,
        });
        self.container_outcomes.push(ContainerOutcome::Group);
    }

    /// Open a `:::tab[Label]` item. With no enclosing `::::tabs` its content
    /// renders unwrapped, with a warning.
    fn open_tab_item(&mut self, args: &DirectiveArgs) {
        let label = if args.content().is_empty() {
            "Tab".to_owned()
        } else {
            strip_quotes(args.content()).to_owned()
        };
        let Some(group) = self.tab_open.last_mut() else {
            self.warnings.push(
                "`:::tab` outside a `::::tabs` group; its content is rendered without tab chrome"
                    .to_owned(),
            );
            self.container_outcomes.push(ContainerOutcome::LoneItem);
            return;
        };
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;
        let is_first = group.tabs.is_empty();
        let group_id = group.id;
        let info = TabInfo {
            id: tab_id,
            label,
            is_first,
        };
        B::tab_panel_open(group_id, &info, &mut self.output);
        group.tabs.push(info);
        self.container_outcomes.push(ContainerOutcome::Item);
    }

    #[allow(clippy::too_many_lines)]
    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph { lone_image } => {
                if lone_image {
                    self.lone = LoneParagraph::Held;
                } else {
                    B::paragraph_start(&mut self.output);
                }
            }
            Tag::Heading { level, opens_page } => {
                let (toc_text, rendered_html) = self.take_heading_buffers();
                self.scopes.push(Scope::Heading {
                    level,
                    opens_page,
                    toc_text,
                    rendered_html,
                });
            }
            Tag::BlockQuote(Some(kind)) => {
                B::alert_start(kind, &mut self.output);
            }
            Tag::BlockQuote(None) => {
                B::blockquote_start(&mut self.output);
            }
            Tag::List(start) => {
                B::list_start(start, &mut self.output);
            }
            Tag::Item => {
                B::list_item_start(&mut self.output);
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
                self.with_markup_buffer(B::emphasis_start);
            }
            Tag::Strong => {
                self.with_markup_buffer(B::strong_start);
            }
            Tag::Strikethrough => {
                self.with_markup_buffer(B::strikethrough_start);
            }
            Tag::Link {
                kind: LinkKind::Wiki { has_pothole },
                dest_url,
            } => {
                let resolution = wikilink::resolve(self.cfg, &dest_url);
                match &resolution {
                    WikilinkResolution::Resolved {
                        href,
                        section_ref,
                        subpath,
                        ..
                    } => {
                        let section_attrs = Some((section_ref.as_str(), subpath.as_str()));
                        self.with_markup_buffer(|out| B::link_start(href, section_attrs, out));
                    }
                    WikilinkResolution::Fragment => {
                        self.with_markup_buffer(|out| B::link_start(&dest_url, None, out));
                    }
                    WikilinkResolution::Broken(reason) => {
                        let warning = format!("wikilink `[[{dest_url}]]`: {reason}");
                        self.publish_warnings.push(warning.clone());
                        self.warnings.push(warning);
                        self.with_markup_buffer(B::broken_link_start);
                    }
                }
                if !has_pothole {
                    let display = wikilink::display_text(&resolution, &dest_url);
                    self.text(&display);
                }
                if let WikilinkResolution::Resolved { section_ref, .. } = resolution {
                    self.section_refs.insert(section_ref);
                }
            }
            Tag::Link {
                kind: LinkKind::Other,
                dest_url,
            } => {
                // Alt text drops a nested link, so it leads to no file.
                let file = if matches!(self.scopes.last(), Some(Scope::Image { .. })) {
                    None
                } else {
                    link::file_link(self.cfg, self.source_path.as_deref(), &dest_url)
                };
                if let Some(Ok(file)) = file {
                    self.with_markup_buffer(|out| {
                        B::file_link_start(&file.href, file.attachment, out);
                    });
                    self.files.insert(file.path);
                    return;
                }
                if let Some(Err(warning)) = file {
                    self.publish_warnings.push(warning.clone());
                    self.warnings.push(warning);
                }
                let href = link::resolve_href(self.cfg, self.source_path.as_deref(), &dest_url);
                let section_ref = link::section_ref_attrs(self.cfg, &href);
                let section_attrs = section_ref.as_ref().map(|(r, p)| (r.as_str(), p.as_str()));
                self.with_markup_buffer(|out| B::link_start(&href, section_attrs, out));
                if let Some((r, _)) = section_ref {
                    self.section_refs.insert(r);
                }
            }
            Tag::Image { dest_url, title } => {
                // Alt text drops a nested image, so it references nothing.
                let mut served = None;
                let mut diagram = None;
                if !matches!(self.scopes.last(), Some(Scope::Image { .. })) {
                    match file::resolve(self.cfg, self.source_path.as_deref(), &dest_url) {
                        Some(Ok(path)) => {
                            if let Some(language) = diagram_language(&path) {
                                diagram = Some(self.diagram_image(language, path, &dest_url));
                            } else {
                                served =
                                    self.cfg.image_src.as_deref().and_then(|image_src| {
                                        image_src.image_src(&path, &dest_url)
                                    });
                                self.files.insert(path);
                            }
                        }
                        Some(Err(warning)) => {
                            self.publish_warnings.push(warning.clone());
                            self.warnings.push(warning);
                        }
                        None => {}
                    }
                }
                if self.lone == LoneParagraph::Held {
                    self.lone = LoneParagraph::No;
                    B::paragraph_start(&mut self.output);
                }
                let shown_as = diagram.unwrap_or_else(|| ImageAs::Image {
                    dest_url: served.unwrap_or_else(|| dest_url.to_string()),
                    title: title.to_string(),
                });
                self.scopes.push(Scope::Image {
                    alt_text: String::new(),
                    shown_as,
                });
            }
            Tag::Superscript => {
                self.with_markup_buffer(B::superscript_start);
            }
            Tag::Subscript => {
                self.with_markup_buffer(B::subscript_start);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if self.lone == LoneParagraph::Diagram {
                    self.lone = LoneParagraph::No;
                } else {
                    B::paragraph_end(&mut self.output);
                }
            }
            TagEnd::Heading => {
                let Some(Scope::Heading {
                    level,
                    opens_page,
                    toc_text,
                    rendered_html,
                }) = self
                    .scopes
                    .pop_if(|scope| matches!(scope, Scope::Heading { .. }))
                else {
                    debug_assert!(false, "TagEnd::Heading without matching Scope::Heading");
                    return;
                };
                if let Some(done) = self.heading.complete_heading(level, opens_page, &toc_text) {
                    B::heading_start(done.adjusted_level, &done.id, &mut self.output);
                    self.output.push_str(rendered_html.trim());
                    B::heading_end(done.adjusted_level, &mut self.output);
                }
                self.store_heading_buffers(toc_text, rendered_html);
            }
            TagEnd::BlockQuote(Some(_)) => {
                B::alert_end(&mut self.output);
            }
            TagEnd::BlockQuote(None) => {
                B::blockquote_end(&mut self.output);
            }
            TagEnd::List(ordered) => {
                B::list_end(ordered, &mut self.output);
            }
            TagEnd::Item => {
                B::list_item_end(&mut self.output);
            }
            TagEnd::Image => {
                let Some(Scope::Image { alt_text, shown_as }) = self
                    .scopes
                    .pop_if(|scope| matches!(scope, Scope::Image { .. }))
                else {
                    debug_assert!(false, "TagEnd::Image without matching Scope::Image");
                    return;
                };
                // Pop before emitting: the image's own scope would swallow it.
                match shown_as {
                    ImageAs::Image { dest_url, title } => {
                        self.with_markup_buffer(|out| B::image(&dest_url, &alt_text, &title, out));
                    }
                    ImageAs::Diagram { language, path } => {
                        // Top level, as `reserve_hole` requires: a paragraph
                        // is never inside a heading or alt text.
                        let slot = self.reserve_hole();
                        self.diagram_holes
                            .push((slot, Some(alt_text).filter(|alt| !alt.is_empty())));
                        self.diagrams.push(DiagramRequest {
                            language: language.to_owned(),
                            source: DiagramSource::File(path),
                            attrs: Vec::new(),
                            id: None,
                        });
                    }
                    ImageAs::AltText => self.text(&alt_text),
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
                self.with_markup_buffer(B::emphasis_end);
            }
            TagEnd::Strong => {
                self.with_markup_buffer(B::strong_end);
            }
            TagEnd::Strikethrough => {
                self.with_markup_buffer(B::strikethrough_end);
            }
            TagEnd::Link => {
                self.with_markup_buffer(B::link_end);
            }
            TagEnd::Superscript => {
                self.with_markup_buffer(B::superscript_end);
            }
            TagEnd::Subscript => {
                self.with_markup_buffer(B::subscript_end);
            }
        }
    }

    /// Route text: `s` feeds the active scope's plain channel (a heading's
    /// `toc_text`, an image's `alt_text`), and `write` emits its markup form
    /// wherever markup goes.
    fn plain_text(&mut self, s: &str, write: impl Fn(&str, &mut String)) {
        match self.scopes.last_mut() {
            Some(Scope::Heading {
                rendered_html,
                toc_text,
                ..
            }) => {
                toc_text.push_str(s);
                write(s, rendered_html);
            }
            Some(Scope::Image { alt_text, .. }) => alt_text.push_str(s),
            None => write(s, &mut self.output),
        }
    }

    fn text(&mut self, text: &str) {
        self.plain_text(text, B::text);
    }

    fn inline_code(&mut self, code: &str) {
        self.plain_text(code, B::inline_code);
    }

    fn raw_html(&mut self, html: &str) {
        self.with_markup_buffer(|out| B::raw_html(html, out));
    }

    /// What an image naming a diagram file renders as. It must be alone in its
    /// paragraph, which then becomes the diagram; without a router that
    /// renders `language` it is its alt text.
    fn diagram_image(&mut self, language: &'static str, path: String, src: &str) -> ImageAs {
        if self.lone != LoneParagraph::Held {
            // A publish warning too: a publish renders without a router and
            // must still report it.
            let warning = format!(
                "diagram `{src}` must be alone in its paragraph: put it on its own line with a blank line before and after"
            );
            self.publish_warnings.push(warning.clone());
            self.warnings.push(warning);
            return ImageAs::AltText;
        }
        if self
            .cfg
            .diagram_router
            .as_deref()
            .is_some_and(|router| router.handles(language))
        {
            self.lone = LoneParagraph::Diagram;
            ImageAs::Diagram { language, path }
        } else {
            // Not a publish warning: a publish's router-less render embeds it later.
            self.warnings.push(format!(
                "diagram `{src}` not rendered: no diagram renderer for {language} is configured"
            ));
            ImageAs::AltText
        }
    }

    /// Reserve a hole at the current output position, returning its slot.
    ///
    /// Only valid at top level: inside a scope, output goes to that scope's
    /// own buffer, so the offset would name the wrong string.
    fn reserve_hole(&mut self) -> usize {
        debug_assert!(
            self.scopes.is_empty(),
            "hole reserved inside a scope: the offset would reference the wrong buffer"
        );
        self.holes.reserve(self.output.len())
    }

    /// Route a line break: markup through `write`, or a single space inside
    /// alt text.
    fn line_break(&mut self, write: impl Fn(&mut String)) {
        match self.scopes.last_mut() {
            Some(Scope::Heading { rendered_html, .. }) => write(rendered_html),
            Some(Scope::Image { alt_text, .. }) => alt_text.push(' '),
            None => write(&mut self.output),
        }
    }

    fn soft_break(&mut self) {
        self.line_break(B::soft_break);
    }

    fn hard_break(&mut self) {
        self.line_break(B::hard_break);
    }

    /// Run `f` against the active scope's markup buffer: a heading's rendered
    /// HTML, `self.output` at top level, or nothing inside alt text.
    ///
    /// Keep this the only path from inline markup to a `&mut String`: an
    /// accessor could forget the alt-text case and leak markup into `alt`.
    fn with_markup_buffer(&mut self, f: impl FnOnce(&mut String)) {
        match self.scopes.last_mut() {
            Some(Scope::Heading { rendered_html, .. }) => f(rendered_html),
            Some(Scope::Image { .. }) => {}
            None => f(&mut self.output),
        }
    }

    fn horizontal_rule(&mut self) {
        B::horizontal_rule(&mut self.output);
    }

    fn task_list_marker(&mut self, checked: bool) {
        B::task_list_marker(checked, &mut self.output);
    }
}

const INCLUDE_NAME: &str = "include";

/// Strip a single pair of surrounding quotes (single or double) from a tab
/// label. `:::tab["macOS"]` displays as `macOS`.
fn strip_quotes(s: &str) -> &str {
    let is_quoted =
        (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\''));
    if is_quoted && s.len() >= 2 {
        return &s[1..s.len() - 1];
    }
    s
}

/// Remove a fence's `alt` attribute, returning its text. An empty `alt` is
/// none: a diagram is never decorative.
fn take_alt(attrs: &mut Vec<(String, String)>) -> Option<String> {
    // `Attrs` keeps one entry per key, so there is no second `alt` left.
    let pos = attrs.iter().position(|(key, _)| key == "alt")?;
    Some(attrs.remove(pos).1).filter(|alt| !alt.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::HtmlBackend;

    fn cfg() -> RenderConfig {
        RenderConfig::default()
    }

    #[test]
    fn pause_returns_empty_result_when_no_events_processed() {
        let config = cfg();
        let walker = Walker::<HtmlBackend>::new(&config);
        let result = walker.pause().assemble::<HtmlBackend>(Vec::new());
        assert_eq!(result.html, "");
        assert!(result.title.is_none());
        assert_eq!(result.toc, [] as [crate::toc::TocEntry; 0]);
        assert_eq!(result.warnings, [] as [String; 0]);
    }
}
