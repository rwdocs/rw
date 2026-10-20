//! Interpreter half of the render pipeline: turns rw
//! [`Event`]s into backend output.
//!
//! The boundary with [`Parser`](rw_parser::Parser) is syntax versus
//! meaning. The Parser tokenizes: it coalesces text runs, recognizes directive
//! syntax, accumulates fenced code blocks, and swallows metadata blocks, so
//! nothing arriving here needs re-scanning. The Walker interprets what arrives
//! — it is the half that holds the section index and the backend, and it owns
//! the state that spans events (open containers, the heading accumulator, the
//! code-block counter, the hole table, and any warnings raised during the
//! walk).
//!
//! `Walker` is constructed fresh inside every call to
//! [`MarkdownRenderer::render`](crate::MarkdownRenderer::render) and
//! [`MarkdownRenderer::begin`](crate::MarkdownRenderer::begin), borrowing the
//! façade's `RenderConfig`, so per-render state cannot leak across renders.

use std::collections::BTreeSet;
use std::marker::PhantomData;

use crate::backend::RenderBackend;
use crate::config::RenderConfig;
use crate::file;
use crate::fills::{GlobalFills, GlobalKey, Source, hole_key};
use crate::holes::Holes;
use crate::link;
use crate::renderer::RenderResult;
use crate::scope::Scope;
use crate::status::{STATUS_NAME, StatusColor};
use crate::table::TableState;
use crate::tabs::{TAB_NAME, TABS_NAME, TabInfo};
use crate::toc::{HeadingAccumulator, TocEntry};
use crate::wikilink::{self, WikilinkResolution};
use rw_diagrams::{DiagramRequest, DiagramSource, RequestKey};
use rw_parser::DirectiveArgs;
use rw_parser::{BlockDirectivePayload, InlineMatch};
use rw_parser::{Event, LinkKind, LiteralCause, Tag, TagEnd};

/// What a container opener resolved to, recorded so the parser's matching close
/// renders correctly.
///
/// The parser emits a balanced, well-nested
/// `ContainerDirectiveStart`/`ContainerDirectiveEnd` stream (closing unclosed
/// containers at block and EOF boundaries itself), so pairing lives here: the
/// walker keeps one of these per open container. Tab metadata (ids, labels)
/// lives on the walker's `tab_open` stack, not here.
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

/// A tab group the walker is rendering as a built-in: its id and the tabs seen
/// so far. The bar hole reserved at its open is keyed by the id itself
/// (narrowed via [`hole_key`]; `Source::Tabs` keeps it disjoint from diagram
/// keys), and its accessible `<div role="tablist">` markup fills that hole at
/// group close, once every label is known.
struct TabGroup {
    id: usize,
    tabs: Vec<TabInfo>,
}

/// A walked document that has not been assembled yet.
///
/// [`Walker::pause`] produces one with every hole reserved and every fill but
/// the diagrams' already collected. Assembly is deliberately not part of the
/// walk: it is what lets a caller resolve diagrams — over the network, from a
/// cache, not at all — between the two halves, with no I/O inside the renderer.
pub(crate) struct Paused {
    /// The walk buffer: rendered markup with a gap at each reserved offset.
    pub(crate) html: String,
    /// Where each deferred fill belongs in `html`.
    pub(crate) holes: Holes,
    /// Content for those holes, less the diagrams'.
    pub(crate) fills: GlobalFills,
    pub(crate) title: Option<String>,
    pub(crate) toc: Vec<TocEntry>,
    /// Warnings raised during the walk.
    pub(crate) warnings: Vec<String>,
    pub(crate) section_refs: BTreeSet<String>,
    pub(crate) files: BTreeSet<String>,
    /// Every diagram fence, in document order. Empty unless a
    /// [`DiagramRouter`](rw_diagrams::DiagramRouter) was configured.
    pub(crate) diagrams: Vec<DiagramRequest>,
}

impl Paused {
    /// Splice every fill into the walk buffer and fold the walk's state into a
    /// [`RenderResult`].
    ///
    /// `leading_warnings` go ahead of the walk's own, which is where
    /// [`RenderPass::finish`](crate::RenderPass::finish) puts its diagram
    /// warnings: a diagram's warning precedes every directive warning on the
    /// page, wherever the two sit relative to each other in the source.
    /// `--strict` publishing prints the list verbatim, so the order is
    /// user-visible.
    pub(crate) fn assemble<B: RenderBackend>(
        self,
        mut leading_warnings: Vec<String>,
    ) -> RenderResult {
        leading_warnings.extend(self.warnings);
        RenderResult {
            // Fills are markup the backend never saw during the walk, so they
            // go in through `raw_html` like every other emission. `assemble`
            // returns the buffer untouched when no hole was reserved. Keep this
            // the only step that transforms it — see `Walker::pause`'s docs.
            html: self.holes.assemble(self.html, &self.fills, B::raw_html),
            title: self.title,
            toc: self.toc,
            warnings: leading_warnings,
            section_refs: self.section_refs,
            files: self.files,
        }
    }
}

pub(crate) struct Walker<'r, B: RenderBackend> {
    cfg: &'r RenderConfig,
    /// Warnings raised during the walk (unknown directives, directive lines
    /// left as text, unclosed containers, stray closes, empty tab groups),
    /// folded into the result at [`pause`](Self::pause).
    warnings: Vec<String>,
    output: String,
    /// Byte offsets where deferred content belongs — from the built-in tab bars
    /// and from diagram fences.
    holes: Holes,
    /// Every diagram fence the router claimed, in document order.
    diagrams: Vec<DiagramRequest>,
    /// Content for the walk's own holes — the tab bars, filled at group close.
    /// Diagram fills join in [`RenderPass::finish`](crate::RenderPass::finish).
    fills: GlobalFills,
    /// Open tab groups, innermost last (a `::::tabs` may nest in another's
    /// panel), so an inner group never overwrites the outer's reserved bar hole.
    tab_open: Vec<TabGroup>,
    next_group_id: usize,
    next_tab_id: usize,
    table: TableState,
    heading: HeadingAccumulator,
    /// One entry per open container directive, in nesting order, recording what
    /// its opener resolved to so the matching close (which the parser pairs and
    /// emits — explicit or synthesized) renders correctly. A
    /// `ContainerDirectiveEnd` with no entry here is a stray `:::`.
    container_outcomes: Vec<ContainerOutcome>,
    code_block_index: usize,
    /// The previous heading's `(toc_text, rendered_html)` buffers, cleared and
    /// parked for the next one. Headings can't nest, so a single spare pair
    /// covers the whole document: without it every heading allocates two
    /// zero-capacity `String`s and grows them from empty.
    spare_heading_buffers: Option<(String, String)>,
    scopes: Vec<Scope>,
    /// Canonical section refs referenced by prose links in this document.
    section_refs: BTreeSet<String>,
    /// Project files referenced by local images in this document.
    files: BTreeSet<String>,
    _backend: PhantomData<B>,
}

impl<'r, B: RenderBackend> Walker<'r, B> {
    pub(crate) fn new(cfg: &'r RenderConfig) -> Self {
        Self {
            cfg,
            warnings: Vec::new(),
            // Average page-sized documents fit without reallocating.
            output: String::with_capacity(4096),
            holes: Holes::default(),
            diagrams: Vec::new(),
            fills: GlobalFills::default(),
            tab_open: Vec::new(),
            next_group_id: 0,
            next_tab_id: 0,
            table: TableState::default(),
            heading: HeadingAccumulator::new(cfg.drop_title),
            container_outcomes: Vec::new(),
            code_block_index: 0,
            spare_heading_buffers: None,
            scopes: Vec::new(),
            section_refs: BTreeSet::new(),
            files: BTreeSet::new(),
            _backend: PhantomData,
        }
    }

    /// Consume the walker and hand over everything the walk produced, with the
    /// diagram holes still empty.
    ///
    /// Everything but the diagrams is already settled by now: the tab bars
    /// filled at their groups' closes, warnings and section refs accumulated as
    /// the walk raised them, and the parser synthesized a close for every
    /// container left open, so the walk already emitted every closing tag. A
    /// diagram's warnings and its SVG's section refs join in
    /// [`RenderPass::finish`](crate::RenderPass::finish), once the caller has
    /// resolved it.
    ///
    /// # Do not add a step that rewrites the buffer
    ///
    /// The walk buffer is append-only until [`Paused::assemble`] — see the
    /// Invariant section on [`holes`](crate::holes). The window outlives this
    /// function: the buffer stays unassembled for as long as a caller holds a
    /// [`RenderPass`](crate::RenderPass) and resolves diagrams over the
    /// network, and nothing in between may rewrite it either.
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
        // The walker has no `Drop`, so it destructures: each field moves out
        // exactly once.
        let Self {
            output: html,
            holes,
            fills,
            mut heading,
            warnings,
            section_refs,
            files,
            diagrams,
            ..
        } = self;

        Paused {
            html,
            holes,
            fills,
            title: heading.take_title(),
            toc: heading.take_toc(),
            warnings,
            section_refs,
            files,
            diagrams,
        }
    }

    /// Interpret one event.
    ///
    /// `Event::Text` arrives already segmented: the Parser joins adjacent text
    /// into a run, splits inline directives out of it, and lends each piece
    /// borrowed — so a text event is literal by construction and needs no
    /// scanning here. The run-vs-markup ordering is likewise already settled
    /// by the time an event arrives.
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
                let index = self.code_block_index;
                self.code_block_index += 1;

                // A fence the router claims reserves a hole and becomes a
                // request the caller resolves outside the render, rather than
                // rendering as a code block. Scopes are empty here, as
                // `reserve_hole` requires: a code block cannot occur inside a
                // heading or alt text, and blockquotes and list items are not
                // scopes.
                if let Some(router) = self.cfg.diagram_router.as_deref()
                    && let Some(lang) = payload.language.as_deref()
                    && router.handles(lang)
                {
                    let key = hole_key(index);
                    self.reserve_hole(GlobalKey(Source::Diagram, key));
                    // Moved out of `payload`, not cloned: this is the one arm
                    // that never reads it again.
                    self.diagrams.push(DiagramRequest {
                        // Doubles as the hole key `RenderPass::finish` fills.
                        key: RequestKey::from(key),
                        // As authored: `kroki-mermaid` stays `kroki-mermaid`,
                        // because canonicalizing is the provider's business.
                        language: payload
                            .language
                            .take()
                            .expect("the router matched a language, so there is one"),
                        source: DiagramSource::Inline(std::mem::take(&mut payload.source)),
                        attrs: std::mem::take(&mut payload.attrs.map),
                        id: payload.attrs.id.take(),
                    });
                } else {
                    B::code_block(
                        payload.language.as_deref(),
                        &payload.source,
                        &mut self.output,
                    );
                }
            }
            // Block directives arrive already parsed: the decision is made
            // against the fully coalesced run, which only the Parser has.
            Event::ContainerDirectiveStart {
                payload,
                colon_count,
            } => {
                // Tabs are the only container built-in (like `:status` inline);
                // every other container name is unrecognized, so it renders
                // literally and its matching close renders literally too.
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
            Event::LeafDirective(payload) => {
                // `::before` or `::1` on a line of its own is likelier prose.
                if payload.is_unambiguous_leaf() {
                    self.warn_once(format!("unknown leaf directive '::{}'", payload.name));
                }
                // A leaf directive always renders literally.
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

    /// Buffers for a heading's plain-text shadow and formatted HTML body,
    /// reusing the previous heading's pair when there is one. The initial
    /// capacities cover a typical heading without a growth chain.
    fn take_heading_buffers(&mut self) -> (String, String) {
        self.spare_heading_buffers
            .take()
            .unwrap_or_else(|| (String::with_capacity(64), String::with_capacity(128)))
    }

    /// Park a finished heading's buffers for the next heading. Both are spent
    /// by this point — their contents have been copied into `output`, the TOC
    /// entry, or the title — so clearing them loses nothing.
    fn store_heading_buffers(&mut self, mut toc_text: String, mut rendered_html: String) {
        toc_text.clear();
        rendered_html.clear();
        self.spare_heading_buffers = Some((toc_text, rendered_html));
    }

    /// Render one inline directive. Shared by the `Event::InlineDirective` arm
    /// and by [`emit_literal_line`](Self::emit_literal_line).
    ///
    /// `:status` is the only inline built-in; every other name is unknown, so
    /// it warns and renders literally, as `raw`, its byte-exact source slice.
    fn emit_inline_directive(&mut self, name: &str, args: &DirectiveArgs, raw: &str) {
        // `:status` is a walker built-in. The open/close tags route through
        // `with_markup_buffer` like any other inline tag, so status obeys the
        // same scope rules (e.g. inside a heading), while the label goes through
        // `self.text` for escaping and heading-slug/alt-text capture.
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
    /// Only for a leaf or unrecognized container directive that renders as
    /// prose, arriving outside any paragraph event pair and so owing both tags
    /// itself. The Parser took the line whole and never split inline
    /// directives out of it, so they are split here: `:::foo[:status[X]]` with
    /// `foo` unrecognized still expands the inner `:status`.
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

    /// Render a `ContainerDirectiveEnd` the parser paired: pop the outcome its
    /// opener recorded and emit the matching close.
    ///
    /// The parser guarantees a balanced, well-nested stream — every open
    /// container is closed (explicitly, or by a synthesized `implicit` close at
    /// its block/EOF boundary) before the enclosing block's own `End`, so the
    /// outcome stack is always in step with it. A close with no `name` is a
    /// stray `:::` the parser paired to nothing: it pops nothing, warns, and
    /// renders its colons literally.
    fn close_container(&mut self, name: Option<&str>, colon_count: usize, implicit: bool) {
        // A stray can arrive while a container is open (inside a blockquote
        // or list item it encloses) and must leave that container's outcome
        // alone.
        let outcome = name.and_then(|_| self.container_outcomes.pop());

        // An `implicit` close of a tab scope means the author left the
        // container unclosed (the parser synthesized the close at a block/EOF
        // boundary), which warns — naming the directive from the close event
        // (`tab` for an item, `tabs` for a group). A `Literal` implicit close
        // deliberately does not: the opener was already plain prose.
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
            // The opener rendered as text. An explicit close renders its
            // colons literally; an `implicit` one renders nothing (the opener
            // was already plain prose).
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
                    // Every label is known once the group closes, so the
                    // accessible tab-bar markup fills the hole reserved at its
                    // opening offset (before the panels). `assemble` writes it
                    // through `B::raw_html` like every other fill, so the
                    // search backend (no-op `raw_html`) drops the bar labels
                    // by design.
                    let mut bar = String::new();
                    B::tabs_open(group.id, &group.tabs, &mut bar);
                    self.fills
                        .insert(GlobalKey(Source::Tabs, hole_key(group.id)), bar);
                }
            }
            Some(ContainerOutcome::Item) => {
                B::tab_panel_close(&mut self.output);
            }
            // LoneItem: `open_tab_item` emitted no opening tag, so nothing
            // closes.
            Some(ContainerOutcome::LoneItem) => {}
            None => {
                self.warnings
                    .push("stray ::: with no opening directive".to_owned());
                self.emit_literal_line(colon_count, None);
            }
        }
    }

    /// Open a `::::tabs` group: reserve its deferred tab-bar hole at the current
    /// (pre-panel) offset and start collecting tabs. Scopes are empty here —
    /// tabs never open inside a heading or image — so the reservation offset
    /// names the right place in `self.output`.
    fn open_tab_group(&mut self) {
        let id = self.next_group_id;
        self.next_group_id += 1;
        self.reserve_hole(GlobalKey(Source::Tabs, hole_key(id)));
        self.tab_open.push(TabGroup {
            id,
            tabs: Vec::new(),
        });
        self.container_outcomes.push(ContainerOutcome::Group);
    }

    /// Open a `:::tab[Label]` item: emit its panel's opening tag inline (through
    /// the backend) and record the label for the group's deferred bar. A `:::tab`
    /// with no enclosing `::::tabs` renders its content unwrapped and warns.
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
            Tag::Paragraph => {
                // Unconditional: the Parser has already withheld this event for
                // every paragraph that turned out to be a block-directive
                // delimiter, so one arriving here always owes a `<p>`.
                B::paragraph_start(&mut self.output);
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
                    // The resolution carries no copy of the fragment —
                    // `dest_url` is the `#fragment` href itself.
                    WikilinkResolution::Fragment => {
                        self.with_markup_buffer(|out| B::link_start(&dest_url, None, out));
                    }
                    WikilinkResolution::Broken => {
                        self.with_markup_buffer(B::broken_link_start);
                    }
                }
                if !has_pothole {
                    let display = wikilink::display_text(self.cfg, &resolution, &dest_url);
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
                let href = link::resolve_href(self.cfg, &dest_url);
                let section_ref = link::section_ref_attrs(self.cfg, &href);
                let section_attrs = section_ref.as_ref().map(|(r, p)| (r.as_str(), p.as_str()));
                self.with_markup_buffer(|out| B::link_start(&href, section_attrs, out));
                if let Some((r, _)) = section_ref {
                    self.section_refs.insert(r);
                }
            }
            Tag::Image { dest_url, title } => {
                // Alt text drops a nested image, so it references nothing. The
                // origin strip below is for an image left as written: the
                // source path already places a homepage's images.
                let mut served = None;
                if !matches!(self.scopes.last(), Some(Scope::Image { .. })) {
                    match file::resolve(self.cfg, &dest_url) {
                        Some(Ok(path)) => {
                            served = self
                                .cfg
                                .image_src
                                .as_deref()
                                .and_then(|image_src| image_src.image_src(&path, &dest_url));
                            self.files.insert(path);
                        }
                        Some(Err(warning)) => self.warnings.push(warning),
                        None => {}
                    }
                }
                let dest_url = served.unwrap_or_else(|| {
                    link::strip_origin(self.cfg, &dest_url)
                        .unwrap_or(&dest_url)
                        .to_owned()
                });
                self.scopes.push(Scope::Image {
                    alt_text: String::new(),
                    dest_url,
                    title: title.to_string(),
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
                B::paragraph_end(&mut self.output);
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
                let Some(Scope::Image {
                    alt_text,
                    dest_url,
                    title,
                }) = self
                    .scopes
                    .pop_if(|scope| matches!(scope, Scope::Image { .. }))
                else {
                    debug_assert!(false, "TagEnd::Image without matching Scope::Image");
                    return;
                };
                // Pop BEFORE emit: the image's own scope must not intercept
                // its own B::image call — the emit needs to resolve against
                // the parent.
                self.with_markup_buffer(|out| B::image(&dest_url, &alt_text, &title, out));
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

    /// Route an event that is plain text with a markup rendering: `s` feeds
    /// the active scope's plain channel (a heading's `toc_text`, an image's
    /// `alt_text` — `CommonMark` alt text is plain, so inline code lands there
    /// unwrapped), while `write` emits the markup form into the heading's
    /// rendered body or `output` at top level.
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

    /// Reserve a hole at the current output position.
    ///
    /// # Invariant
    ///
    /// Only valid at top level. Inside a scope, output is routed to that
    /// scope's own buffer (`Scope::Heading` collects into `rendered_html` and
    /// is spliced back *trimmed*), so an offset into `self.output` would name
    /// the wrong place in the wrong string.
    fn reserve_hole(&mut self, key: GlobalKey) {
        debug_assert!(
            self.scopes.is_empty(),
            "hole reserved inside a scope: the offset would reference the wrong buffer"
        );
        self.holes.reserve(self.output.len(), key);
    }

    /// Route a line break: markup through `write` (into the heading's rendered
    /// body or `output`), a single space inside alt text — `CommonMark`'s
    /// plain-text projection collapses both break kinds to a space.
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

    /// Run `f` against the buffer of the currently-active markup-accepting
    /// scope, or against `self.output` if no scope is active.
    /// `Image` is a no-op — the closure is never invoked and the buffer is
    /// never exposed.
    ///
    /// Inside a `Heading`, markup lands in the heading's rendered HTML but
    /// `toc_text` is intentionally left alone: it feeds the TOC entry title and
    /// the slug id, which are plain text. Inside an `Image`, markup is dropped
    /// because `CommonMark` alt text is plain text (its visible characters arrive
    /// separately as `Text` events).
    ///
    /// # Invariant
    ///
    /// **This is the only path from inline-markup events to a `&mut String`.**
    /// Do NOT add a `markup_buffer_mut() -> Option<&mut String>` accessor:
    /// the closure form is what makes "alt text drops markup" structural
    /// rather than convention. An accessor lets callers forget to no-op that
    /// scope and silently leak markup into `<img alt="…">`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::HtmlBackend;

    fn cfg() -> RenderConfig {
        RenderConfig::new()
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
