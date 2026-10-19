//! Markdown tokenizer: wraps `pulldown_cmark` and emits rw's [`Event`].
//!
//! The Parser recognizes **syntax** — markdown structure and rw's directive
//! syntax — and has no knowledge of what any directive name means. Every
//! interpretation decision belongs to the consumer.
//!
//! Takes only what a tokenizer needs, by value. The cmark feature set is its
//! own too: `cmark_options` defines rw's markdown dialect.

use std::collections::VecDeque;
use std::iter::Fuse;
use std::ops::Range;

use pulldown_cmark as cmark;
use pulldown_cmark::{CodeBlockKind, CowStr, HeadingLevel, Options};

use crate::alert::AlertKind;
use crate::directive::DirectiveArgs;
use crate::directive::line::{
    ContainerLine, Escapes, InlineMatch, parse_container_line, parse_escaped_line, parse_leaf_line,
};
use crate::event::{CodeBlockPayload, Event, InlineDirectivePayload, LinkKind, Tag, TagEnd};
use crate::fence::{FenceAttrs, parse_fence_info};

/// Convert heading level enum to number (1-6).
#[must_use]
fn heading_level_to_num(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// A fenced or indented code block being accumulated.
///
/// A single slot, not a stack: code blocks do not nest, and cannot occur
/// inside a heading or alt text, so this can never need to interleave with a
/// consumer's own nesting state.
struct CodeBlockAccum {
    language: Option<String>,
    attrs: FenceAttrs,
    buffer: String,
}

/// One open `:::` container scope tracked by the parser for pairing.
struct OpenContainer {
    name: String,
    /// Blockquote + list nesting depth at the opener, used to decide which
    /// boundary (an enclosing `End(BlockQuote)`/`End(Item)`, or EOF) synthesizes
    /// this container's close.
    block_depth: usize,
}

/// What the Parser holds back, or still owes, before it may pull from cmark
/// again.
///
/// Releasing a held `<p>` or line break returns it at once and owes the rest
/// in order: for `:foo **bold**`, the `<p>`, then the coalesced run, then the
/// `Start(Strong)` that triggered the release.
///
/// # Invariant
///
/// The stashed [`Event<'a>`] borrows the **source**, never the run buffer: it
/// is what [`Parser::translate`] returned, at the source lifetime. A
/// run-borrowed value assigned here does not compile (`assignment requires
/// that '1 must outlive 'a`) — which is exactly why run-borrowed text must be
/// yielded straight out of [`Parser::next`] and never stored.
enum Deferred<'a> {
    None,
    /// `Start(Paragraph)` seen and held; the run is still coalescing.
    Paragraph,
    /// This soft or hard break at the top level of an open paragraph, held
    /// while the line after it coalesces: that line may be a block
    /// directive, which ends the paragraph instead.
    LineBreak(Event<'a>),
    /// Owe the run, then this event: a run interrupted by markup.
    Draining(Event<'a>),
}

/// One slice cut out of the coalesced run, named by **byte range** rather than
/// by `CowStr`.
///
/// The indirection is what keeps the lending property: a
/// `-> Option<Event<'_>>` helper would reborrow `*self` for the whole of
/// [`Parser::next`]'s return lifetime, so the segment is resolved as owned data
/// first — ending the `&mut self` borrow — and the slice is taken afresh, in
/// the return expression, by [`run_segment_event`].
enum RunSegment {
    /// Literal text.
    Text(Range<usize>),
    /// `:name[content]{attrs}`; the range is the byte-exact source slice.
    Inline {
        name: String,
        args: DirectiveArgs,
        raw: Range<usize>,
    },
}

/// Lend a matched directive as a segment, moving its name and args.
///
/// `range` must already be absolute within the run buffer, which is what
/// [`RunSegment::Inline::raw`] indexes.
impl From<InlineMatch> for RunSegment {
    fn from(matched: InlineMatch) -> Self {
        RunSegment::Inline {
            name: matched.directive.name,
            args: matched.directive.args,
            raw: matched.range,
        }
    }
}

/// A lending tokenizer over one markdown source.
///
/// Pull events with [`next`](Self::next) until it returns `None`. Each event
/// borrows the Parser, so consume it before asking for the next one — that is
/// what keeps a text run borrowed out of a reused buffer rather than allocated
/// per segment.
pub struct Parser<'a> {
    inner: Fuse<cmark::OffsetIter<'a>>,
    source: &'a str,
    /// Source offset just past the last character judged escaped.
    escaped_until: usize,
    /// Recognize directive syntax. When false, `:name[…]`, `::name` and
    /// `:::name` stay prose and no directive event is ever emitted.
    ///
    /// A consumer that cannot act on directives must switch them off here
    /// rather than ignore the events: a block directive's line arrives only
    /// inside its event, so an ignored event loses the line.
    directives: bool,
    /// The coalesced text run. cmark splits text at every `[`, `]`, entity and
    /// escape, and rw's directive syntax is only recognizable joined.
    run: String,
    /// Where [`run`](Self::run) holds characters cmark decoded from an escape
    /// or entity, sorted. Cleared with the run.
    escapes: Vec<usize>,
    /// How much of [`run`](Self::run) has already been lent out. The buffer is
    /// cleared (never reallocated) on the call after the borrow expires.
    run_cursor: usize,
    /// An inline directive already cut from the run, owed once the text
    /// preceding it has been lent.
    ///
    /// One slot: a single `parse_line` call yields at most one match, and the
    /// run is fully drained before cmark is pulled again. Only a directive is
    /// ever parked — text is lent directly.
    ///
    /// The slot exists to keep the parse, not to queue work — `parse_line`
    /// allocates the directive's name and args, so re-parsing at the cursor
    /// after lending the text before it would cost an allocation set per inline
    /// directive. The cursor stays the resume point: it is parked *before*
    /// this directive and moves past it when the slot is taken, so the run
    /// buffer `range` indexes cannot be recycled underneath it.
    pending: Option<InlineMatch>,
    deferred: Deferred<'a>,
    code_block: Option<CodeBlockAccum>,
    /// Open container-directive scopes, innermost last, for pairing a `:::`
    /// closer to its opener's name and synthesizing closes at block boundaries.
    /// Each entry records the opener name and the block-nesting depth at which
    /// it opened.
    open_containers: Vec<OpenContainer>,
    /// Blockquote + list nesting depth (NOT counting list items). Drives boundary
    /// synthesis: a container is closed when the block it opened inside ends.
    block_depth: usize,
    /// Events owed before anything in [`deferred`](Self::deferred), drained at
    /// the top of [`next`](Self::next): container closes synthesized at a
    /// block/EOF boundary (innermost first, with an `End(BlockQuote)` /
    /// `End(Item)` boundary riding behind them), or a directive line split out
    /// of a paragraph after its `End(Paragraph)`, with the `Start(Paragraph)`
    /// that re-opens the rest of it.
    owed: VecDeque<Event<'a>>,
    /// Open inline tags, plus one outside a paragraph. A line break can split
    /// a paragraph only at 0, where no tag would straddle the split.
    split_depth: u32,
    /// Blockquote nesting depth: a line with fewer `>` is a lazy continuation.
    quote_depth: usize,
    /// Source offset of the open paragraph's first text.
    paragraph_at: usize,
    /// Source offset of the last line break.
    break_at: usize,
    /// Inside a YAML metadata block: every event is swallowed.
    in_metadata: bool,
    /// Swallow the next `Text`: it is the display text cmark synthesises for a
    /// pothole-less `[[wikilink]]`, whose display text rw resolves itself
    /// instead.
    ///
    /// One-shot, and Parser-only. A consumer-side copy could never clear itself:
    /// the reset belongs on the very `Text` event this flag makes the Parser
    /// swallow.
    skip_wikilink_text: bool,
}

/// rw's markdown dialect: the cmark features every render enables, plus
/// wikilinks when `wikilinks` is set.
///
/// Scoped to rendering. Other passes over the same document — frontmatter and
/// title extraction among them — parse with their own, narrower option sets,
/// so this is not a site-wide definition of the markdown rw accepts.
fn cmark_options(wikilinks: bool) -> Options {
    let mut opts = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    if wikilinks {
        opts |= Options::ENABLE_WIKILINKS;
    }
    opts
}

impl<'a> Parser<'a> {
    /// Tokenize `markdown`.
    ///
    /// `wikilinks` enables `[[target]]` syntax; with it off, cmark leaves the
    /// brackets as literal text. `directives` enables rw's `:name` /
    /// `::name` / `:::name` syntax; with it off, those stay prose. Both are
    /// off in plain `CommonMark`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rw_parser::{Event, Parser, Tag};
    ///
    /// let mut parser = Parser::new("hi", false, false);
    /// assert_eq!(parser.next(), Some(Event::Start(Tag::Paragraph)));
    /// ```
    #[must_use]
    pub fn new(markdown: &'a str, wikilinks: bool, directives: bool) -> Self {
        Self {
            inner: cmark::Parser::new_ext(markdown, cmark_options(wikilinks))
                .into_offset_iter()
                .fuse(),
            source: markdown,
            escaped_until: 0,
            directives,
            run: String::new(),
            escapes: Vec::new(),
            run_cursor: 0,
            pending: None,
            deferred: Deferred::None,
            code_block: None,
            open_containers: Vec::new(),
            block_depth: 0,
            owed: VecDeque::new(),
            split_depth: 1,
            quote_depth: 0,
            paragraph_at: 0,
            break_at: 0,
            in_metadata: false,
            skip_wikilink_text: false,
        }
    }

    /// Pull the next event.
    ///
    /// **Inherent, not `Iterator`.** The returned `Event<'_>` borrows `self`,
    /// which is a lending iterator — expressible without GATs only as an
    /// inherent method. That is what lets text stay borrowed rather than
    /// forcing an owned `Box<str>` per segment, keeping the text path
    /// zero-allocation.
    ///
    /// # Release order
    ///
    /// Everything owed comes out before cmark is pulled again, in the order a
    /// consumer needs it: the held `<p>` or line break, then the run, then
    /// the event that interrupted the run. See `Deferred`.
    // The name is deliberate despite the lint: this is the lending counterpart
    // of `Iterator::next`, and calling it anything else would only disguise
    // that. The signature already refuses `Iterator`, so there is nothing to
    // confuse it with.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Event<'_>> {
        loop {
            // The previous call lent the whole run out; that borrow has since
            // expired, so reclaim the buffer. `clear` keeps the allocation —
            // this is the recycling the lending design exists to preserve.
            if self.run_cursor > 0 && self.run_cursor >= self.run.len() {
                self.run.clear();
                self.escapes.clear();
                self.run_cursor = 0;
            }

            // 0. Container closes synthesized at a block/EOF boundary, or a
            //    directive line split out of a paragraph and the
            //    `Start(Paragraph)` re-opening the rest. These are owed
            //    before whatever the boundary itself emits — an
            //    `End(BlockQuote)`/`End(Item)` rides at the back of this queue
            //    behind them (see `translate`) — so they drain first, before
            //    anything held in `deferred`. Emitted straight out, not through
            //    `dispatch`: none of them coalesces, and the boundary `End`
            //    only reaches this queue once the run enclosed by the block
            //    has already drained.
            if let Some(event) = self.owed.pop_front() {
                return Some(event);
            }

            // 1. Then the run — one segment per call, text and inline
            //    directives interleaved.
            //
            // Two steps, not one: `take_run_segment` returns owned data, which
            // ends its `&mut self` borrow, and only then is the buffer
            // reborrowed to lend the slice. A `-> Option<Event<'_>>` helper
            // would instead reborrow `*self` for the whole of `next`'s return
            // lifetime, which NLL cannot release on the `None` path (the
            // `get_default` case), so the loop below would stop compiling.
            if matches!(self.deferred, Deferred::Draining(_)) && self.run_cursor < self.run.len() {
                let segment = self.take_run_segment();
                return Some(run_segment_event(&self.run, segment));
            }

            // 2. Then the event that interrupted the run. Re-dispatched rather
            //    than returned blindly: with the run now drained, a held
            //    `Start(Paragraph)` still has to open its own deferral.
            match std::mem::replace(&mut self.deferred, Deferred::None) {
                Deferred::Draining(event) => {
                    if let Some(event) = self.dispatch(event) {
                        return Some(event);
                    }
                    continue;
                }
                other => self.deferred = other,
            }

            // 3. Nothing owed: pull from cmark.
            let Some((event, range)) = self.inner.next() else {
                // Defensive, and believed unreachable: no input has been found
                // that arrives here with the run non-empty, and deleting the
                // drain fails no test. cmark's last event is always an `End`,
                // and an `End` either reaches `dispatch` — which drains the run
                // before the event — or is one of the two the Parser drops
                // (`HtmlBlock`, `FootnoteDefinition`) — and those close over
                // content that either never buffers (an html block's body
                // arrives as `Html`, not `Text`) or has already been flushed by
                // the inner `End(Paragraph)`. Kept anyway: there is no finish
                // hook to fall back on, so a hole in that argument would
                // silently truncate a document's last run.
                //
                // Re-polling `inner` after it returned `None` is sound: it is
                // fused.
                if self.run_cursor < self.run.len() {
                    let segment = self.take_run_segment();
                    return Some(run_segment_event(&self.run, segment));
                }
                // Every container the author left open at EOF is synthesized
                // closed, innermost first, before the stream ends. Enqueue them
                // and loop: the top-of-`next` drain emits them in order.
                if self.open_containers.is_empty() {
                    return None;
                }
                self.enqueue_boundary_closes(0);
                continue;
            };
            if let Some(event) = self.translate(event, range.start)
                && let Some(event) = self.dispatch(event)
            {
                return Some(event);
            }
        }
    }

    /// Cut the next segment out of the run and advance the cursor past it —
    /// one segment per call.
    ///
    /// Text preceding a directive is lent first, the directive itself parked in
    /// [`pending`](Self::pending) until the call after.
    ///
    /// Only called with `run_cursor < run.len()`.
    fn take_run_segment(&mut self) -> RunSegment {
        if let Some(matched) = self.pending.take() {
            self.run_cursor = matched.range.end;
            return matched.into();
        }

        let from = self.run_cursor;
        let len = self.run.len();
        debug_assert!(from < len, "take_run_segment with nothing left to lend");

        // Directive syntax off: the run is prose, lent whole. Gating the scan
        // rather than its result is a performance choice, not a behavioural
        // one — scanning and then discarding the match yields this same
        // stream, only slower.
        if !self.directives {
            self.run_cursor = len;
            return RunSegment::Text(from..len);
        }

        let escapes = Escapes::new(&self.escapes, from);
        let Some(InlineMatch { directive, range }) = parse_escaped_line(&self.run[from..], escapes)
        else {
            self.run_cursor = len;
            return RunSegment::Text(from..len);
        };

        let matched = InlineMatch {
            directive,
            range: from + range.start..from + range.end,
        };
        let start = matched.range.start;

        if start > from {
            // The cursor stops short of the directive, which the slot now
            // owns: recycling is gated on the cursor reaching the end of the
            // run.
            self.run_cursor = start;
            self.pending = Some(matched);
            return RunSegment::Text(from..start);
        }

        self.run_cursor = matched.range.end;
        matched.into()
    }

    /// Route one translated event: return it, or absorb it into the run and
    /// the deferral state.
    ///
    /// # Why the inlining is pinned here and on [`start_tag`](Self::start_tag)
    ///
    /// [`Event`] is 104 bytes, and `Option<Event>` is 104 too — its
    /// discriminant rides in a spare niche — so every hand-off between `next`,
    /// `translate`, `dispatch` and `start_tag` that LLVM does not collapse is a
    /// real 104-byte `sret` copy, and `next` runs once per emitted event, which
    /// is the renderer's innermost loop. Left to itself LLVM makes the opposite
    /// choice on both counts: it outlines `dispatch` (so the event `translate`
    /// just built is copied out and straight back in, twice per event) while
    /// inlining the sprawling `start_tag` (whose `cmark::Tag` match is the
    /// widest thing in the function, inflating `next`'s stack frame to ~1.5 KB
    /// and its prologue to twelve callee-saved registers, paid on *every* call
    /// including the common `Text` one).
    ///
    /// Pinning both directions is what pays: measured on the two bench
    /// fixtures, either attribute alone is worth under 1% — inside the noise —
    /// while the pair together is 2.5-3%. That is the shape to expect, since
    /// `inline(always)` here grows `next` and the `inline(never)` below is what
    /// buys the space back.
    #[inline(always)]
    #[allow(
        clippy::inline_always,
        reason = "measured: see the pairing note above, not a hunch"
    )]
    fn dispatch(&mut self, event: Event<'a>) -> Option<Event<'a>> {
        // Adjacent text coalesces: cmark splits a run at every `[`, `]`,
        // entity and escape, and rw's directive syntax is only recognizable
        // in the joined form.
        if let Event::Text(text) = &event {
            self.run.push_str(text);
            return None;
        }

        if matches!(self.deferred, Deferred::Paragraph | Deferred::LineBreak(_)) {
            return Some(self.resolve_held(event));
        }

        // Hold the `<p>`: the paragraph may turn out to be a block-directive
        // delimiter, which emits none.
        if matches!(event, Event::Start(Tag::Paragraph))
            && self.directives
            && self.run.is_empty()
            && colon_line_indent(self.source, self.paragraph_at, 0).is_some()
        {
            self.deferred = Deferred::Paragraph;
            return None;
        }

        if matches!(event, Event::SoftBreak | Event::HardBreak)
            && self.directives
            && self.split_depth == 0
            && self.run.is_empty()
            && self.line_after_break_is_colon_led()
        {
            self.deferred = Deferred::LineBreak(event);
            return None;
        }

        if self.run.is_empty() {
            Some(event)
        } else {
            self.deferred = Deferred::Draining(event);
            None
        }
    }

    /// Route the event that follows a held `<p>` or line break: it either
    /// ends a directive line, or releases what was held, returning it now
    /// and owing the run and `event` after it.
    ///
    /// Cold and out of line: only a line whose source starts with `:` is
    /// held, and inlined this would grow [`next`](Self::next)'s frame (see
    /// the pairing note on [`dispatch`](Self::dispatch)).
    #[cold]
    #[inline(never)]
    fn resolve_held(&mut self, event: Event<'a>) -> Event<'a> {
        let ends_paragraph = matches!(event, Event::End(TagEnd::Paragraph));
        let ends_line = ends_paragraph || matches!(event, Event::SoftBreak | Event::HardBreak);
        match std::mem::replace(&mut self.deferred, Deferred::None) {
            Deferred::Paragraph => {
                if ends_paragraph {
                    return self.decide_block();
                }
                // The run is the paragraph's first line. As a directive it
                // leaves the rest of the paragraph to start after it.
                if ends_line && let Some(directive) = self.take_directive_line(true) {
                    self.continue_paragraph();
                    return directive;
                }
                self.deferred = Deferred::Draining(event);
                Event::Start(Tag::Paragraph)
            }
            Deferred::LineBreak(held) => {
                if ends_line && let Some(directive) = self.take_directive_line(true) {
                    self.owed.push_back(directive);
                    if !ends_paragraph {
                        self.continue_paragraph();
                    }
                    return Event::End(TagEnd::Paragraph);
                }
                self.deferred = Deferred::Draining(event);
                held
            }
            _ => unreachable!("only called while a paragraph or line break is held"),
        }
    }

    /// Decide, at `End(Paragraph)`, whether the coalesced run is a block
    /// directive or an ordinary paragraph.
    ///
    /// The decision has to live here, next to the run: cmark splits
    /// `:::tab[Label]{#a}` into five `Text` events, so only the joined form is
    /// recognizable, which is why the paragraph is held back until then. Only
    /// a paragraph whose source starts with `:` is held.
    fn decide_block(&mut self) -> Event<'a> {
        self.deferred = Deferred::None;
        debug_assert!(
            self.pending.is_none(),
            "the run is fully drained before cmark is pulled again, so no segment can still index it"
        );
        if let Some(event) = self.take_directive_line(false) {
            return event;
        }

        // An ordinary paragraph: `<p>`, then the run, then `</p>`.
        self.deferred = Deferred::Draining(Event::End(TagEnd::Paragraph));
        Event::Start(Tag::Paragraph)
    }

    /// Whether the line after the line break at [`break_at`](Self::break_at)
    /// starts with `:` and is no lazy continuation.
    ///
    /// In a list item, a line indented less than the paragraph's first text
    /// is lazy: markdown lets it continue the paragraph, but it belongs
    /// after the list, not inside the item.
    fn line_after_break_is_colon_led(&self) -> bool {
        let rest = &self.source[self.break_at..];
        let next_line = self.break_at + rest.find('\n').map_or(rest.len(), |i| i + 1);
        let Some(indent) = colon_line_indent(self.source, next_line, self.quote_depth) else {
            return false;
        };
        let in_list = self.block_depth > self.quote_depth;
        !in_list || indent >= text_indent(self.source, self.paragraph_at)
    }

    /// Start the paragraph that continues after a directive line split out
    /// of it, holding it only if its first line could be a directive too.
    fn continue_paragraph(&mut self) {
        if self.line_after_break_is_colon_led() {
            self.deferred = Deferred::Paragraph;
        } else {
            self.deferred = Deferred::None;
            self.owed.push_back(Event::Start(Tag::Paragraph));
        }
    }

    /// Consume the run if it is one whole block-directive line; otherwise
    /// leave it untouched.
    ///
    /// A `glued` line shares its paragraph with other lines, where prose
    /// starting with colons is likelier: there a closer must close something,
    /// and a leaf needs a `[…]` or `{…}` group and a name that starts with a
    /// letter, so `::before` and `::1` stay text.
    ///
    /// Only called on a run nothing has been lent from.
    fn take_directive_line(&mut self, glued: bool) -> Option<Event<'a>> {
        // Taken, not borrowed: `container_line_event` needs `&mut self`
        // while the line is still being parsed.
        let mut text = std::mem::take(&mut self.run);
        let escapes = Escapes::new(&self.escapes, 0);
        let matched = match parse_container_line(&text, escapes) {
            Some(ContainerLine::End { .. }) if glued && !self.closer_pairs() => None,
            Some(line) => Some(self.container_line_event(line)),
            None => parse_leaf_line(&text, escapes)
                .filter(|leaf| {
                    !glued
                        || leaf.after_colons.len() > leaf.name.len()
                            && leaf.name.starts_with(char::is_alphabetic)
                })
                .map(Event::LeafDirective),
        };
        if matched.is_some() {
            text.clear();
            self.escapes.clear();
        }
        // Hand the allocation back: the `mem::take` above left a
        // zero-capacity run, which would otherwise re-grow from empty.
        self.run = text;
        matched
    }

    /// Translate one cmark event, or absorb it into Parser-side state.
    ///
    /// Returns a source-borrowed `Event<'a>`; the coercion down to the shorter
    /// `&mut self` lifetime happens on return from [`next`](Self::next).
    fn translate(&mut self, event: cmark::Event<'a>, start: usize) -> Option<Event<'a>> {
        // A metadata block is swallowed whole, its text included, so the
        // directive scanner never sees YAML.
        if self.in_metadata {
            if matches!(event, cmark::Event::End(cmark::TagEnd::MetadataBlock(_))) {
                self.in_metadata = false;
            }
            return None;
        }

        if self.code_block.is_some() {
            return self.accumulate_code_block(event);
        }

        match event {
            cmark::Event::Start(tag) => self.start_tag(tag, start),
            cmark::Event::End(tag) => {
                let translated = self.end_tag(tag).map(Event::End);
                // `end_tag` may have synthesized container closes owed *before*
                // this boundary End (a blockquote or list item closing over an
                // unclosed container). If so, hold the End at the back of the
                // queue and return `None`: `next` loops, and its top-of-loop
                // drain emits the closes first, then the End.
                if !self.owed.is_empty() {
                    if let Some(event) = translated {
                        self.owed.push_back(event);
                    }
                    return None;
                }
                translated
            }
            cmark::Event::Text(text) => {
                if self.skip_wikilink_text {
                    self.skip_wikilink_text = false;
                    return None;
                }
                // cmark drops an escape's backslash and starts a new `Text` at
                // the escaped character, and decodes an entity into a `Text`
                // of its own that starts at the `&`. `dispatch` appends this
                // text to the run next, so its first byte lands at the run's
                // current end. A backslash before `escaped_until` is escaped
                // content and escapes nothing: in `:kbd[\\]` the `]` is live.
                debug_assert!(
                    self.escaped_until <= start,
                    "cmark yields text in source order"
                );
                if start > self.escaped_until && self.source.as_bytes()[start - 1] == b'\\' {
                    self.escaped_until = start + 1;
                    self.escapes.push(self.run.len());
                } else if self.source.as_bytes().get(start) == Some(&b'&') {
                    self.escapes.push(self.run.len());
                }
                Some(Event::Text(text))
            }
            cmark::Event::Code(code) => Some(Event::Code(code)),
            // Block and inline raw HTML are one event: rw renders both
            // identically.
            cmark::Event::Html(html) | cmark::Event::InlineHtml(html) => Some(Event::RawHtml(html)),
            cmark::Event::SoftBreak => {
                self.break_at = start;
                Some(Event::SoftBreak)
            }
            cmark::Event::HardBreak => {
                self.break_at = start;
                Some(Event::HardBreak)
            }
            cmark::Event::Rule => Some(Event::Rule),
            cmark::Event::TaskListMarker(checked) => Some(Event::TaskListMarker(checked)),
            cmark::Event::FootnoteReference(_)
            | cmark::Event::InlineMath(_)
            | cmark::Event::DisplayMath(_) => {
                // `cmark_options` enables neither `ENABLE_FOOTNOTES` nor
                // `ENABLE_MATH`, so cmark cannot emit these. Verified against
                // a document containing all three syntaxes.
                debug_assert!(
                    false,
                    "cmark emitted a footnote/math event, which rw's parser options never enable"
                );
                None
            }
        }
    }

    /// Feed one event into the open code block's accumulator, or close it.
    ///
    /// Only ever called with `self.code_block.is_some()`.
    fn accumulate_code_block(&mut self, event: cmark::Event<'a>) -> Option<Event<'a>> {
        let accum = self
            .code_block
            .as_mut()
            .expect("checked by caller: code_block is Some");
        match event {
            cmark::Event::Text(text) => accum.buffer.push_str(&text),
            cmark::Event::SoftBreak => accum.buffer.push('\n'),
            cmark::Event::End(cmark::TagEnd::CodeBlock) => {
                let CodeBlockAccum {
                    language,
                    attrs,
                    buffer,
                } = self
                    .code_block
                    .take()
                    .expect("checked by caller: code_block is Some");
                return Some(Event::CodeBlock(CodeBlockPayload {
                    language,
                    attrs,
                    source: buffer,
                }));
            }
            _ => {
                // Dormant: between a code block's start and end, cmark emits
                // nothing but text.
                debug_assert!(false, "unexpected event inside a code block");
            }
        }
        None
    }

    /// Translate an opening tag, or absorb the ones no consumer ever sees.
    ///
    /// `#[inline(never)]`: keeping this match out of
    /// [`next`](Self::next) is half of the pair documented on
    /// [`dispatch`](Self::dispatch).
    #[inline(never)]
    fn start_tag(&mut self, tag: cmark::Tag<'a>, start: usize) -> Option<Event<'a>> {
        if matches!(
            tag,
            cmark::Tag::Emphasis
                | cmark::Tag::Strong
                | cmark::Tag::Strikethrough
                | cmark::Tag::Superscript
                | cmark::Tag::Subscript
                | cmark::Tag::Link { .. }
                | cmark::Tag::Image { .. }
        ) {
            self.split_depth += 1;
        }
        let translated = match tag {
            cmark::Tag::Paragraph => {
                self.split_depth -= 1;
                self.paragraph_at = start;
                Tag::Paragraph
            }
            cmark::Tag::Heading { level, .. } => Tag::Heading {
                level: heading_level_to_num(level),
            },
            cmark::Tag::BlockQuote(kind) => {
                // A blockquote deepens block nesting: a container opened inside
                // it must be closed at this level's `End(BlockQuote)`.
                self.block_depth += 1;
                self.quote_depth += 1;
                Tag::BlockQuote(kind.map(AlertKind::from))
            }
            cmark::Tag::CodeBlock(kind) => {
                let (language, attrs) = match kind {
                    CodeBlockKind::Fenced(ref info) if !info.is_empty() => {
                        let (lang, attrs) = parse_fence_info(info);
                        (if lang.is_empty() { None } else { Some(lang) }, attrs)
                    }
                    _ => (None, FenceAttrs::default()),
                };
                self.code_block = Some(CodeBlockAccum {
                    language,
                    attrs,
                    buffer: String::new(),
                });
                return None;
            }
            cmark::Tag::MetadataBlock(_) => {
                self.in_metadata = true;
                return None;
            }
            // Nothing rw renders reacts to these. An HTML block's raw
            // contents still arrive, as `Event::RawHtml`; footnote definitions
            // cannot occur at all (`ENABLE_FOOTNOTES` is never set).
            //
            // Absorbing them is safe only because an HTML block always emits
            // at least one `Html` event between its tags, and that event
            // drains the run. An empty `HtmlBlock` pair — or any new
            // event type added to this arm — would let two runs from different
            // blocks coalesce, and `parse_line` could then synthesize a
            // directive spanning the block boundary. Drain explicitly before
            // absorbing if you extend this list.
            cmark::Tag::HtmlBlock | cmark::Tag::FootnoteDefinition(_) => return None,
            cmark::Tag::List(start) => {
                // A list deepens block nesting (the list, not its items): a
                // container opened inside an item is closed at that item's
                // `End(Item)`, computed against this depth.
                self.block_depth += 1;
                Tag::List(start)
            }
            cmark::Tag::Item => Tag::Item,
            cmark::Tag::DefinitionList => Tag::DefinitionList,
            cmark::Tag::DefinitionListTitle => Tag::DefinitionListTitle,
            cmark::Tag::DefinitionListDefinition => Tag::DefinitionListDefinition,
            // Moved, never copied: the alignments vector is cmark's, and the
            // consumer needs the alignments exactly as cmark computed them.
            cmark::Tag::Table(alignments) => Tag::Table(alignments),
            cmark::Tag::TableHead => Tag::TableHead,
            cmark::Tag::TableRow => Tag::TableRow,
            cmark::Tag::TableCell => Tag::TableCell,
            cmark::Tag::Emphasis => Tag::Emphasis,
            cmark::Tag::Strong => Tag::Strong,
            cmark::Tag::Strikethrough => Tag::Strikethrough,
            cmark::Tag::Superscript => Tag::Superscript,
            cmark::Tag::Subscript => Tag::Subscript,
            // No flag test here: cmark produces `LinkType::WikiLink` only when
            // `ENABLE_WIKILINKS` is set, and `cmark_options` sets it from the
            // caller's `wikilinks`. Reaching this arm therefore *is* the
            // wikilinks-enabled case.
            cmark::Tag::Link {
                link_type: cmark::LinkType::WikiLink { has_pothole },
                dest_url,
                ..
            } => {
                if !has_pothole {
                    self.skip_wikilink_text = true;
                }
                Tag::Link {
                    kind: LinkKind::Wiki { has_pothole },
                    dest_url,
                }
            }
            cmark::Tag::Link { dest_url, .. } => Tag::Link {
                kind: LinkKind::Other,
                dest_url,
            },
            cmark::Tag::Image {
                dest_url, title, ..
            } => Tag::Image { dest_url, title },
        };
        Some(Event::Start(translated))
    }

    /// Translate a closing tag. `None` for the ends whose starts the Parser
    /// absorbed — a code block or metadata block reaching here is malformed.
    ///
    /// Takes `&mut self` to maintain [`block_depth`](Self::block_depth) and to
    /// synthesize container closes at a block boundary. A blockquote or list
    /// item closing takes any container the author left open inside it down with
    /// it — innermost first — so the container's markup never crosses the
    /// block's own closing tag. Depth counts blockquote and list levels (not
    /// items); the closing quote/list is still counted when its containers are
    /// collected, then decremented.
    fn end_tag(&mut self, tag: cmark::TagEnd) -> Option<TagEnd> {
        if matches!(tag, cmark::TagEnd::BlockQuote(_) | cmark::TagEnd::Item) {
            self.enqueue_boundary_closes(self.block_depth);
        }
        if matches!(tag, cmark::TagEnd::BlockQuote(_) | cmark::TagEnd::List(_)) {
            self.block_depth -= 1;
        }
        if matches!(
            tag,
            cmark::TagEnd::Emphasis
                | cmark::TagEnd::Strong
                | cmark::TagEnd::Strikethrough
                | cmark::TagEnd::Superscript
                | cmark::TagEnd::Subscript
                | cmark::TagEnd::Link
                | cmark::TagEnd::Image
        ) {
            self.split_depth -= 1;
        }
        let translated = match tag {
            cmark::TagEnd::Paragraph => {
                self.split_depth += 1;
                TagEnd::Paragraph
            }
            cmark::TagEnd::Heading(_) => TagEnd::Heading,
            cmark::TagEnd::BlockQuote(kind) => {
                self.quote_depth -= 1;
                TagEnd::BlockQuote(kind.map(AlertKind::from))
            }
            cmark::TagEnd::List(ordered) => TagEnd::List(ordered),
            cmark::TagEnd::Item => TagEnd::Item,
            cmark::TagEnd::DefinitionList => TagEnd::DefinitionList,
            cmark::TagEnd::DefinitionListTitle => TagEnd::DefinitionListTitle,
            cmark::TagEnd::DefinitionListDefinition => TagEnd::DefinitionListDefinition,
            cmark::TagEnd::Table => TagEnd::Table,
            cmark::TagEnd::TableHead => TagEnd::TableHead,
            cmark::TagEnd::TableRow => TagEnd::TableRow,
            cmark::TagEnd::TableCell => TagEnd::TableCell,
            cmark::TagEnd::Emphasis => TagEnd::Emphasis,
            cmark::TagEnd::Strong => TagEnd::Strong,
            cmark::TagEnd::Strikethrough => TagEnd::Strikethrough,
            cmark::TagEnd::Superscript => TagEnd::Superscript,
            cmark::TagEnd::Subscript => TagEnd::Subscript,
            cmark::TagEnd::Link => TagEnd::Link,
            cmark::TagEnd::Image => TagEnd::Image,
            cmark::TagEnd::HtmlBlock | cmark::TagEnd::FootnoteDefinition => return None,
            cmark::TagEnd::CodeBlock => {
                debug_assert!(false, "TagEnd::CodeBlock without an open code block");
                return None;
            }
            cmark::TagEnd::MetadataBlock(_) => {
                debug_assert!(
                    false,
                    "TagEnd::MetadataBlock without an open metadata block"
                );
                return None;
            }
        };
        Some(translated)
    }

    /// Enqueue synthesized closes for every open container at `depth` or deeper,
    /// innermost first. Used at block boundaries and at EOF (`depth == 0`, which
    /// flushes the whole stack). Each carries its opener's name so a consumer
    /// can report which container was left open; `colon_count` is `0` because a
    /// synthesized close corresponds to no source text.
    fn enqueue_boundary_closes(&mut self, depth: usize) {
        while self
            .open_containers
            .last()
            .is_some_and(|c| c.block_depth >= depth)
        {
            let c = self.open_containers.pop().expect("checked by last()");
            self.owed.push_back(Event::ContainerDirectiveEnd {
                name: Some(c.name),
                colon_count: 0,
                implicit: true,
            });
        }
    }
}

/// The indentation after the last blockquote marker of the source line at
/// `at`, if past it the line starts with `:` and it has at least `quotes`
/// markers.
///
/// Reading the source rather than the run keeps an entity (`&#58;::name`) or
/// an escape from forming the leading colon, and holds nothing back for
/// prose. Too few `>` marks a lazy continuation line, which belongs to the
/// quote's paragraph however it reads.
fn colon_line_indent(source: &str, at: usize, quotes: usize) -> Option<usize> {
    let (mut seen, mut indent) = (0, 0);
    for &b in &source.as_bytes()[at..] {
        match b {
            b'>' => (seen, indent) = (seen + 1, 0),
            b' ' | b'\t' => indent += 1,
            b':' => return (seen >= quotes).then_some(indent),
            _ => return None,
        }
    }
    None
}

/// How far the text at `at` sits from its line's start, or from the last
/// blockquote marker before it: a list item's content column.
fn text_indent(source: &str, at: usize) -> usize {
    at - source[..at].rfind(['\n', '>']).map_or(0, |i| i + 1)
}

/// Lend one resolved [`RunSegment`] as an event.
///
/// Free-standing and taking the buffer as a slice: it must borrow the run
/// without reborrowing the `Parser`, so that the caller's `&mut self` is
/// already released. Every segment stays `CowStr::Borrowed` — the whole point
/// of the lending `next`.
fn run_segment_event(run: &str, segment: RunSegment) -> Event<'_> {
    match segment {
        RunSegment::Text(range) => Event::Text(CowStr::Borrowed(&run[range])),
        RunSegment::Inline { name, args, raw } => Event::InlineDirective(InlineDirectivePayload {
            name,
            args,
            raw: CowStr::Borrowed(&run[raw]),
        }),
    }
}

impl<'a> Parser<'a> {
    /// Project a `:::` line onto the event vocabulary, maintaining the open
    /// stack so a closer carries its opener's name.
    fn container_line_event(&mut self, line: ContainerLine) -> Event<'a> {
        match line {
            ContainerLine::Start {
                payload,
                colon_count,
            } => {
                self.open_containers.push(OpenContainer {
                    name: payload.name.clone(),
                    block_depth: self.block_depth,
                });
                Event::ContainerDirectiveStart {
                    payload,
                    colon_count,
                }
            }
            ContainerLine::End { colon_count } => {
                let name = if self.closer_pairs() {
                    self.open_containers.pop().map(|c| c.name)
                } else {
                    None
                };
                Event::ContainerDirectiveEnd {
                    name,
                    colon_count,
                    implicit: false,
                }
            }
        }
    }

    /// Whether a closer here closes the innermost open container.
    ///
    /// Not from inside a blockquote or list item the container encloses: that
    /// would leave the block's own end tag outside the container's.
    fn closer_pairs(&self) -> bool {
        self.open_containers
            .last()
            .is_some_and(|c| c.block_depth == self.block_depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, Tag, TagEnd};

    /// `ENABLE_HEADING_ATTRIBUTES` makes cmark claim a trailing `{…}` on a
    /// heading line as heading metadata — the same braces a directive uses for
    /// its attributes. With it on, `# a :status[X]{color=green}` reaches the
    /// directive scanner already stripped of `{color=green}`, and the badge
    /// silently loses its colour.
    ///
    /// Renderer tests would catch that, but only as a rendering failure three
    /// crates away. What they would *not* catch is the flag arriving on one
    /// arm only: every one of them runs with wikilinks off, so setting it
    /// inside the `wikilinks` branch passes the whole suite. Hence the loop,
    /// and hence asserting the dialect here rather than inferring it from
    /// output.
    #[test]
    fn cmark_options_leaves_heading_attributes_off() {
        for wikilinks in [false, true] {
            assert!(!cmark_options(wikilinks).contains(Options::ENABLE_HEADING_ATTRIBUTES));
        }
    }

    /// Collect the whole stream, owning nothing borrowed — `next` lends, so
    /// each event must be consumed before the next call.
    fn debug_stream(markdown: &str) -> Vec<String> {
        let mut parser = Parser::new(markdown, false, true);
        let mut out = Vec::new();
        while let Some(event) = parser.next() {
            out.push(format!("{event:?}"));
        }
        out
    }

    #[test]
    fn a_fence_arrives_as_one_code_block_event_with_its_body() {
        let mut parser = Parser::new("```rust\nfn a() {}\n```\n", false, true);
        let mut seen = None;
        while let Some(event) = parser.next() {
            if let Event::CodeBlock(payload) = event {
                seen = Some((payload.language.clone(), payload.source.clone()));
            }
        }
        assert_eq!(
            seen,
            Some((Some("rust".to_owned()), "fn a() {}\n".to_owned()))
        );
    }

    #[test]
    fn a_metadata_block_is_swallowed_whole() {
        let stream = debug_stream("---\ntitle: T\n---\n\nbody\n");
        assert!(
            !stream.iter().any(|e| e.contains("title: T")),
            "metadata text must never be emitted: {stream:?}"
        );
        assert!(stream.iter().any(|e| e.contains("body")));
    }

    #[test]
    fn structure_translates_to_rw_tags() {
        let mut parser = Parser::new("# H\n", false, true);
        assert_eq!(parser.next(), Some(Event::Start(Tag::Heading { level: 1 })));
        assert_eq!(parser.next(), Some(Event::Text("H".into())));
        assert_eq!(parser.next(), Some(Event::End(TagEnd::Heading)));
        assert_eq!(parser.next(), None);
    }

    fn debug_stream_wikilinks(markdown: &str) -> Vec<String> {
        let mut parser = Parser::new(markdown, true, true);
        let mut out = Vec::new();
        while let Some(event) = parser.next() {
            out.push(format!("{event:?}"));
        }
        out
    }

    #[test]
    fn a_potholeless_wikilink_swallows_the_display_text_cmark_supplies() {
        // rw resolves its own display text from the link target, so
        // cmark's must not also arrive.
        let stream = debug_stream_wikilinks("[[target]]");
        assert!(
            !stream
                .iter()
                .any(|e| e.contains("Text(") && e.contains("target")),
            "cmark's wikilink text must be swallowed: {stream:?}"
        );
    }

    #[test]
    fn a_wikilink_with_a_pothole_keeps_its_text() {
        let stream = debug_stream_wikilinks("[[target|Display]]");
        assert!(
            stream.iter().any(|e| e.contains("Display")),
            "explicit display text is cmark's to supply: {stream:?}"
        );
    }

    #[test]
    fn suppression_is_one_shot() {
        // Only the wikilink's own text is swallowed; the prose after it stays.
        let stream = debug_stream_wikilinks("[[target]] and more prose");
        assert!(
            stream.iter().any(|e| e.contains("and more prose")),
            "suppression must not leak past the link: {stream:?}"
        );
    }

    #[test]
    fn wikilink_syntax_stays_prose_when_wikilinks_are_off() {
        // `start_tag`'s wikilink arm tests no flag of its own: it relies on
        // cmark emitting `LinkType::WikiLink` only when `cmark_options` set
        // `ENABLE_WIKILINKS`. That invariant is pulldown-cmark's, not rw's, so
        // pin it here — a cmark release that emitted a wikilink with the option
        // off would otherwise turn `[[target]]` into a link on every site that
        // has wikilinks disabled.
        let stream = debug_stream("[[target]]");
        assert!(
            !stream.iter().any(|e| e.contains("Wiki")),
            "wikilinks are off, so no Wiki link may be emitted: {stream:?}"
        );
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("[[target]]"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn a_split_directive_line_coalesces_before_the_block_decision() {
        // cmark splits `:::tab[Label]{#a}` at every `[`, `]` and `{`. The
        // decision is made against the whole run, so a `<p>` must never be
        // emitted for it.
        let stream = debug_stream(":::tab[Label]{#a}\n\nbody\n\n:::\n");
        assert!(
            stream
                .iter()
                .any(|e| e.starts_with("ContainerDirectiveStart")),
            "expected a container start: {stream:?}"
        );
        assert!(
            !stream.iter().take(1).any(|e| e.contains("Paragraph")),
            "the suppressed paragraph must never be emitted: {stream:?}"
        );
    }

    #[test]
    fn a_paragraph_releases_p_then_run_then_trigger() {
        // For `:foo **bold**` the order owed is <p>, then the run,
        // then the trigger — three items, which is why deferral is a state
        // machine and not a slot.
        let stream = debug_stream(":foo **bold**\n");
        let start = stream.iter().position(|e| e.contains("Paragraph")).unwrap();
        let text = stream.iter().position(|e| e.contains(":foo")).unwrap();
        let strong = stream.iter().position(|e| e.contains("Strong")).unwrap();
        assert!(start < text && text < strong, "wrong order: {stream:?}");
    }

    #[test]
    fn a_container_end_keeps_its_colon_count() {
        let stream = debug_stream(&format!("{}\n", ":".repeat(300)));
        assert!(
            stream.iter().any(|e| e.contains("colon_count: 300")),
            "a 300-colon closer must survive: {stream:?}"
        );
    }

    #[test]
    fn a_container_start_reports_its_colon_count() {
        let mut parser = Parser::new("::::note\n", false, true);
        let Some(Event::ContainerDirectiveStart {
            payload,
            colon_count,
        }) = parser.next()
        else {
            panic!("expected a container start");
        };
        assert_eq!(payload.name, "note");
        assert_eq!(colon_count, 4);
    }

    #[test]
    fn an_inline_directive_is_split_out_of_its_run() {
        let stream = debug_stream("Press :kbd[Ctrl+C] to copy.");
        assert_eq!(
            stream.iter().filter(|e| e.starts_with("Text(")).count(),
            2,
            "run splits around the directive: {stream:?}"
        );
        assert!(stream.iter().any(|e| e.starts_with("InlineDirective")));
    }

    #[test]
    fn a_run_ending_in_a_directive_keeps_its_buffer_alive() {
        // The parked directive holds a range into the run, and the run is
        // recycled once the cursor reaches its end — so the cursor must stop
        // *before* a parked segment, never past it. Parking it past the
        // directive instead clears the buffer out from under this slice.
        let stream = debug_stream("a :one[1] b :two[2]");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("a "))"#,
                r#"InlineDirective(InlineDirectivePayload { name: "one", args: DirectiveArgs { content: "1", attrs: [] }, raw: Borrowed(":one[1]") })"#,
                r#"Text(Borrowed(" b "))"#,
                r#"InlineDirective(InlineDirectivePayload { name: "two", args: DirectiveArgs { content: "2", attrs: [] }, raw: Borrowed(":two[2]") })"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn directives_off_keeps_directive_syntax_as_prose() {
        // A pipeline with no processor cannot dispatch a directive event, so
        // `:kbd[…]` must be emitted as ordinary text.
        //
        // This pins the *result* only. `take_run_segment` gates the scan
        // itself — it returns before calling `parse_line` — but a gate placed
        // after the scan would produce this same stream, only slower, so the
        // placement is a performance property no black-box assertion here can
        // enforce. Named for what it checks rather than for the gate.
        let mut parser = Parser::new("Press :kbd[Ctrl+C] to copy.", false, false);
        let mut out = Vec::new();
        while let Some(event) = parser.next() {
            out.push(format!("{event:?}"));
        }
        assert_eq!(
            out,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("Press :kbd[Ctrl+C] to copy."))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn an_inline_directive_carries_its_byte_exact_raw_slice() {
        // Asserted against the payload rather than `debug_stream`, whose
        // `Debug` output escapes the `"` in `key="v"` and so could never
        // contain the byte-exact slice.
        let raw = r#":foo[x]{.a.b key="v" bareword}"#;
        let markdown = format!("before {raw} after");
        let mut parser = Parser::new(&markdown, false, true);
        let mut seen = None;
        while let Some(event) = parser.next() {
            if let Event::InlineDirective(payload) = event {
                seen = Some(payload.raw.to_string());
            }
        }
        assert_eq!(seen.as_deref(), Some(raw), "raw must be byte-exact");
    }

    #[test]
    fn a_document_ending_mid_text_yields_its_final_text_before_none() {
        // There is no finish hook, so a document whose last bytes are
        // text has to be drained on the way out or its tail is lost.
        //
        // Which drain carries it is not pinned here, and deliberately so: on
        // today's cmark the run is released by `End(Paragraph)` (cmark closes
        // every block it opens, so its last event is always an `End`), and the
        // separate drain on cmark exhaustion in `next` is defensive — deleting
        // it fails no test in this crate, and no input we could construct
        // reaches it. What must hold, by whichever path, is that the tail is
        // not truncated.
        let stream = debug_stream("trailing text with no terminator");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("trailing text with no terminator"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn an_inline_directive_alone_in_a_paragraph_is_not_taken_for_a_block_one() {
        // The paragraph is offered to the block parsers first (its trimmed
        // text starts with `:`); both must decline, leaving an ordinary
        // paragraph whose run is then scanned for inline directives.
        let mut parser = Parser::new(":status[On Track]{color=green}", false, true);
        assert_eq!(parser.next(), Some(Event::Start(Tag::Paragraph)));
        let Some(Event::InlineDirective(payload)) = parser.next() else {
            panic!("expected an inline directive");
        };
        assert_eq!(payload.name, "status");
        assert_eq!(&*payload.raw, ":status[On Track]{color=green}");
        assert_eq!(parser.next(), Some(Event::End(TagEnd::Paragraph)));
        assert_eq!(parser.next(), None);
    }

    #[test]
    fn a_directive_split_at_an_entity_is_recoalesced() {
        // cmark cuts `&amp;` into its own `Text` event, splitting the
        // directive's content in three. The run is what the scanner sees, so
        // `raw` is the *decoded* slice (`&`), not the source bytes (`&amp;`).
        let stream = debug_stream("See :note[a &amp; b] end");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("See "))"#,
                r#"InlineDirective(InlineDirectivePayload { name: "note", args: DirectiveArgs { content: "a & b", attrs: [] }, raw: Borrowed(":note[a & b]") })"#,
                r#"Text(Borrowed(" end"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn a_directive_split_at_an_escape_is_recoalesced() {
        // `\[` and `\]` each arrive as their own `Text` event. Joined, the
        // escaped brackets are ordinary content characters — the directive's
        // own brackets are the outermost pair.
        let stream = debug_stream(r"See :note[a \[b\] c] end");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("See "))"#,
                r#"InlineDirective(InlineDirectivePayload { name: "note", args: DirectiveArgs { content: "a [b] c", attrs: [] }, raw: Borrowed(":note[a [b] c]") })"#,
                r#"Text(Borrowed(" end"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn a_tab_container_opens_and_closes_around_its_body() {
        // cmark splits the opener into five `Text` events (at `[`, at `]`, and
        // again before `{`) and puts both delimiters in paragraphs of their
        // own. What is emitted is one start, the body, one end — and
        // no paragraph tags for either delimiter.
        let stream = debug_stream(":::tab[Label]{#a}\n\nbody\n\n:::\n");
        assert_eq!(
            stream,
            [
                r#"ContainerDirectiveStart { payload: BlockDirectivePayload { name: "tab", args: DirectiveArgs { content: "Label", attrs: [] }, after_colons: "tab[Label]{#a}" }, colon_count: 3 }"#,
                "Start(Paragraph)",
                r#"Text(Borrowed("body"))"#,
                "End(Paragraph)",
                r#"ContainerDirectiveEnd { name: Some("tab"), colon_count: 3, implicit: false }"#,
            ]
        );
    }

    #[test]
    fn an_unclosed_container_is_synthesized_closed_at_eof() {
        let stream = debug_stream(":::note\n\nbody\n");
        assert_eq!(
            stream,
            [
                r#"ContainerDirectiveStart { payload: BlockDirectivePayload { name: "note", args: DirectiveArgs { content: "", attrs: [] }, after_colons: "note" }, colon_count: 3 }"#,
                "Start(Paragraph)",
                r#"Text(Borrowed("body"))"#,
                "End(Paragraph)",
                r#"ContainerDirectiveEnd { name: Some("note"), colon_count: 0, implicit: true }"#,
            ]
        );
    }

    #[test]
    fn an_unclosed_container_in_a_blockquote_closes_before_the_quote_ends() {
        let stream = debug_stream("> :::note\n>\n> body\n\nafter\n");
        // The synthesized close must precede End(BlockQuote).
        let close = stream
            .iter()
            .position(|e| e.contains("ContainerDirectiveEnd"))
            .unwrap();
        let quote_end = stream
            .iter()
            .position(|e| e == "End(BlockQuote(None))")
            .unwrap();
        assert!(close < quote_end, "stream: {stream:?}");
        assert!(
            stream[close].contains("implicit: true"),
            "stream: {stream:?}"
        );
    }

    #[test]
    fn a_stray_close_carries_no_name() {
        let stream = debug_stream(":::\n");
        assert!(
            stream
                .iter()
                .any(|e| e.contains("ContainerDirectiveEnd { name: None")),
            "stream: {stream:?}"
        );
    }

    #[test]
    fn a_close_inside_a_deeper_block_is_stray_and_leaves_the_outer_container_open() {
        for markdown in [
            ":::note\n\n> :::\n\nafter\n\n:::\n",
            ":::note\n\n- item\n\n  :::\n\nafter\n\n:::\n",
        ] {
            let stream = debug_stream(markdown);
            let ends: Vec<&String> = stream
                .iter()
                .filter(|e| e.contains("ContainerDirectiveEnd"))
                .collect();
            assert_eq!(
                ends,
                [
                    "ContainerDirectiveEnd { name: None, colon_count: 3, implicit: false }",
                    r#"ContainerDirectiveEnd { name: Some("note"), colon_count: 3, implicit: false }"#,
                ],
                "stream: {stream:?}"
            );
        }
    }

    #[test]
    fn nested_unclosed_containers_close_innermost_first_at_eof() {
        // Two openers left open: the inner one closes before the outer, and both
        // carry `implicit: true`.
        let stream = debug_stream(":::note\n\n:::warn\n\nbody\n");
        let ends: Vec<&String> = stream
            .iter()
            .filter(|e| e.contains("ContainerDirectiveEnd"))
            .collect();
        assert_eq!(ends.len(), 2, "stream: {stream:?}");
        assert!(
            ends[0].contains(r#"name: Some("warn")"#),
            "stream: {stream:?}"
        );
        assert!(
            ends[1].contains(r#"name: Some("note")"#),
            "stream: {stream:?}"
        );
        assert!(ends.iter().all(|e| e.contains("implicit: true")));
    }

    #[test]
    fn an_explicit_close_is_not_marked_implicit() {
        let stream = debug_stream(":::note\n\nbody\n\n:::\n");
        assert!(
            stream.iter().any(|e| e.contains("ContainerDirectiveEnd")
                && e.contains(r#"name: Some("note")"#)
                && e.contains("implicit: false")),
            "stream: {stream:?}"
        );
    }

    #[test]
    fn a_colon_led_paragraph_that_parses_as_no_directive_stays_a_paragraph() {
        // A leading `:` only makes the paragraph a *candidate*: both block
        // parsers decline, and the inline scan finds no `:name[…]` either, so
        // the run is lent whole as prose.
        let stream = debug_stream(": not a directive");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed(": not a directive"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn a_punctuation_colon_does_not_hide_the_directive_after_it() {
        // The scan must step past a colon that starts no directive and keep
        // looking; stopping at the first one drops `:kbd` back into prose.
        let stream = debug_stream("Note: press :kbd[Ctrl+C]");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("Note: press "))"#,
                r#"InlineDirective(InlineDirectivePayload { name: "kbd", args: DirectiveArgs { content: "Ctrl+C", attrs: [] }, raw: Borrowed(":kbd[Ctrl+C]") })"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn an_escaped_or_encoded_colon_opens_no_directive() {
        for (md, text) in [
            (r"a \:kbd[X]", "a :kbd[X]"),
            (r"\:kbd[X]", ":kbd[X]"),
            (r"\:kbd{a=1}", ":kbd{a=1}"),
            (r"a \::kbd[X]", "a ::kbd[X]"),
            (r"\::youtube[x]", "::youtube[x]"),
            (r"\:::note", ":::note"),
            (r":\::note", ":::note"),
            ("&#58;status[X]", ":status[X]"),
            ("a &#x3A;status[X] b", "a :status[X] b"),
            ("a &colon;status{c=d} b", "a :status{c=d} b"),
            (":&#58;youtube[x]", "::youtube[x]"),
            (":&#58;:note", ":::note"),
            ("::&#58;note", ":::note"),
        ] {
            let stream = debug_stream(md);
            let joined: String = stream
                .iter()
                .filter_map(|e| e.strip_prefix(r#"Text(Borrowed(""#)?.strip_suffix(r#""))"#))
                .collect();
            assert_eq!(
                stream.first().map(String::as_str),
                Some("Start(Paragraph)"),
                "{md}"
            );
            assert_eq!(
                stream.last().map(String::as_str),
                Some("End(Paragraph)"),
                "{md}"
            );
            assert_eq!(joined, text, "{md}: {stream:?}");
        }
    }

    #[test]
    fn an_escaped_or_encoded_closer_closes_no_container() {
        for closer in ["\\:::", ":&#58;:"] {
            let stream = debug_stream(&format!(":::note\n\nbody\n\n{closer}\n"));
            assert_eq!(
                stream,
                [
                    r#"ContainerDirectiveStart { payload: BlockDirectivePayload { name: "note", args: DirectiveArgs { content: "", attrs: [] }, after_colons: "note" }, colon_count: 3 }"#,
                    "Start(Paragraph)",
                    r#"Text(Borrowed("body"))"#,
                    "End(Paragraph)",
                    "Start(Paragraph)",
                    r#"Text(Borrowed(":::"))"#,
                    "End(Paragraph)",
                    r#"ContainerDirectiveEnd { name: Some("note"), colon_count: 0, implicit: true }"#,
                ],
                "{closer}"
            );
        }
    }

    #[test]
    fn an_escaped_backslash_leaves_the_bracket_after_it_live() {
        let stream = debug_stream(r"a :kbd[\\] b");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("a "))"#,
                r#"InlineDirective(InlineDirectivePayload { name: "kbd", args: DirectiveArgs { content: "\\", attrs: [] }, raw: Borrowed(":kbd[\\]") })"#,
                r#"Text(Borrowed(" b"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn a_long_run_of_escaped_backslashes_closes_its_group() {
        // One `Text` per escaped backslash: a check that scanned back over
        // the run on each would be quadratic.
        let markdown = format!(":kbd[{}]", "\\".repeat(200_000));
        let mut parser = Parser::new(&markdown, false, true);
        let mut content_len = None;
        while let Some(event) = parser.next() {
            if let Event::InlineDirective(payload) = event {
                content_len = Some(payload.args.content.len());
            }
        }
        assert_eq!(content_len, Some(100_000));
    }

    #[test]
    fn an_escaped_bracket_or_brace_is_content_not_a_delimiter() {
        let stream = debug_stream(r":note[a \] b]{t=\}} c");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"InlineDirective(InlineDirectivePayload { name: "note", args: DirectiveArgs { content: "a ] b", attrs: [("t", "}")] }, raw: Borrowed(":note[a ] b]{t=}}") })"#,
                r#"Text(Borrowed(" c"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn an_escaped_opening_bracket_is_no_group() {
        let stream = debug_stream(r"a :foo\[x] b");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("a :foo[x] b"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn an_encoded_bracket_is_content_like_an_escaped_one() {
        assert_eq!(
            debug_stream("a :foo&#91;x] b"),
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("a :foo[x] b"))"#,
                "End(Paragraph)",
            ]
        );
        assert_eq!(
            debug_stream(":note[a &#93; b] c"),
            [
                "Start(Paragraph)",
                r#"InlineDirective(InlineDirectivePayload { name: "note", args: DirectiveArgs { content: "a ] b", attrs: [] }, raw: Borrowed(":note[a ] b]") })"#,
                r#"Text(Borrowed(" c"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn an_escaped_or_encoded_quote_is_part_of_a_quoted_value() {
        for (md, title) in [
            (r#":note[x]{title="a \"b\" c" k=v}"#, r#"a "b" c"#),
            (r#":note[x]{title="a &quot;b&quot; c" k=v}"#, r#"a "b" c"#),
            (r":note[x]{title='a \'b\' c' k=v}", "a 'b' c"),
            (r":note[x]{title=\'a k=v}", "'a"),
        ] {
            let mut parser = Parser::new(md, false, true);
            let mut attrs = None;
            while let Some(event) = parser.next() {
                if let Event::InlineDirective(payload) = event {
                    attrs = Some(payload.args.attrs.clone());
                }
            }
            let expected =
                [("title", title), ("k", "v")].map(|(k, v)| (k.to_owned(), v.to_owned()));
            assert_eq!(attrs.as_deref(), Some(&expected[..]), "{md}");
        }
    }

    #[test]
    fn an_escaped_bracket_in_a_block_directive_label_is_content() {
        let stream = debug_stream(r":::tab[a \] b]");
        assert_eq!(
            stream,
            [
                r#"ContainerDirectiveStart { payload: BlockDirectivePayload { name: "tab", args: DirectiveArgs { content: "a ] b", attrs: [] }, after_colons: "tab[a ] b]" }, colon_count: 3 }"#,
                r#"ContainerDirectiveEnd { name: Some("tab"), colon_count: 0, implicit: true }"#,
            ]
        );
    }

    #[test]
    fn escapes_still_line_up_after_a_directive_is_lent() {
        let stream = debug_stream(r":a[1] \:b[2] :c[x\]y]");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"InlineDirective(InlineDirectivePayload { name: "a", args: DirectiveArgs { content: "1", attrs: [] }, raw: Borrowed(":a[1]") })"#,
                r#"Text(Borrowed(" :b[2] "))"#,
                r#"InlineDirective(InlineDirectivePayload { name: "c", args: DirectiveArgs { content: "x]y", attrs: [] }, raw: Borrowed(":c[x]y]") })"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn escapes_from_an_earlier_run_do_not_leak_into_the_next() {
        // Escapes at offsets 0 (a paragraph) and 4 (a leaf directive); the
        // last run has a `:` at 0 and a `[` at 4, and both must stay live.
        let stream = debug_stream("\\:a\n\n::y[\\]]\n\n:kbd[X] b");
        assert_eq!(
            stream,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed(":a"))"#,
                "End(Paragraph)",
                r#"LeafDirective(BlockDirectivePayload { name: "y", args: DirectiveArgs { content: "]", attrs: [] }, after_colons: "y[]]" })"#,
                "Start(Paragraph)",
                r#"InlineDirective(InlineDirectivePayload { name: "kbd", args: DirectiveArgs { content: "X", attrs: [] }, raw: Borrowed(":kbd[X]") })"#,
                r#"Text(Borrowed(" b"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn compact_directive_lines_stream_as_their_blank_line_form() {
        for (compact, spaced) in [
            (":::note\nSome text\n:::", ":::note\n\nSome text\n\n:::"),
            (
                "::::tabs\n:::tab[A]{#a}\na\n:::\n:::tab[B]\nb\n:::\n::::",
                "::::tabs\n\n:::tab[A]{#a}\n\na\n\n:::\n\n:::tab[B]\n\nb\n\n:::\n\n::::",
            ),
            (
                "before\n::youtube[x]\nafter",
                "before\n\n::youtube[x]\n\nafter",
            ),
            ("> :::note\n> body\n> :::", "> :::note\n>\n> body\n>\n> :::"),
            (
                ":::outer\n\n:::note\nbody\n:::\n\n:::",
                ":::outer\n\n:::note\n\nbody\n\n:::\n\n:::",
            ),
            (
                ":::note\nsome **bold** and `code`\nmore\n:::",
                ":::note\n\nsome **bold** and `code`\nmore\n\n:::",
            ),
            (":::note\n:::", ":::note\n\n:::"),
            ("  :::note\n  body\n  :::", ":::note\n\nbody\n\n:::"),
            (
                "- a\n\n- b\n  :::note\n  body\n  :::",
                "- a\n\n- b\n\n  :::note\n\n  body\n\n  :::",
            ),
            (
                "> - a\n>\n>   b\n>   :::note",
                "> - a\n>\n>   b\n>\n>   :::note",
            ),
            (":::note\r\nbody\r\n:::", ":::note\n\nbody\n\n:::"),
            // A hard break before or on a directive line splits like a soft one.
            (":::note\nbody  \n:::", ":::note\n\nbody\n\n:::"),
            (":::note\nbody\\\n:::", ":::note\n\nbody\n\n:::"),
            (
                ":::note  \nbody\n:::  \nafter",
                ":::note\n\nbody\n\n:::\n\nafter",
            ),
            (
                "::::tabs\n:::tab[A]\na\n:::\n::::\n::selection{color:red}\nb",
                "::::tabs\n\n:::tab[A]\n\na\n\n:::\n\n::::\n\n::selection{color:red}\n\nb",
            ),
        ] {
            assert_eq!(debug_stream(compact), debug_stream(spaced), "{compact:?}");
        }
    }

    #[test]
    fn ambiguous_colon_lines_stay_prose() {
        for md in [
            // A glued closer splits only when it closes something.
            "text\n:::\nmore",
            // An opener is the name and its groups, nothing more.
            "Some prose\n:::note text here\nmore",
            "Some prose\n:::tab[a]b]\nmore",
            // A glued leaf needs a group and a name that starts with a letter.
            "a\n::before\nb",
            "a\n::1[x]\nb",
            "a\n::std::move\nb",
            // Lazy continuation lines stay in the quote or list item they
            // continue.
            "> quote\n::::tabs\n:::tab[A]\na\n:::\n::::",
            "- one\n\n- two\n::::tabs\n:::tab[B]\nb\n:::\n::::",
            "> - a\n>\n>   b\n> :::note",
            // Every colon has to be in the source, not decoded from an entity.
            "&#58;::note",
            "&#58;::note\nbody",
            "a\n&#58;::note\nb",
            "a\n:&#58;:note",
            "a\n:&#58;:note\nb",
            "a\n:&#58;youtube[x]\nb",
        ] {
            let stream = debug_stream(md);
            assert!(
                !stream
                    .iter()
                    .any(|e| e.starts_with("Container") || e.starts_with("Leaf")),
                "{md:?}: {stream:?}"
            );
        }
    }

    #[test]
    fn a_glued_closer_that_would_be_stray_stays_prose() {
        // In the blank-line form this closer is stray: the quote it sits in
        // did not open the container.
        let stream = debug_stream(":::note\n\n> a\n> :::\n\n:::");
        let ends: Vec<&String> = stream
            .iter()
            .filter(|e| e.starts_with("ContainerDirectiveEnd"))
            .collect();
        assert_eq!(
            ends,
            [r#"ContainerDirectiveEnd { name: Some("note"), colon_count: 3, implicit: false }"#],
            "{stream:?}"
        );
    }

    #[test]
    fn a_line_inside_inline_markup_or_without_a_paragraph_is_not_split() {
        for (md, expected) in [
            (
                "*a\n:::*",
                &[
                    "Start(Paragraph)",
                    "Start(Emphasis)",
                    r#"Text(Borrowed("a"))"#,
                    "SoftBreak",
                    r#"Text(Borrowed(":::"))"#,
                    "End(Emphasis)",
                    "End(Paragraph)",
                ][..],
            ),
            (
                "- a\n  :::",
                &[
                    "Start(List(None))",
                    "Start(Item)",
                    r#"Text(Borrowed("a"))"#,
                    "SoftBreak",
                    r#"Text(Borrowed(":::"))"#,
                    "End(Item)",
                    "End(List(false))",
                ][..],
            ),
            (
                "*a\n::youtube[x]\nb*",
                &[
                    "Start(Paragraph)",
                    "Start(Emphasis)",
                    r#"Text(Borrowed("a"))"#,
                    "SoftBreak",
                    r#"Text(Borrowed("::youtube[x]"))"#,
                    "SoftBreak",
                    r#"Text(Borrowed("b"))"#,
                    "End(Emphasis)",
                    "End(Paragraph)",
                ][..],
            ),
            (
                "- a\n  ::youtube[x]\n  b",
                &[
                    "Start(List(None))",
                    "Start(Item)",
                    r#"Text(Borrowed("a"))"#,
                    "SoftBreak",
                    r#"Text(Borrowed("::youtube[x]"))"#,
                    "SoftBreak",
                    r#"Text(Borrowed("b"))"#,
                    "End(Item)",
                    "End(List(false))",
                ][..],
            ),
        ] {
            assert_eq!(debug_stream(md), expected, "{md:?}");
        }
    }

    #[test]
    fn an_inline_directive_at_line_start_stays_inline() {
        assert_eq!(
            debug_stream("a\n:abbr[x] b"),
            [
                "Start(Paragraph)",
                r#"Text(Borrowed("a"))"#,
                "SoftBreak",
                r#"InlineDirective(InlineDirectivePayload { name: "abbr", args: DirectiveArgs { content: "x", attrs: [] }, raw: Borrowed(":abbr[x]") })"#,
                r#"Text(Borrowed(" b"))"#,
                "End(Paragraph)",
            ]
        );
    }

    #[test]
    fn an_escaped_compact_directive_line_stays_prose() {
        for (md, [first, second]) in [
            ("body\n\\:::", ["body", ":::"]),
            ("\\:::note\nbody", [":::note", "body"]),
        ] {
            assert_eq!(
                debug_stream(md),
                [
                    "Start(Paragraph)".to_owned(),
                    format!(r#"Text(Borrowed("{first}"))"#),
                    "SoftBreak".to_owned(),
                    format!(r#"Text(Borrowed("{second}"))"#),
                    "End(Paragraph)".to_owned(),
                ],
                "{md:?}"
            );
        }
    }

    #[test]
    fn compact_directive_lines_stay_prose_with_directives_off() {
        let mut parser = Parser::new(":::note\nbody\n:::", false, false);
        let mut out = Vec::new();
        while let Some(event) = parser.next() {
            out.push(format!("{event:?}"));
        }
        assert_eq!(
            out,
            [
                "Start(Paragraph)",
                r#"Text(Borrowed(":::note"))"#,
                "SoftBreak",
                r#"Text(Borrowed("body"))"#,
                "SoftBreak",
                r#"Text(Borrowed(":::"))"#,
                "End(Paragraph)",
            ]
        );
    }

    // The four below pin what cmark's block structure does to compact lines
    // before the Parser sees them.

    #[test]
    fn a_compact_opener_over_a_setext_underline_is_a_heading() {
        assert_eq!(
            debug_stream(":::tab[A]\n---"),
            [
                "Start(Heading { level: 2 })",
                r#"Text(Borrowed(":::tab[A]"))"#,
                "End(Heading)",
            ]
        );
    }

    #[test]
    fn a_compact_closer_after_a_list_joins_the_last_item() {
        // The closer is a lazy continuation line of the tight item, so the
        // container stays open until EOF.
        assert_eq!(
            debug_stream(":::note\n- item\n:::"),
            [
                r#"ContainerDirectiveStart { payload: BlockDirectivePayload { name: "note", args: DirectiveArgs { content: "", attrs: [] }, after_colons: "note" }, colon_count: 3 }"#,
                "Start(List(None))",
                "Start(Item)",
                r#"Text(Borrowed("item"))"#,
                "SoftBreak",
                r#"Text(Borrowed(":::"))"#,
                "End(Item)",
                "End(List(false))",
                r#"ContainerDirectiveEnd { name: Some("note"), colon_count: 0, implicit: true }"#,
            ]
        );
    }

    #[test]
    fn a_compact_closer_after_a_blockquote_joins_the_quote() {
        // A lazy continuation line: the closer is text in the quote, and the
        // container stays open until EOF.
        assert_eq!(
            debug_stream(":::note\n> quote\n:::"),
            [
                r#"ContainerDirectiveStart { payload: BlockDirectivePayload { name: "note", args: DirectiveArgs { content: "", attrs: [] }, after_colons: "note" }, colon_count: 3 }"#,
                "Start(BlockQuote(None))",
                "Start(Paragraph)",
                r#"Text(Borrowed("quote"))"#,
                "SoftBreak",
                r#"Text(Borrowed(":::"))"#,
                "End(Paragraph)",
                "End(BlockQuote(None))",
                r#"ContainerDirectiveEnd { name: Some("note"), colon_count: 0, implicit: true }"#,
            ]
        );
    }

    #[test]
    fn a_compact_closer_after_an_html_block_is_html() {
        // An HTML block runs to the next blank line.
        assert_eq!(
            debug_stream(":::note\n<div>x</div>\n:::"),
            [
                r#"ContainerDirectiveStart { payload: BlockDirectivePayload { name: "note", args: DirectiveArgs { content: "", attrs: [] }, after_colons: "note" }, colon_count: 3 }"#,
                r#"RawHtml(Borrowed("<div>x</div>\n"))"#,
                r#"RawHtml(Borrowed(":::"))"#,
                r#"ContainerDirectiveEnd { name: Some("note"), colon_count: 0, implicit: true }"#,
            ]
        );
    }
}
