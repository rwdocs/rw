//! Markdown tokenizer: wraps `pulldown_cmark` and emits rw's [`Event`].
//!
//! The Parser recognizes **syntax** — markdown structure and rw's directive
//! syntax — and holds no directive registry, no handlers, and no knowledge of
//! what any directive name means. Every interpretation decision belongs to
//! the consumer.
//!
//! Takes only what a tokenizer needs, by value. The cmark feature set is its
//! own too: `cmark_options` defines rw's markdown dialect.

use std::ops::Range;

use pulldown_cmark as cmark;
use pulldown_cmark::{CodeBlockKind, CowStr, HeadingLevel, Options};

use crate::alert::AlertKind;
use crate::directive::DirectiveArgs;
use crate::directive::line::{
    ContainerLine, Directive, InlineMatch, parse_container_line, parse_leaf_line, parse_line,
};
use crate::event::{
    BlockDirectivePayload, CodeBlockPayload, Event, InlineDirectivePayload, LinkKind, Tag, TagEnd,
};
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

/// What the Parser owes before it may pull from cmark again.
///
/// Paragraph deferral needs more than a single slot: releasing `:foo **bold**`
/// owes three things in order — the held `<p>`, then the coalesced run, then
/// the event that triggered the release.
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
    /// Owe the held `Start(Paragraph)`, then the run, then this event.
    ReleasingParagraph(Event<'a>),
    /// Owe the run, then this event. The non-paragraph case — a run inside a
    /// heading or a tight list item interrupted by markup.
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
    inner: cmark::Parser<'a>,
    /// Recognize directive syntax. When false, `:name[…]`, `::name` and
    /// `:::name` stay prose and no directive event is ever emitted.
    ///
    /// A consumer that cannot act on directives must switch them off here
    /// rather than ignore the events: a block directive's source text is not
    /// recoverable from a `BlockDirectivePayload`, which carries no source
    /// slice, so an ignored event loses the line.
    directives: bool,
    /// The coalesced text run. cmark splits text at every `[`, `]`, entity and
    /// escape, and rw's directive syntax is only recognizable joined.
    run: String,
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
            inner: cmark::Parser::new_ext(markdown, cmark_options(wikilinks)),
            directives,
            run: String::new(),
            run_cursor: 0,
            pending: None,
            deferred: Deferred::None,
            code_block: None,
            in_metadata: false,
            skip_wikilink_text: false,
        }
    }

    /// Pull the next event.
    ///
    /// **Inherent, not `Iterator`.** The returned `Event<'_>` borrows `self`,
    /// which is a lending iterator — expressible without GATs only as an
    /// inherent method. That is what lets text stay borrowed rather than
    /// forcing an owned `Box<str>` per segment, preserving today's
    /// zero-allocation text path.
    ///
    /// # Release order
    ///
    /// Everything owed comes out before cmark is pulled again, in the order a
    /// consumer needs it: the held `<p>`, then the run it encloses, then the
    /// event that interrupted the run. See `Deferred`.
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
                self.run_cursor = 0;
            }

            // 1. The held `<p>` — before the run it encloses.
            match std::mem::replace(&mut self.deferred, Deferred::None) {
                Deferred::ReleasingParagraph(event) => {
                    self.deferred = Deferred::Draining(event);
                    return Some(Event::Start(Tag::Paragraph));
                }
                other => self.deferred = other,
            }

            // 2. Then the run — one segment per call, text and inline
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

            // 3. Then the event that interrupted the run. Re-dispatched rather
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

            // 4. Nothing owed: pull from cmark.
            let Some(event) = self.inner.next() else {
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
                // Re-polling `inner` after it returned `None` is sound:
                // `pulldown_cmark::Parser` is a `FusedIterator`.
                if self.run_cursor < self.run.len() {
                    let segment = self.take_run_segment();
                    return Some(run_segment_event(&self.run, segment));
                }
                return None;
            };
            if let Some(event) = self.translate(event)
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

        let Some(InlineMatch { directive, range }) = parse_line(&self.run[from..]) else {
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
    /// [`Event`] is 144 bytes, and `Option<Event>` is 144 too — its
    /// discriminant rides in a spare niche — so every hand-off between `next`,
    /// `translate`, `dispatch` and `start_tag` that LLVM does not collapse is a
    /// real 144-byte `sret` copy, and `next` runs once per emitted event, which
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

        if matches!(self.deferred, Deferred::Paragraph) {
            if matches!(event, Event::End(TagEnd::Paragraph)) {
                return Some(self.decide_block());
            }
            self.deferred = Deferred::ReleasingParagraph(event);
            return None;
        }

        // Hold the `<p>`: the paragraph may turn out to be a block-directive
        // delimiter, which emits none.
        if matches!(event, Event::Start(Tag::Paragraph)) && self.run.is_empty() {
            self.deferred = Deferred::Paragraph;
            return None;
        }

        if self.run.is_empty() {
            Some(event)
        } else {
            self.deferred = Deferred::Draining(event);
            None
        }
    }

    /// Decide, at `End(Paragraph)`, whether the coalesced run is a block
    /// directive or an ordinary paragraph.
    ///
    /// The decision has to live here, next to the run: cmark splits
    /// `:::tab[Label]{#a}` into five `Text` events, so only the joined form is
    /// recognizable, which is why the paragraph is held back until then.
    ///
    /// The first-byte early-out costs nothing in behaviour: only a paragraph
    /// whose trimmed text starts with `:` can be a block directive, so both
    /// parsers (which already reject non-`:` input) are skipped for the common
    /// prose case.
    fn decide_block(&mut self) -> Event<'a> {
        self.deferred = Deferred::None;
        debug_assert!(
            self.pending.is_none(),
            "the run is fully drained before cmark is pulled again, so no segment can still index it"
        );
        // Taken, not borrowed: the run has to be free to be recycled or handed
        // back below, which a live `&self.run` in the condition would forbid.
        let text = std::mem::take(&mut self.run);
        self.run_cursor = 0;

        let matched = if self.directives && text.trim_start().starts_with(':') {
            parse_container_line(&text)
                .map(Event::from)
                .or_else(|| parse_leaf_line(&text).map(Event::leaf))
        } else {
            None
        };

        if let Some(event) = matched {
            self.recycle_run(text);
            return event;
        }

        // An ordinary paragraph: `<p>`, then the run, then `</p>`. Handing the
        // buffer back keeps its allocation exactly as `recycle_run` would.
        self.run = text;
        self.deferred = Deferred::Draining(Event::End(TagEnd::Paragraph));
        Event::Start(Tag::Paragraph)
    }

    /// Return a taken run buffer's allocation so the next run of text reuses
    /// it. The `mem::take` in [`decide_block`](Self::decide_block) leaves a
    /// zero-capacity `String` behind, so without this every run that turned out
    /// to be a block directive would re-allocate and re-grow from empty.
    ///
    /// Content always wins over capacity: if the Parser has meanwhile buffered
    /// text, that buffer stays and `buf`'s allocation is dropped. No caller
    /// reaches that today — every one recycles into a buffer it just emptied —
    /// but the guard keeps the optimization from being silently lossy if one
    /// ever does. Discarding a spare allocation costs one `malloc` later;
    /// discarding buffered text would drop it from the rendered page.
    fn recycle_run(&mut self, mut buf: String) {
        if !self.run.is_empty() {
            return;
        }
        buf.clear();
        if buf.capacity() > self.run.capacity() {
            self.run = buf;
        }
    }

    /// Translate one cmark event, or absorb it into Parser-side state.
    ///
    /// Returns a source-borrowed `Event<'a>`; the coercion down to the shorter
    /// `&mut self` lifetime happens on return from [`next`](Self::next).
    fn translate(&mut self, event: cmark::Event<'a>) -> Option<Event<'a>> {
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
            cmark::Event::Start(tag) => self.start_tag(tag),
            cmark::Event::End(tag) => Self::end_tag(tag).map(Event::End),
            cmark::Event::Text(text) => {
                if self.skip_wikilink_text {
                    self.skip_wikilink_text = false;
                    return None;
                }
                Some(Event::Text(text))
            }
            cmark::Event::Code(code) => Some(Event::Code(code)),
            // Block and inline raw HTML are one event: rw renders both
            // identically.
            cmark::Event::Html(html) | cmark::Event::InlineHtml(html) => Some(Event::RawHtml(html)),
            cmark::Event::SoftBreak => Some(Event::SoftBreak),
            cmark::Event::HardBreak => Some(Event::HardBreak),
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
    fn start_tag(&mut self, tag: cmark::Tag<'a>) -> Option<Event<'a>> {
        let translated = match tag {
            cmark::Tag::Paragraph => Tag::Paragraph,
            cmark::Tag::Heading { level, .. } => Tag::Heading {
                level: heading_level_to_num(level),
            },
            cmark::Tag::BlockQuote(kind) => Tag::BlockQuote(kind.map(AlertKind::from)),
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
            cmark::Tag::List(start) => Tag::List(start),
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
    fn end_tag(tag: cmark::TagEnd) -> Option<TagEnd> {
        let translated = match tag {
            cmark::TagEnd::Paragraph => TagEnd::Paragraph,
            cmark::TagEnd::Heading(_) => TagEnd::Heading,
            cmark::TagEnd::BlockQuote(_) => TagEnd::BlockQuote,
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

/// Project a `:::` line onto the event vocabulary, moving its fields.
impl From<ContainerLine> for Event<'_> {
    fn from(line: ContainerLine) -> Self {
        match line {
            ContainerLine::Start {
                directive,
                colon_count,
            } => Event::ContainerDirectiveStart(BlockDirectivePayload {
                name: directive.name,
                args: directive.args,
                colon_count,
            }),
            ContainerLine::End { colon_count } => Event::ContainerDirectiveEnd { colon_count },
        }
    }
}

impl Event<'_> {
    /// Project a `::name` line onto the event vocabulary, moving its fields.
    ///
    /// Named rather than a [`From`] impl: [`Directive`] is also the payload of
    /// [`ContainerLine::Start`] and [`InlineMatch`], so it has three plausible
    /// projections and no canonical one.
    fn leaf(directive: Directive) -> Self {
        Event::LeafDirective(BlockDirectivePayload {
            name: directive.name,
            args: directive.args,
            colon_count: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, Tag, TagEnd};

    #[test]
    fn cmark_options_defaults_include_gfm_and_metadata() {
        let opts = cmark_options(false);
        assert!(opts.contains(Options::ENABLE_TABLES));
        assert!(opts.contains(Options::ENABLE_STRIKETHROUGH));
        assert!(opts.contains(Options::ENABLE_TASKLISTS));
        assert!(opts.contains(Options::ENABLE_GFM));
        assert!(opts.contains(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS));
        assert!(!opts.contains(Options::ENABLE_WIKILINKS));
    }

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

    #[test]
    fn cmark_options_enables_wikilinks_when_flag_on() {
        assert!(cmark_options(true).contains(Options::ENABLE_WIKILINKS));
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
    fn recycle_run_keeps_content_over_capacity() {
        // No path reaches this today (every caller recycles into an empty
        // buffer), but the guard is what keeps the optimization from being
        // silently lossy if one ever does: buffered text must survive, even
        // when the incoming allocation is roomier.
        let mut parser = Parser::new("", false, true);

        parser.run = String::from("pending");
        parser.recycle_run(String::with_capacity(4096));
        assert_eq!(parser.run, "pending");
    }

    #[test]
    fn recycle_run_reclaims_capacity_when_idle() {
        let mut parser = Parser::new("", false, true);

        parser.run = String::new();
        parser.recycle_run(String::from("spent buffer"));
        assert!(parser.run.is_empty());
        assert!(
            parser.run.capacity() > 0,
            "the spent buffer's allocation should have been kept for reuse"
        );
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
                r#"InlineDirective(InlineDirectivePayload { name: "one", args: DirectiveArgs { content: "1", id: None, classes: [], attrs: [] }, raw: Borrowed(":one[1]") })"#,
                r#"Text(Borrowed(" b "))"#,
                r#"InlineDirective(InlineDirectivePayload { name: "two", args: DirectiveArgs { content: "2", id: None, classes: [], attrs: [] }, raw: Borrowed(":two[2]") })"#,
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
        // `DirectiveArgs::to_syntax` is not a round-trip — this input alone
        // loses `bareword` and re-quotes `key` — so an unregistered inline
        // directive must be emitted as the source slice, never rebuilt.
        //
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
        // No finish hook exists any more, so a document whose last bytes are
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
                r#"InlineDirective(InlineDirectivePayload { name: "note", args: DirectiveArgs { content: "a & b", id: None, classes: [], attrs: [] }, raw: Borrowed(":note[a & b]") })"#,
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
                r#"InlineDirective(InlineDirectivePayload { name: "note", args: DirectiveArgs { content: "a [b] c", id: None, classes: [], attrs: [] }, raw: Borrowed(":note[a [b] c]") })"#,
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
                r#"ContainerDirectiveStart(BlockDirectivePayload { name: "tab", args: DirectiveArgs { content: "Label", id: Some("a"), classes: [], attrs: [] }, colon_count: 3 })"#,
                "Start(Paragraph)",
                r#"Text(Borrowed("body"))"#,
                "End(Paragraph)",
                "ContainerDirectiveEnd { colon_count: 3 }",
            ]
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
                r#"InlineDirective(InlineDirectivePayload { name: "kbd", args: DirectiveArgs { content: "Ctrl+C", id: None, classes: [], attrs: [] }, raw: Borrowed(":kbd[Ctrl+C]") })"#,
                "End(Paragraph)",
            ]
        );
    }
}
