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
//! `Event::CodeBlock` arm of [`Walker::handle`], also used by
//! `close_containers_for_block_end`; pattern B (tightly-scoped reborrow) in
//! [`Walker::emit_inline_directive`], also used by `handle_block_directive`.
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
use crate::directive::Marker;
use crate::directive::Part;
use crate::directive::fills::{GlobalKey, Source};
use crate::directive::processor::BlockDispatch;
use crate::holes::Holes;
use crate::link;
use crate::renderer::RenderResult;
use crate::scope::Scope;
use crate::table::TableState;
use crate::toc::HeadingAccumulator;
use crate::wikilink::{self, WikilinkResolution};
use rw_parser::AlertKind;
use rw_parser::{Event, LinkKind, Tag, TagEnd};
use rw_parser::{InlineMatch, parse_line};

pub(crate) struct Walker<'r, B: RenderBackend> {
    cfg: &'r RenderConfig,
    processors: &'r mut [Box<dyn CodeBlockProcessor>],
    directives: Option<&'r mut DirectiveProcessor>,
    output: String,
    /// Byte offsets where deferred directive content belongs.
    holes: Holes,
    list_stack: Vec<bool>,
    table: TableState,
    heading: HeadingAccumulator,
    alert_stack: Vec<Option<AlertKind>>,
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
            list_stack: Vec::new(),
            table: TableState::default(),
            heading: HeadingAccumulator::new(cfg.extract_title, B::TITLE_AS_METADATA),
            alert_stack: Vec::new(),
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
    /// 1. Close containers still open at end of input, appending their closing
    ///    markup to `output`. Appending only extends the buffer, so every
    ///    recorded hole offset stays valid.
    /// 2. `mem::take` `output` into a local `html` — this owned `String` is
    ///    moved into the returned `RenderResult`, so it must be freestanding.
    /// 3. Collect fills from directive handlers and code-block processors, then
    ///    assemble: one pass, copying spans of the buffer and writing each fill
    ///    at its reserved offset.
    /// 4. Collect code-block processor warnings, transient-error state, and
    ///    section refs. `has_transient_error` is populated only during step 3
    ///    (by [`CodeBlockProcessor::fills`]). Section refs come from two
    ///    sources: `self.section_refs`, accumulated during the walk itself from
    ///    prose links and wikilinks, plus each processor's `section_refs()`
    ///    (populated during step 3, e.g. from diagram `$link`s) — the two are
    ///    merged here. Warnings may also be pushed earlier — during the walk
    ///    (directive warnings are collected by the façade in `render`), or
    ///    before it, by [`CodeBlockProcessor::bundle`], which runs from the
    ///    separate [`bundle_markdown`](crate::bundle_markdown) entry point
    ///    (used by the S3 publish path) ahead of any walk.
    /// 5. Take title and toc from the heading accumulator.
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
        // Balance any container left open by missing `:::`, while the buffer is
        // still the walk buffer holes address. Appending only extends it, so
        // every recorded hole offset (all `<= output.len()`) stays valid.
        if let Some(processor) = self.directives.as_deref_mut() {
            processor.close_unclosed_containers(&mut self.output, B::raw_html);
        }

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
                // Read before the closure: calling this inside would capture
                // `&*self` and collide with the `&mut self` receiver.
                let depth = self.enclosing_block_depth();
                self.handle_block_directive(move |directives| {
                    directives.dispatch_container_start(&payload.name, payload.args, depth)
                });
            }
            Event::ContainerDirectiveEnd { colon_count } => {
                self.handle_block_directive(move |directives| {
                    directives.dispatch_container_end(colon_count)
                });
            }
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
            DirectiveOutput::Marker { marker, body } => {
                self.marker_open(&marker);
                self.text(&body);
                self.marker_close(&marker);
            }
            DirectiveOutput::Deferred(parts) => {
                if let Some(p) = self.directives.as_deref_mut() {
                    p.push_warning(format!(
                        "inline directive ':{name}' returned Deferred; its holes were dropped (inline directives cannot defer content — return Marker instead)"
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
            BlockDispatch::Marker { marker, body } => {
                self.marker_open(&marker);
                self.text(&body);
                self.marker_close(&marker);
            }
            BlockDispatch::Deferred { parts, source } => self.emit_parts(parts, source),
            BlockDispatch::PassThrough(text) => self.emit_text_paragraph(&text),
        }
    }

    /// How deeply the current position is nested in blockquotes and lists.
    ///
    /// A block directive records this when it opens a container scope, so a
    /// container left unclosed can be balanced when the block enclosing it
    /// ends. Counting blockquote and list levels together is enough: the two
    /// stacks only ever grow and shrink in properly nested order, so the sum is
    /// monotonic with actual nesting.
    fn enclosing_block_depth(&self) -> usize {
        self.alert_stack.len() + self.list_stack.len()
    }

    /// Close container scopes opened inside the blockquote or list item that is
    /// ending, before its own closing tag is written.
    ///
    /// Must be called while the level being closed is still on its stack — the
    /// depth it computes is the one containers opened *directly inside* it
    /// recorded.
    fn close_containers_for_block_end(&mut self) {
        let depth = self.enclosing_block_depth();
        // Field-disjoint borrows (pattern A): `self.directives` and
        // `self.output` are separate fields, so NLL splits the borrow.
        if let Some(processor) = self.directives.as_deref_mut() {
            processor.close_containers_nested_in(depth, &mut self.output, B::raw_html);
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
            TagEnd::BlockQuote => {
                self.close_containers_for_block_end();
                match self.alert_stack.pop() {
                    Some(Some(alert_kind)) => {
                        B::alert_end(alert_kind, &mut self.output);
                    }
                    _ => {
                        B::blockquote_end(&mut self.output);
                    }
                }
            }
            TagEnd::List(ordered) => {
                self.list_stack.pop();
                B::list_end(ordered, &mut self.output);
            }
            TagEnd::Item => {
                self.close_containers_for_block_end();
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

    /// Route a marker's opening to the backend. A marker is markup, so it
    /// follows the same scope rules as every other inline tag.
    fn marker_open(&mut self, marker: &Marker) {
        self.with_markup_buffer(|out| B::marker_open(marker, out));
    }

    /// Route a marker's closing to the backend. See [`marker_open`](Self::marker_open).
    fn marker_close(&mut self, marker: &Marker) {
        self.with_markup_buffer(|out| B::marker_close(marker, out));
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
