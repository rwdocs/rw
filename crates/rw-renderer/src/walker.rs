//! Per-render scratch state for [`MarkdownRenderer`](crate::MarkdownRenderer).
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
//! Two distinct borrow patterns are load-bearing inside `Walker` methods.
//! Both are explained in detail on the methods that use them:
//! pattern B (tightly-scoped reborrow) lives in [`Walker::flush_text`];
//! pattern A (field-disjoint borrows) lives in the `TagEnd::CodeBlock` arm
//! of [`Walker::end_tag`]. Don't "simplify" either pattern without reading
//! the comments first; both will fail to compile if hoisted.

use std::collections::BTreeSet;
use std::marker::PhantomData;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Tag, TagEnd};

use crate::backend::{AlertKind, RenderBackend};
use crate::code_block::{CodeBlockProcessor, FenceAttrs, ProcessResult, parse_fence_info};
use crate::config::RenderConfig;
use crate::directive::DirectiveOutput;
use crate::directive::DirectiveProcessor;
use crate::directive::Fills;
use crate::directive::Marker;
use crate::directive::Part;
use crate::directive::fills::{GlobalKey, Source};
use crate::directive::parser::{
    ParsedDirective, parse_container_line, parse_leaf_line, parse_line,
};
use crate::directive::processor::BlockDispatch;
use crate::holes::Holes;
use crate::link;
use crate::renderer::RenderResult;
use crate::scope::Scope;
use crate::table::TableState;
use crate::toc::HeadingAccumulator;
use crate::util::heading_level_to_num;
use crate::wikilink::{self, WikilinkResolution};

/// Lifecycle of a paragraph's `<p>`/`</p>` emission.
///
/// `Tag::Paragraph` defers the `<p>` (we may discover the paragraph is a
/// block-directive delimiter, which emits no `<p>`); the `<p>` is committed —
/// or skipped under the `CodeBlock` guard — on the first non-text content.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ParagraphState {
    /// No paragraph open.
    None,
    /// `Tag::Paragraph` seen; `<p>` deferred until we know it's not a directive.
    Deferred,
    /// `<p>` emitted; `</p>` owed at `TagEnd::Paragraph`.
    Open,
}

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
    skip_wikilink_text: bool,
    text_buffer: String,
    /// The previous heading's `(toc_text, rendered_html)` buffers, cleared and
    /// parked for the next one. Headings can't nest, so a single spare pair
    /// covers the whole document: without it every heading allocates two
    /// zero-capacity `String`s and grows them from empty.
    spare_heading_buffers: Option<(String, String)>,
    scopes: Vec<Scope>,
    /// Lifecycle of the current paragraph's `<p>`/`</p>` emission.
    paragraph: ParagraphState,
    /// Current depth of `DirectiveOutput::Markdown` reparse recursion.
    block_depth: usize,
    /// Cached `max_include_depth` from the directive processor (or 10).
    block_depth_limit: usize,
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
        let block_depth_limit = directives
            .as_deref()
            .map_or(10, DirectiveProcessor::max_include_depth);
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
            skip_wikilink_text: false,
            text_buffer: String::new(),
            spare_heading_buffers: None,
            scopes: Vec::new(),
            paragraph: ParagraphState::None,
            block_depth: 0,
            block_depth_limit,
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

    #[allow(clippy::too_many_lines)]
    pub(crate) fn process_event(&mut self, event: Event<'_>) {
        // Block-directive paragraph deferral: a paragraph whose entire text is a
        // `:::`/`::` delimiter emits no <p>/</p>. Decide at End(Paragraph); commit
        // the <p> on the first non-text content.
        if self.paragraph == ParagraphState::Deferred {
            if matches!(&event, Event::End(TagEnd::Paragraph)) {
                self.finish_pending_paragraph();
                return;
            }
            let still_buffering = matches!(&event, Event::Text(_))
                && !matches!(
                    self.scopes.last(),
                    Some(Scope::CodeBlock { .. } | Scope::Metadata)
                )
                && !self.skip_wikilink_text;
            if !still_buffering {
                self.commit_paragraph();
            }
        }

        // Inline-directive expansion needs to see a full `:name[content]`
        // span, but pulldown-cmark splits text at delimiters like `[` and
        // `]`. We buffer adjacent `Event::Text` content outside code blocks
        // and metadata, and flush it through `flush_text` immediately
        // before processing any non-text event.
        let in_code_or_metadata = matches!(
            self.scopes.last(),
            Some(Scope::CodeBlock { .. } | Scope::Metadata)
        );
        let should_buffer =
            matches!(&event, Event::Text(_)) && !in_code_or_metadata && !self.skip_wikilink_text;

        if !should_buffer {
            self.flush_text_buffer();
        }

        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => {
                if self.skip_wikilink_text {
                    self.skip_wikilink_text = false;
                    return;
                }
                // Short-circuit before flush_text_buffer would otherwise run
                // the directive scanner (parse_line / dispatch_inline_named)
                // over YAML content, polluting result.warnings and firing
                // handler side effects. The Scope::Metadata arm of self.text
                // only suppresses the final B::text — by then the side
                // effects have already happened.
                if matches!(self.scopes.last(), Some(Scope::Metadata)) {
                    return;
                }
                let in_code = matches!(self.scopes.last(), Some(Scope::CodeBlock { .. }));
                if in_code {
                    self.text(&text);
                } else {
                    self.text_buffer.push_str(&text);
                }
            }
            Event::Code(code) => {
                self.inline_code(&code);
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

    /// Flush buffered text through inline-directive expansion (if any handlers
    /// are registered) and into the backend via [`text`](Self::text) /
    /// [`raw_html`](Self::raw_html).
    pub(crate) fn flush_text_buffer(&mut self) {
        if self.text_buffer.is_empty() {
            return;
        }
        let buf = std::mem::take(&mut self.text_buffer);
        self.flush_text(&buf);
        self.recycle_text_buffer(buf);
    }

    /// Return a taken text buffer's allocation so the next run of text reuses
    /// it. The `mem::take`s around the walker leave a zero-capacity `String`
    /// behind, so without this every run of text re-allocates and re-grows
    /// from empty.
    ///
    /// Content always wins over capacity: if the walker has meanwhile buffered
    /// text (a nested flush, or a restored outer buffer), that buffer stays and
    /// `buf`'s allocation is dropped. Discarding a spare allocation costs one
    /// `malloc` later; discarding buffered text would silently drop it from the
    /// rendered page.
    fn recycle_text_buffer(&mut self, mut buf: String) {
        if !self.text_buffer.is_empty() {
            return;
        }
        buf.clear();
        if buf.capacity() > self.text_buffer.capacity() {
            self.text_buffer = buf;
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

    fn flush_text(&mut self, text: &str) {
        if self.directives.is_none() {
            self.text(text);
            return;
        }

        let mut remaining = text;
        while !remaining.is_empty() {
            let Some((directive, start, end)) = parse_line(remaining) else {
                self.text(remaining);
                return;
            };

            if start > 0 {
                self.text(&remaining[..start]);
            }

            let matched = &remaining[start..end];

            // `parse_line` only yields inline directives; block delimiters are
            // handled in `finish_pending_paragraph` during the event walk, so
            // anything non-inline is emitted verbatim.
            let ParsedDirective::Inline { name, args } = directive else {
                self.text(matched);
                remaining = &remaining[end..];
                continue;
            };

            // Tightly-scoped processor borrow: dispatch and capture the name
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
                    .expect("checked above: directives is Some");
                processor.dispatch_inline_named(&name, args)
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
                DirectiveOutput::Markdown(md) => {
                    if let Some(p) = self.directives.as_deref_mut() {
                        p.push_warning(format!(
                            "inline directive ':{name}' returned Markdown; emitted as raw HTML (re-parsing of inline-directive Markdown output is not supported)"
                        ));
                    }
                    self.raw_html(&md);
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
                    self.text(matched);
                }
            }

            remaining = &remaining[end..];
        }
    }

    /// Emit the deferred `<p>` (respecting the dormant `CodeBlock` guard) and
    /// mark the paragraph open so `TagEnd::Paragraph` emits `</p>`. Under the
    /// `CodeBlock` guard the `<p>` is skipped and the state falls back to
    /// `None`.
    fn commit_paragraph(&mut self) {
        if matches!(self.scopes.last(), Some(Scope::CodeBlock { .. })) {
            self.paragraph = ParagraphState::None;
        } else {
            B::paragraph_start(&mut self.output);
            self.paragraph = ParagraphState::Open;
        }
    }

    /// Called at `End(Paragraph)` while the paragraph is still pending (text
    /// only). If the coalesced buffer is a block-directive delimiter, dispatch
    /// it (no `<p>`); otherwise render an ordinary paragraph.
    fn finish_pending_paragraph(&mut self) {
        self.paragraph = ParagraphState::None;
        let text = std::mem::take(&mut self.text_buffer);

        // First-byte early-out: only a paragraph whose trimmed text starts with
        // `:` can be a block directive, so skip both parsers (which already
        // reject non-`:` input) for the common prose case.
        if self.directives.is_some()
            && text.trim_start().starts_with(':')
            && let Some(parsed) = parse_container_line(&text).or_else(|| parse_leaf_line(&text))
        {
            self.handle_block_directive(parsed);
        } else {
            self.emit_text_paragraph(&text);
        }

        self.recycle_text_buffer(text);
    }

    /// Render `text` as an ordinary paragraph: emit `<p>`, flush the text
    /// through inline-directive expansion, then `</p>`.
    fn emit_text_paragraph(&mut self, text: &str) {
        self.commit_paragraph();
        self.flush_text(text);
        if self.paragraph == ParagraphState::Open {
            self.paragraph = ParagraphState::None;
            B::paragraph_end(&mut self.output);
        }
    }

    /// Dispatch a recognized block directive through the processor and render
    /// the result. Pattern-B borrow discipline: the `&mut self.directives`
    /// reborrow is dropped (owned `BlockDispatch` returned) before any
    /// `&mut self` method call.
    fn handle_block_directive(&mut self, parsed: ParsedDirective) {
        let depth = self.enclosing_block_depth();
        let dispatch = {
            let processor = self
                .directives
                .as_deref_mut()
                .expect("checked above: directives is Some");
            processor.dispatch_block(parsed, depth)
        };
        match dispatch {
            BlockDispatch::Html(html) => self.raw_html(&html),
            BlockDispatch::Marker { marker, body } => {
                self.marker_open(&marker);
                self.text(&body);
                self.marker_close(&marker);
            }
            BlockDispatch::Markdown(md) => self.reparse_block_markdown(&md),
            BlockDispatch::Deferred { parts, source } => self.emit_parts(parts, source),
            BlockDispatch::PassThrough(text) => self.emit_text_paragraph(&text),
        }
    }

    /// Re-parse a block directive's `Markdown` output in context, feeding the
    /// nested events back through `self.process_event`. Saves/restores the
    /// paragraph state around the loop (belt-and-suspenders) and enforces the
    /// include-depth limit.
    fn reparse_block_markdown(&mut self, md: &str) {
        if self.block_depth >= self.block_depth_limit {
            if let Some(p) = self.directives.as_deref_mut() {
                p.push_warning(format!(
                    "Maximum include depth ({}) exceeded",
                    self.block_depth_limit
                ));
            }
            return;
        }
        self.block_depth += 1;

        let saved_paragraph = self.paragraph;
        let saved_buffer = std::mem::take(&mut self.text_buffer);
        self.paragraph = ParagraphState::None;

        // The parser borrows only `md` (a `&str` that outlives the loop), not
        // `self`, so the nested events can stream straight through
        // `process_event` without being materialized first.
        for event in self.cfg.create_parser(md) {
            self.process_event(event);
        }
        self.flush_text_buffer();

        // Restore the outer buffer, then offer the nested one's allocation
        // back rather than dropping it — otherwise every re-parsed block
        // directive pays a fresh allocate-and-grow cycle. `recycle_text_buffer`
        // keeps the restored buffer if it still holds text.
        let nested_buffer = std::mem::replace(&mut self.text_buffer, saved_buffer);
        self.recycle_text_buffer(nested_buffer);
        self.paragraph = saved_paragraph;
        self.block_depth -= 1;
    }

    /// How deeply the current position is nested in blockquotes and lists.
    ///
    /// A block directive records this when it opens a container scope, so a
    /// container left unclosed can be balanced when the block enclosing it
    /// ends. Counting blockquote and list levels together is enough: the two
    /// stacks only ever grow and shrink in properly nested order, so the sum is
    /// monotonic with actual nesting.
    ///
    /// Distinct from `self.block_depth`, which counts `Markdown` reparse
    /// recursion, not document structure.
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
                // Defer the <p>: see process_event's pending-paragraph block.
                debug_assert!(
                    self.paragraph == ParagraphState::None,
                    "nested pending paragraph"
                );
                self.paragraph = ParagraphState::Deferred;
            }
            Tag::Heading { level, .. } => {
                let level_num = heading_level_to_num(level);
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
                let (language, attrs) = match kind {
                    CodeBlockKind::Fenced(ref info) if !info.is_empty() => {
                        let (lang, attrs) = parse_fence_info(info);
                        (if lang.is_empty() { None } else { Some(lang) }, attrs)
                    }
                    _ => (None, FenceAttrs::default()),
                };
                self.scopes.push(Scope::CodeBlock {
                    language,
                    buffer: String::new(),
                    attrs,
                });
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
                self.scopes.push(Scope::Metadata);
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
                link_type: LinkType::WikiLink { has_pothole },
                dest_url,
                ..
            } if self.cfg.wikilinks => {
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
                    self.skip_wikilink_text = true;
                    self.text(&display);
                }
            }
            Tag::Link { dest_url, .. } => {
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
            Tag::Image {
                dest_url, title, ..
            } => {
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
                if self.paragraph == ParagraphState::Open {
                    self.paragraph = ParagraphState::None;
                    B::paragraph_end(&mut self.output);
                }
            }
            TagEnd::Heading(_level) => {
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
            TagEnd::BlockQuote(_) => {
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
            TagEnd::CodeBlock => {
                if !matches!(self.scopes.last(), Some(Scope::CodeBlock { .. })) {
                    debug_assert!(false, "TagEnd::CodeBlock without matching Scope::CodeBlock");
                    return;
                }
                let Some(Scope::CodeBlock {
                    language,
                    buffer,
                    attrs,
                }) = self.scopes.pop()
                else {
                    unreachable!("peeked match guarantees pop returns Some(Scope::CodeBlock)");
                };
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
                if let Some(lang_str) = language.as_deref() {
                    for (proc_idx, processor) in self.processors.iter_mut().enumerate() {
                        match processor.process(lang_str, &attrs, &buffer, index) {
                            ProcessResult::Deferred => {
                                // Reserve at the current end of the append-only
                                // buffer and write nothing. Scopes are empty
                                // here: Scope::CodeBlock was popped above, and
                                // blockquotes/list items are not scopes.
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
                                self.output.push_str(&html);
                                handled = true;
                                break;
                            }
                            ProcessResult::PassThrough => {}
                        }
                    }
                }

                if !handled {
                    B::code_block(language.as_deref(), &buffer, &mut self.output);
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
            TagEnd::FootnoteDefinition | TagEnd::HtmlBlock => {}
            TagEnd::MetadataBlock(_) => {
                if !matches!(self.scopes.last(), Some(Scope::Metadata)) {
                    debug_assert!(
                        false,
                        "TagEnd::MetadataBlock without matching Scope::Metadata"
                    );
                    return;
                }
                self.scopes.pop();
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
            Some(Scope::CodeBlock { buffer, .. }) => buffer.push_str(text),
            Some(Scope::Metadata) => {}
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
            // Dormant: pulldown-cmark doesn't emit InlineCode inside a fenced code block.
            Some(Scope::CodeBlock { .. }) => {
                debug_assert!(false, "inline_code inside a fenced code block");
            }
            Some(Scope::Metadata) => {}
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
            Some(Scope::CodeBlock { buffer, .. }) => buffer.push('\n'),
            Some(Scope::Metadata) => {}
            None => B::soft_break(&mut self.output),
        }
    }

    fn hard_break(&mut self) {
        match self.scopes.last_mut() {
            Some(Scope::Heading { rendered_html, .. }) => B::hard_break(rendered_html),
            // Hard breaks inside alt text collapse to a single space.
            Some(Scope::Image { alt_text, .. }) => alt_text.push(' '),
            // Dormant: pulldown-cmark doesn't emit HardBreak inside fenced code.
            Some(Scope::CodeBlock { .. }) => {
                debug_assert!(false, "hard_break inside a fenced code block");
            }
            Some(Scope::Metadata) => {}
            None => B::hard_break(&mut self.output),
        }
    }

    /// Run `f` against the buffer of the currently-active markup-accepting
    /// scope, or against `self.output` if no scope is active.
    /// `Image` / `Metadata` are no-ops — the closure is never invoked and
    /// the buffer is never exposed. `CodeBlock` is dormant
    /// (`debug_assert! + drop`).
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
    /// the closure form is what makes "alt text drops markup" and
    /// "metadata block suppresses output" structural rather than convention.
    /// An accessor lets callers forget to no-op those scopes and silently
    /// leak markup into `<img alt="…">` or into the suppressed metadata
    /// block.
    fn with_markup_buffer(&mut self, f: impl FnOnce(&mut String)) {
        match self.scopes.last_mut() {
            Some(Scope::Heading { rendered_html, .. }) => f(rendered_html),
            Some(Scope::Image { .. } | Scope::Metadata) => {}
            Some(Scope::CodeBlock { .. }) => {
                debug_assert!(false, "markup-gated event inside a fenced code block");
            }
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
    fn recycle_text_buffer_keeps_content_over_capacity() {
        // No path reaches this today (every caller recycles into an empty
        // buffer), but the guard is what keeps the optimization from being
        // silently lossy if one ever does: buffered text must survive, even
        // when the incoming allocation is roomier.
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let mut walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());

        walker.text_buffer = String::from("pending");
        walker.recycle_text_buffer(String::with_capacity(4096));
        assert_eq!(walker.text_buffer, "pending");
    }

    #[test]
    fn recycle_text_buffer_reclaims_capacity_when_idle() {
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let mut walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());

        walker.text_buffer = String::new();
        walker.recycle_text_buffer(String::from("spent buffer"));
        assert!(walker.text_buffer.is_empty());
        assert!(
            walker.text_buffer.capacity() > 0,
            "the spent buffer's allocation should have been kept for reuse"
        );
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
        walker.scopes.push(Scope::Metadata);
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
