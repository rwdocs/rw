//! Interpreter half of the render pipeline: turns rw
//! [`Event`](rw_parser::Event)s into backend output.
//!
//! The boundary with [`Parser`](rw_parser::Parser) is syntax versus
//! meaning. The Parser tokenizes: it coalesces text runs, recognizes directive
//! syntax, accumulates fenced code blocks, and swallows metadata blocks, so
//! nothing arriving here needs re-scanning. The Walker interprets what arrives
//! — it is the half that holds the directive registry, the section index and
//! the backend, and it owns the state that spans events (open containers, list
//! and alert stacks, the heading accumulator, the code-block counter, the hole
//! table).
//!
//! `Walker` is constructed fresh inside every call to
//! [`MarkdownRenderer::render`](crate::MarkdownRenderer::render). That's how
//! we guarantee per-render state (`code_block_index`, heading accumulator
//! id-counts, "seen first H1" flag, scope stacks, buffers) starts empty —
//! the renderer's own scratch cannot leak across renders.
//!
//! Borrows the long-lived `RenderConfig` and (mutably) the processor and
//! directive extensions from the façade. Dropped on the way out — including
//! on panic, which leaves the façade's `RenderConfig` untouched for
//! subsequent renders.
//!
//! # Borrow discipline
//!
//! Two borrow patterns recur inside `Walker` methods, each explained in
//! detail where it is first used: pattern A (field-disjoint borrows) in the
//! `Event::CodeBlock` arm of [`Walker::handle`]; pattern B (tightly-scoped
//! reborrow) in [`Walker::emit_inline_directive`], also used by
//! `handle_block_directive`.
//!
//! A third constraint applies wherever a dispatch closure is built: whatever
//! the closure would read off `self` has to be read before it, or the capture
//! collides with the `&mut self` receiver. Don't "simplify" any of them
//! without reading the comments first; each will fail to compile if hoisted.

use std::collections::BTreeSet;
use std::marker::PhantomData;

use crate::backend::RenderBackend;
use crate::code_block::{CodeBlockProcessor, ProcessResult};
use crate::config::RenderConfig;
use crate::directive::DirectiveArgs;
use crate::directive::DirectiveOutput;
use crate::directive::DirectiveProcessor;
use crate::directive::Fills;
use crate::directive::Part;
use crate::directive::fills::{GlobalKey, HoleKey, Source};
use crate::directive::processor::{BlockDispatch, ContainerOutcome, TabScope};
use crate::holes::Holes;
use crate::link;
use crate::renderer::RenderResult;
use crate::scope::Scope;
use crate::status::{STATUS_NAME, StatusColor};
use crate::table::TableState;
use crate::tabs::{TAB_NAME, TABS_NAME, TabInfo};
use crate::toc::HeadingAccumulator;
use crate::wikilink::{self, WikilinkResolution};
use rw_parser::AlertKind;
use rw_parser::{Event, LinkKind, Tag, TagEnd};
use rw_parser::{InlineMatch, parse_line};

/// A tab group the walker is rendering as a built-in: its id, the tabs seen so
/// far, and the reserved bar-hole its accessible `<div role="tablist">` markup
/// fills at [`Walker::finish`] (once every label is known).
struct TabGroup {
    id: usize,
    tabs: Vec<TabInfo>,
    bar_hole: HoleKey,
}

pub(crate) struct Walker<'r, B: RenderBackend> {
    cfg: &'r RenderConfig,
    processors: &'r mut [Box<dyn CodeBlockProcessor>],
    directives: Option<&'r mut DirectiveProcessor>,
    output: String,
    /// Byte offsets where deferred directive content belongs.
    holes: Holes,
    /// Completed tab groups awaiting their bar-hole fill at `finish`.
    tab_groups: Vec<TabGroup>,
    /// Open tab groups, innermost last (a `::::tabs` may nest in another's
    /// panel), so an inner group never overwrites the outer's reserved bar hole.
    tab_open: Vec<TabGroup>,
    next_group_id: usize,
    next_tab_id: usize,
    next_tab_hole: HoleKey,
    list_stack: Vec<bool>,
    table: TableState,
    heading: HeadingAccumulator,
    alert_stack: Vec<Option<AlertKind>>,
    /// One entry per open container directive, in nesting order, recording what
    /// its opener resolved to so the matching close (which the parser now pairs
    /// and emits — explicit or synthesized) renders correctly. A
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
    _backend: PhantomData<B>,
}

impl<'r, B: RenderBackend> Walker<'r, B> {
    /// Construct a fresh walker. Per-render state starts empty;
    /// `HeadingAccumulator` is built from the config's `extract_title` flag
    /// and the backend's `TITLE_AS_METADATA` constant. `output` is pre-allocated
    /// at 4 KiB to give average-sized documents a warm start.
    pub(crate) fn new(
        cfg: &'r RenderConfig,
        processors: &'r mut [Box<dyn CodeBlockProcessor>],
        directives: Option<&'r mut DirectiveProcessor>,
    ) -> Self {
        Self {
            cfg,
            processors,
            directives,
            // 4 KiB warm-start capacity for the output buffer — average-
            // page-sized documents fit without reallocating. A capacity-hint
            // API on the façade could carry per-call sizing (e.g., based on
            // the previous render's final size) but is out of scope here.
            output: String::with_capacity(4096),
            holes: Holes::default(),
            tab_groups: Vec::new(),
            tab_open: Vec::new(),
            next_group_id: 0,
            next_tab_id: 0,
            next_tab_hole: 0,
            list_stack: Vec::new(),
            table: TableState::default(),
            heading: HeadingAccumulator::new(cfg.extract_title, B::TITLE_AS_METADATA),
            alert_stack: Vec::new(),
            container_outcomes: Vec::new(),
            code_block_index: 0,
            spare_heading_buffers: None,
            scopes: Vec::new(),
            section_refs: BTreeSet::new(),
            _backend: PhantomData,
        }
    }

    /// Consume the walker and produce the final `RenderResult`.
    ///
    /// Order still matters, but only in one direction — everything that writes
    /// to the walk buffer must precede assembly, which is keyed to offsets in
    /// it:
    ///
    /// 1. `mem::take` `output` into a local `html` — this owned `String` is
    ///    moved into the returned `RenderResult`, so it must be freestanding.
    ///    (Containers left open no longer need balancing here: the parser
    ///    synthesizes their closes at block/EOF boundaries, so the walk already
    ///    emitted every closing tag.)
    /// 2. Collect fills from directive handlers and code-block processors, then
    ///    assemble: one pass, copying spans of the buffer and writing each fill
    ///    at its reserved offset.
    /// 3. Collect code-block processor warnings, transient-error state, and
    ///    section refs. `has_transient_error` is populated only during step 2
    ///    (by [`CodeBlockProcessor::fills`]). Section refs come from two
    ///    sources: `self.section_refs`, accumulated during the walk itself from
    ///    prose links and wikilinks, plus each processor's `section_refs()`
    ///    (populated during step 2, e.g. from diagram `$link`s) — the two are
    ///    merged here. Warnings may also be pushed earlier — during the walk
    ///    (directive warnings are collected by the façade in `render`), or
    ///    before it, by [`CodeBlockProcessor::bundle`], which runs from the
    ///    separate [`bundle_markdown`](crate::bundle_markdown) entry point
    ///    (used by the S3 publish path) ahead of any walk.
    /// 4. Take title and toc from the heading accumulator.
    ///
    /// # Do not add a step that rewrites the buffer
    ///
    /// Every step before assembly may only *append* to the walk buffer. Hole
    /// offsets are byte positions into that buffer, so any insertion, deletion,
    /// or replacement ahead of a recorded offset shifts the bytes out from
    /// under it and every later fill splices into the wrong place. If you need
    /// to transform the rendered markup, do it after `assemble` returns, on the
    /// finished `String`. See the Invariant section on
    /// [`holes`](crate::holes).
    pub(crate) fn finish(mut self) -> RenderResult {
        debug_assert!(
            self.scopes.is_empty(),
            "Walker::finish called with unclosed scopes (malformed event stream): {} scopes still open",
            self.scopes.len()
        );
        debug_assert!(
            self.list_stack.is_empty(),
            "Walker::finish called with unclosed list nesting: {} levels still open",
            self.list_stack.len()
        );
        debug_assert!(
            self.alert_stack.is_empty(),
            "Walker::finish called with unclosed blockquote/alert nesting: {} levels still open",
            self.alert_stack.len()
        );
        debug_assert!(
            self.container_outcomes.is_empty(),
            "Walker::finish called with unpaired container directives (malformed event stream): {} still open — the parser must synthesize a close for every open container at EOF",
            self.container_outcomes.len()
        );
        debug_assert!(
            self.tab_open.is_empty(),
            "Walker::finish called with a tab group still open: its bar hole would go unfilled ({} still open)",
            self.tab_open.len()
        );

        let mut html = std::mem::take(&mut self.output);
        let holes = std::mem::take(&mut self.holes);

        // Collect fills unconditionally, without first checking whether any
        // hole was reserved. Whether a handler has anything to contribute is
        // the handler's own business, and hole bookkeeping is private to
        // `Holes` — gating the call on it would couple this call site to state
        // it has no reason to inspect. The empty path is cheap:
        // `Fills`/`GlobalFills` wrap `HashMap::default()`, which does not
        // allocate until the first insert, so a handler that sets nothing
        // costs no allocation.
        let mut fills = self
            .directives
            .as_deref_mut()
            .map(DirectiveProcessor::collect_fills)
            .unwrap_or_default();
        for (idx, processor) in self.processors.iter_mut().enumerate() {
            let mut local = Fills::new();
            processor.fills(&mut local);
            fills.merge(Source::CodeBlock(idx), local);
        }

        // Built-in tab bars: each `::::tabs` reserved a hole at its opening
        // offset (before its panels), and now that every label is known the
        // accessible tab-bar markup fills it. `assemble` writes it through
        // `B::raw_html` like every other fill, so the search backend (no-op
        // `raw_html`) drops the bar labels by design.
        for group in &self.tab_groups {
            let mut bar = String::new();
            B::tabs_open(group.id, &group.tabs, &mut bar);
            fills.insert(GlobalKey(Source::Tabs, group.bar_hole), bar);
        }

        // Fills are markup the backend never saw during the walk, so they go in
        // through `raw_html` like every other emission. `assemble` returns
        // `html` untouched when no hole was reserved. Keep this the only step
        // that transforms the buffer — see this function's doc comment.
        html = holes.assemble(html, &fills, B::raw_html);

        let warnings = self
            .processors
            .iter()
            .flat_map(|p| p.warnings())
            .cloned()
            .collect();
        let has_transient_error = self.processors.iter().any(|p| p.has_transient_error());
        let mut section_refs = std::mem::take(&mut self.section_refs);
        for processor in self.processors.iter() {
            section_refs.extend(processor.section_refs().iter().cloned());
        }
        RenderResult {
            html,
            title: self.heading.take_title(),
            toc: self.heading.take_toc(),
            warnings,
            has_transient_error,
            section_refs,
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
            Event::CodeBlock(payload) => {
                let index = self.code_block_index;
                self.code_block_index += 1;

                // Borrow discipline (pattern A): field-disjoint borrows.
                // `self.processors` (mutably via iter_mut) and `self.output` /
                // `self.holes` (mutably in the body) are distinct fields of
                // Walker, so NLL splits the borrow per field. This works only
                // because the body names those fields directly — wrapping the
                // pushes or the reservation in a helper taking `&mut self`
                // would reborrow the whole struct and conflict with iter_mut.
                // That is why this reserves via `self.holes.reserve(...)`
                // rather than the `reserve_hole` helper.
                debug_assert!(
                    self.scopes.is_empty(),
                    "code block processed inside a scope: hole offsets would reference the wrong buffer"
                );
                let mut handled = false;
                if let Some(lang_str) = payload.language.as_deref() {
                    for (proc_idx, processor) in self.processors.iter_mut().enumerate() {
                        match processor.process(lang_str, &payload.attrs, &payload.source, index) {
                            ProcessResult::Deferred => {
                                // Reserve at the current end of the append-only
                                // buffer and write nothing. Scopes are empty
                                // here: a code block cannot occur inside a
                                // heading or alt text, and blockquotes/list
                                // items are not scopes.
                                let key = u32::try_from(index)
                                    .expect("code block index exceeds hole key width");
                                self.holes.reserve(
                                    self.output.len(),
                                    GlobalKey(Source::CodeBlock(proc_idx), key),
                                );
                                handled = true;
                                break;
                            }
                            ProcessResult::Inline(html) => {
                                // Deliberately NOT B::raw_html: `SearchDiagramProcessor`
                                // returns a diagram's text description through this path
                                // and `SearchDocumentBackend::raw_html` is a no-op, so
                                // routing it through the backend would delete every
                                // diagram from the search index.
                                self.output.push_str(&html);
                                handled = true;
                                break;
                            }
                            ProcessResult::PassThrough => {}
                        }
                    }
                }

                if !handled {
                    B::code_block(
                        payload.language.as_deref(),
                        &payload.source,
                        &mut self.output,
                    );
                }
            }
            // Block directives arrive already parsed: the decision is made
            // against the fully coalesced run, which only the Parser has. Each
            // payload's args are moved into the dispatch, never cloned.
            Event::ContainerDirectiveStart(payload) => {
                // Tabs are a walker built-in (like `:status`), not a registered
                // directive: recognize them before dispatching to the processor,
                // so they render even though no handler is registered.
                if payload.name == TABS_NAME {
                    self.open_tab_group();
                } else if payload.name == TAB_NAME {
                    self.open_tab_item(&payload.args);
                } else {
                    // Pattern-B borrow: dispatch under the processor, release the
                    // reborrow, then render. The outcome is recorded so the
                    // parser's matching close renders correctly.
                    let (outcome, dispatch) = {
                        let processor = self.directives.as_deref_mut().expect(
                            "block directives only ever arrive when a processor is registered",
                        );
                        processor.dispatch_container_start(&payload.name, payload.args)
                    };
                    self.container_outcomes.push(outcome);
                    self.render_block_dispatch(dispatch);
                }
            }
            Event::ContainerDirectiveEnd {
                name,
                colon_count,
                implicit,
            } => self.close_container(name.as_deref(), colon_count, implicit),
            Event::LeafDirective(payload) => {
                self.handle_block_directive(move |directives| {
                    directives.dispatch_leaf(&payload.name, payload.args)
                });
            }
            Event::InlineDirective(payload) => {
                self.emit_inline_directive(&payload.name, payload.args, &payload.raw);
            }
        }
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

    /// Emit a reconstructed literal, expanding inline directives on the way,
    /// through [`text`](Self::text) / [`raw_html`](Self::raw_html).
    ///
    /// The Parser segments ordinary text runs itself, so this scanner survives
    /// for exactly one caller: the `BlockDispatch::PassThrough` re-entry in
    /// [`emit_text_paragraph`](Self::emit_text_paragraph), which renders a
    /// block directive nobody claimed as ordinary prose. That literal was
    /// rebuilt from a `DirectiveArgs`, never tokenized, so it legitimately has
    /// to be re-scanned — `:::foo[:kbd[X]]` with `foo` unregistered still
    /// expands the inner `:kbd`.
    ///
    /// That single caller runs only under a registered processor — it is
    /// reached through
    /// [`render_block_dispatch`](Self::render_block_dispatch), which carries
    /// that as a precondition — so this needs no directives-off branch of its
    /// own.
    fn flush_text(&mut self, text: &str) {
        let mut remaining = text;
        while !remaining.is_empty() {
            let Some(InlineMatch { directive, range }) = parse_line(remaining) else {
                self.text(remaining);
                return;
            };

            if range.start > 0 {
                self.text(&remaining[..range.start]);
            }

            let matched = &remaining[range.start..range.end];

            self.emit_inline_directive(&directive.name, directive.args, matched);

            remaining = &remaining[range.end..];
        }
    }

    /// Dispatch one inline directive and render the result. Shared by the
    /// `Event::InlineDirective` arm and by `flush_text`'s re-entry path.
    ///
    /// `raw` is the directive's byte-exact source slice, which is what an
    /// unclaimed directive is emitted as: `DirectiveArgs::to_syntax` is not a
    /// round-trip, so rebuilding the syntax would not reproduce the source.
    fn emit_inline_directive(&mut self, name: &str, args: DirectiveArgs, raw: &str) {
        // `:status` is a walker built-in, not a registered directive: dispatch it
        // directly instead of through `self.directives`, so it renders even when
        // no processor handles it. The open/close tags route through
        // `with_markup_buffer` like any other inline tag, so status obeys the same
        // scope rules (e.g. inside a heading), while the label goes through
        // `self.text` for escaping and heading-slug/alt-text capture.
        if name == STATUS_NAME {
            let color = StatusColor::from(args.get("color").unwrap_or_default());
            self.with_markup_buffer(|out| B::status_open(color, out));
            self.text(args.content().trim());
            self.with_markup_buffer(B::status_close);
            return;
        }

        // Tightly-scoped processor borrow: dispatch and capture the outcome
        // before relinquishing the borrow.
        //
        // Borrow discipline (pattern B): release the `&mut self.directives`
        // reborrow at the end of this block before any `&mut self` method
        // call below. The compiler can't prove those methods don't touch
        // self.directives, so holding the directives
        // borrow across the call would fail. The outcome must be owned data,
        // not a borrow.
        let output = {
            let processor = self
                .directives
                .as_deref_mut()
                .expect("inline directives only ever arrive when a processor is registered");
            processor.dispatch_inline_named(name, args)
        };

        match output {
            DirectiveOutput::Html(html) => {
                self.raw_html(&html);
            }
            DirectiveOutput::Deferred(parts) => {
                if let Some(p) = self.directives.as_deref_mut() {
                    p.push_warning(format!(
                        "inline directive ':{name}' returned Deferred; its holes were dropped (inline directives cannot defer content — return Html instead)"
                    ));
                }
                // Emit the literal pieces so their content isn't lost, but
                // skip the holes: `InlineDirective` has no `fills()` hook,
                // so a reserved hole could never be filled.
                for part in parts {
                    if let Part::Html(html) = part {
                        self.raw_html(&html);
                    }
                }
            }
            DirectiveOutput::Skip => {
                if let Some(p) = self.directives.as_deref_mut() {
                    p.push_warning(format!(
                        "unknown inline directive ':{name}' — no handler registered (or handler returned Skip)"
                    ));
                }
                self.text(raw);
            }
        }
    }

    /// Render `text` as an ordinary paragraph: emit `<p>`, flush the text
    /// through inline-directive expansion, then `</p>`.
    ///
    /// Only for the re-entry path — a block directive the processor handed back
    /// as [`BlockDispatch::PassThrough`], which arrives outside any paragraph
    /// event pair and so owes both tags itself. A paragraph the Parser
    /// recognized as prose emits its own `Start`/`End(Paragraph)` instead.
    fn emit_text_paragraph(&mut self, text: &str) {
        B::paragraph_start(&mut self.output);
        self.flush_text(text);
        B::paragraph_end(&mut self.output);
    }

    /// Dispatch a recognized block directive through the processor and render
    /// the result. Pattern-B borrow discipline: the `&mut self.directives`
    /// reborrow is dropped (owned `BlockDispatch` returned) before any
    /// `&mut self` method call.
    fn handle_block_directive(
        &mut self,
        dispatch_with: impl FnOnce(&mut DirectiveProcessor) -> BlockDispatch,
    ) {
        let dispatch = {
            let processor = self
                .directives
                .as_deref_mut()
                .expect("block directives only ever arrive when a processor is registered");
            dispatch_with(processor)
        };
        self.render_block_dispatch(dispatch);
    }

    /// Render what the processor handed back.
    ///
    /// Kept out of [`handle_block_directive`](Self::handle_block_directive),
    /// which is generic over its dispatch closure and so monomorphizes once
    /// per call site: holding this `match` there would emit one copy per
    /// backend *per call site* instead of one per backend.
    ///
    /// # Precondition
    ///
    /// Only call this with a dispatch obtained under a registered processor.
    /// A [`BlockDispatch::PassThrough`] literal is re-scanned for inline
    /// directives by [`flush_text`](Self::flush_text), which unwraps
    /// `self.directives` without a directives-off branch.
    fn render_block_dispatch(&mut self, dispatch: BlockDispatch) {
        match dispatch {
            BlockDispatch::Html(html) => self.raw_html(&html),
            BlockDispatch::Deferred { parts, source } => self.emit_parts(parts, source),
            BlockDispatch::PassThrough(text) => self.emit_text_paragraph(&text),
        }
    }

    /// Push the "unclosed container directive" warning for `name`, if a
    /// directive processor is registered. Shared by `close_container`'s
    /// implicit-close arm and `close_tab_scope`, which warn on the same
    /// condition with identical text.
    fn warn_unclosed_container(&mut self, name: Option<&str>) {
        if let Some(processor) = self.directives.as_deref_mut() {
            processor.push_warning(format!(
                "unclosed container directive :::{} (missing closing :::)",
                name.unwrap_or_default()
            ));
        }
    }

    /// Render a `ContainerDirectiveEnd` the parser paired: pop the outcome its
    /// opener recorded and emit the matching close.
    ///
    /// The parser guarantees a balanced, well-nested stream — every open
    /// container is closed (explicitly, or by a synthesized `implicit` close at
    /// its block/EOF boundary) before the enclosing block's own `End`, so the
    /// outcome stack is always in step with it. Three cases:
    ///
    /// * [`Handled(idx)`](ContainerOutcome::Handled) — a registered handler;
    ///   emit its `end()`. An `implicit` close of one is an unclosed container:
    ///   warn (the parser closed it for the author).
    /// * [`Literal`](ContainerOutcome::Literal) — the opener rendered as text.
    ///   An explicit close renders its colons literally; an `implicit` one
    ///   renders nothing (the opener was already plain prose).
    /// * no outcome — a stray `:::` the parser paired to nothing: warn and
    ///   render its colons literally.
    fn close_container(&mut self, name: Option<&str>, colon_count: usize, implicit: bool) {
        match self.container_outcomes.pop() {
            Some(ContainerOutcome::Handled(idx)) => {
                if implicit {
                    self.warn_unclosed_container(name);
                }
                // Pattern-B borrow: release the reborrow before render.
                let dispatch = {
                    let processor = self
                        .directives
                        .as_deref_mut()
                        .expect("a Handled outcome was recorded under a registered processor");
                    processor.container_end(idx)
                };
                self.render_block_dispatch(dispatch);
            }
            Some(ContainerOutcome::Literal) => {
                if !implicit {
                    self.emit_text_paragraph(&":".repeat(colon_count));
                }
            }
            Some(ContainerOutcome::Native(scope)) => {
                self.close_tab_scope(scope, name, implicit);
            }
            None => {
                if let Some(processor) = self.directives.as_deref_mut() {
                    processor.push_warning("stray ::: with no opening directive".to_owned());
                }
                self.emit_text_paragraph(&":".repeat(colon_count));
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
        let bar_hole = self.next_tab_hole;
        self.next_tab_hole += 1;
        self.reserve_hole(GlobalKey(Source::Tabs, bar_hole));
        self.tab_open.push(TabGroup {
            id,
            tabs: Vec::new(),
            bar_hole,
        });
        self.container_outcomes
            .push(ContainerOutcome::Native(TabScope::Group));
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
            if let Some(p) = self.directives.as_deref_mut() {
                p.push_warning(
                    "`:::tab` outside a `::::tabs` group; its content is rendered without tab chrome"
                        .to_owned(),
                );
            }
            self.container_outcomes
                .push(ContainerOutcome::Native(TabScope::LoneItem));
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
        self.container_outcomes
            .push(ContainerOutcome::Native(TabScope::Item));
    }

    /// Render the close of a built-in tab scope the parser paired. An `implicit`
    /// close means the author left the container unclosed (the parser
    /// synthesized the close at a block/EOF boundary), which warns — naming the
    /// directive from the close event (`tab` for an item, `tabs` for a group),
    /// matching a registered container's unclosed warning.
    fn close_tab_scope(&mut self, scope: TabScope, name: Option<&str>, implicit: bool) {
        if implicit {
            self.warn_unclosed_container(name);
        }
        match scope {
            TabScope::Item => B::tab_panel_close(&mut self.output),
            TabScope::Group => {
                if let Some(group) = self.tab_open.pop() {
                    if group.tabs.is_empty()
                        && let Some(processor) = self.directives.as_deref_mut()
                    {
                        processor.push_warning("`::::tabs` group has no `:::tab` items".to_owned());
                    }
                    B::tabs_close(&mut self.output);
                    self.tab_groups.push(group);
                }
            }
            // LoneItem: `open_tab_item` emitted no opening tag, so nothing closes.
            TabScope::LoneItem => {}
        }
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
            Tag::Heading { level: level_num } => {
                // Decide once, at start: is_skipped_title flips to false
                // after the first H1 closes, so TagEnd::Heading would get a
                // different answer and skip nothing (or skip the wrong
                // heading) if we re-consulted.
                let in_first_h1 = self.heading.is_skipped_title(level_num);
                let (toc_text, rendered_html) = self.take_heading_buffers();
                self.scopes.push(Scope::Heading {
                    level: level_num,
                    in_first_h1,
                    toc_text,
                    rendered_html,
                });
            }
            Tag::BlockQuote(kind) => {
                if let Some(alert_kind) = kind {
                    self.alert_stack.push(Some(alert_kind));
                    B::alert_start(alert_kind, &mut self.output);
                } else {
                    self.alert_stack.push(None);
                    B::blockquote_start(&mut self.output);
                }
            }
            Tag::List(start) => {
                self.list_stack.push(start.is_some());
                B::list_start(start.is_some(), start, &mut self.output);
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
                        if !section_ref.is_empty() {
                            self.section_refs.insert(section_ref.clone());
                        }
                        let section_attrs = (!section_ref.is_empty())
                            .then_some((section_ref.as_str(), subpath.as_str()));
                        self.with_markup_buffer(|out| B::link_start(href, section_attrs, out));
                    }
                    WikilinkResolution::Fragment(fragment) => {
                        let href = format!("#{fragment}");
                        self.with_markup_buffer(|out| B::link_start(&href, None, out));
                    }
                    WikilinkResolution::Broken { .. } => {
                        self.with_markup_buffer(B::broken_link_start);
                    }
                }
                if !has_pothole {
                    let display = wikilink::display_text(self.cfg, &resolution);
                    self.text(&display);
                }
            }
            Tag::Link {
                kind: LinkKind::Other,
                dest_url,
            } => {
                let dest_url = link::strip_origin(self.cfg, &dest_url);
                let base = link::link_base(self.cfg);
                let href = B::transform_link(&dest_url, base);
                let section_ref = link::section_ref_attrs(self.cfg, &href);
                if let Some((r, _)) = &section_ref {
                    self.section_refs.insert(r.clone());
                }
                let section_attrs = section_ref.as_ref().map(|(r, p)| (r.as_str(), p.as_str()));
                self.with_markup_buffer(|out| B::link_start(&href, section_attrs, out));
            }
            Tag::Image { dest_url, title } => {
                let dest_url = link::strip_origin(self.cfg, &dest_url).into_owned();
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
                if !matches!(self.scopes.last(), Some(Scope::Heading { .. })) {
                    debug_assert!(false, "TagEnd::Heading without matching Scope::Heading");
                    return;
                }
                let Some(Scope::Heading {
                    level,
                    in_first_h1,
                    toc_text,
                    rendered_html,
                }) = self.scopes.pop()
                else {
                    unreachable!("peeked match guarantees pop returns Some(Scope::Heading)");
                };
                if in_first_h1 {
                    self.heading.complete_first_h1(&toc_text);
                    self.store_heading_buffers(toc_text, rendered_html);
                } else {
                    let done = self
                        .heading
                        .complete_heading(level, &toc_text, rendered_html);
                    B::heading_start(done.adjusted_level, &done.id, &mut self.output);
                    self.output.push_str(done.rendered_html.trim());
                    B::heading_end(done.adjusted_level, &mut self.output);
                    self.store_heading_buffers(toc_text, done.rendered_html);
                }
            }
            TagEnd::BlockQuote => match self.alert_stack.pop() {
                Some(Some(alert_kind)) => {
                    B::alert_end(alert_kind, &mut self.output);
                }
                _ => {
                    B::blockquote_end(&mut self.output);
                }
            },
            TagEnd::List(ordered) => {
                self.list_stack.pop();
                B::list_end(ordered, &mut self.output);
            }
            TagEnd::Item => {
                B::list_item_end(&mut self.output);
            }
            TagEnd::Image => {
                if !matches!(self.scopes.last(), Some(Scope::Image { .. })) {
                    debug_assert!(false, "TagEnd::Image without matching Scope::Image");
                    return;
                }
                let Some(Scope::Image {
                    alt_text,
                    dest_url,
                    title,
                }) = self.scopes.pop()
                else {
                    unreachable!("peeked match guarantees pop returns Some(Scope::Image)");
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

    fn text(&mut self, text: &str) {
        match self.scopes.last_mut() {
            Some(Scope::Heading {
                rendered_html,
                toc_text,
                ..
            }) => {
                toc_text.push_str(text);
                B::text(text, rendered_html);
            }
            Some(Scope::Image { alt_text, .. }) => alt_text.push_str(text),
            None => B::text(text, &mut self.output),
        }
    }

    fn inline_code(&mut self, code: &str) {
        match self.scopes.last_mut() {
            Some(Scope::Heading {
                rendered_html,
                toc_text,
                ..
            }) => {
                toc_text.push_str(code);
                B::inline_code(code, rendered_html);
            }
            // CommonMark: alt text is plain text — append code body without `<code>` wrap.
            Some(Scope::Image { alt_text, .. }) => alt_text.push_str(code),
            None => B::inline_code(code, &mut self.output),
        }
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

    /// Emit a deferred directive's parts: literal HTML inline, holes reserved
    /// at the position the fill will occupy.
    ///
    /// `source` identifies the handler the parts came from; its local hole
    /// keys become global here.
    fn emit_parts(&mut self, parts: Vec<Part>, source: Source) {
        for part in parts {
            match part {
                Part::Html(html) => self.raw_html(&html),
                Part::Hole(key) => self.reserve_hole(GlobalKey(source, key)),
            }
        }
    }

    fn soft_break(&mut self) {
        match self.scopes.last_mut() {
            Some(Scope::Heading { rendered_html, .. }) => B::soft_break(rendered_html),
            // Soft breaks inside alt text collapse to a single space (CommonMark plain-text rule).
            Some(Scope::Image { alt_text, .. }) => alt_text.push(' '),
            None => B::soft_break(&mut self.output),
        }
    }

    fn hard_break(&mut self) {
        match self.scopes.last_mut() {
            Some(Scope::Heading { rendered_html, .. }) => B::hard_break(rendered_html),
            // Hard breaks inside alt text collapse to a single space.
            Some(Scope::Image { alt_text, .. }) => alt_text.push(' '),
            None => B::hard_break(&mut self.output),
        }
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
    fn finish_returns_empty_result_when_no_events_processed() {
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());
        let result = walker.finish();
        assert!(result.html.is_empty());
        assert!(result.title.is_none());
        assert!(result.toc.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    #[should_panic(expected = "unclosed scopes")]
    #[cfg(debug_assertions)]
    fn finish_debug_asserts_scopes_empty() {
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let mut walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());
        walker.scopes.push(Scope::Image {
            alt_text: String::new(),
            dest_url: String::new(),
            title: String::new(),
        });
        walker.finish();
    }

    #[test]
    #[should_panic(expected = "unclosed list nesting")]
    #[cfg(debug_assertions)]
    fn finish_debug_asserts_list_stack_empty() {
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let mut walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());
        walker.list_stack.push(false);
        walker.finish();
    }

    #[test]
    #[should_panic(expected = "unclosed blockquote/alert nesting")]
    #[cfg(debug_assertions)]
    fn finish_debug_asserts_alert_stack_empty() {
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let mut walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());
        walker.alert_stack.push(None);
        walker.finish();
    }
}
