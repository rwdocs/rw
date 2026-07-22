//! The event vocabulary produced by [`Parser`](crate::parser::Parser) and
//! consumed by [`Walker`](crate::walker::Walker).
//!
//! # The boundary
//!
//! The Parser **tokenizes**; the Walker **interprets**. These types name
//! syntax, never meaning: a `ContainerDirectiveStart` says a `:::name[…]{…}`
//! opener was seen, not that any handler exists for `name`.
//!
//! Follows `pulldown_cmark`'s structural shape — `Start(Tag)` / `End(TagEnd)`
//! plus leaf events — extended with rw's own syntactic constructs
//! (directives, whole code blocks) and narrowed to what rw actually renders.
//!
//! # Why cmark variants are missing
//!
//! * `FootnoteDefinition`, `FootnoteReference`, `InlineMath`, `DisplayMath` —
//!   rw's parser options (`parser::cmark_options`) enable neither footnotes
//!   nor math, so cmark never emits them. Verified against a document
//!   containing all four syntaxes.
//! * `HtmlBlock` — emitted, but the Walker only ever no-ops on it. The Parser
//!   drops it. Its raw contents still arrive, as [`Event::RawHtml`].
//! * `MetadataBlock` — the Parser swallows the whole block, including its
//!   text, so the directive scanner never sees YAML.
//!
//! `Event` is `pub(crate)` for this step and becomes public API when
//! `rw-parser` is extracted, so it is designed as a public type.

use pulldown_cmark::{Alignment, CowStr};

use crate::backend::AlertKind;
use crate::code_block::FenceAttrs;
use crate::directive::DirectiveArgs;

/// A single syntactic event.
///
/// One lifetime parameter is enough, and is what makes lending work:
/// `CowStr<'a>` is covariant, so a source-borrowed event coerces down to the
/// short `&mut self` borrow returned by
/// [`Parser::next`](crate::parser::Parser::next), and a run-borrowed event is
/// simply built at that shorter lifetime.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Event<'a> {
    Start(Tag<'a>),
    End(TagEnd),
    Text(CowStr<'a>),
    Code(CowStr<'a>),
    /// Raw HTML, block or inline — the Walker renders both identically.
    RawHtml(CowStr<'a>),
    SoftBreak,
    HardBreak,
    Rule,
    TaskListMarker(bool),

    /// `:name[content]{attrs}` seen in a text run.
    InlineDirective(DirectivePayload<'a>),
    /// `::name[content]{attrs}` occupying a whole paragraph.
    LeafDirective(DirectivePayload<'a>),
    /// `:::name[content]{attrs}` opening a container.
    ContainerDirectiveStart(DirectivePayload<'a>),
    /// A bare `:::` run closing a container.
    ContainerDirectiveEnd {
        colon_count: usize,
    },

    /// A fenced or indented code block, body included.
    ///
    /// A leaf rather than `Start`/`Text`/`End`: both Walker-side consumers
    /// (`CodeBlockProcessor::process` and `B::code_block`) need the body
    /// whole, so a split shape would only make the Walker re-accumulate what
    /// the Parser already assembled.
    CodeBlock(CodeBlockPayload),
}

/// The syntax of one directive occurrence.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DirectivePayload<'a> {
    /// As `ParsedDirective` produces it today.
    pub(crate) name: String,
    pub(crate) args: DirectiveArgs,
    /// Leading colon count; `0` for inline and leaf directives.
    ///
    /// `usize`, not a narrower type: a literal closer is reconstructed as
    /// `":".repeat(colon_count)`, so the count is output-visible and a
    /// 300-colon line must survive it. Free — the struct's padding absorbs it.
    pub(crate) colon_count: usize,
    /// The byte-exact source slice, **inline directives only** (`None` for
    /// block directives).
    ///
    /// Inline directives that no handler claims are emitted as this slice,
    /// never reconstructed, because `DirectiveArgs::to_syntax` is not a
    /// round-trip: it drops empty brackets, sorts and re-quotes attribute
    /// keys, respaces `{.a.b}`, and discards unrecognized barewords. Block
    /// directives *are* reconstructed, which is what preserves the
    /// `::::name` colon-loss recorded under Known debt.
    pub(crate) raw: Option<CowStr<'a>>,
}

/// A code block and its body.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CodeBlockPayload {
    /// As `parse_fence_info` returns it today; `None` for a bare fence.
    pub(crate) language: Option<String>,
    pub(crate) attrs: FenceAttrs,
    /// Moved out of the Parser's accumulator. `String`, not `CowStr`: it can
    /// never be borrowed, and `CowStr` would cost an `into_boxed_str` realloc
    /// per fence for nothing.
    pub(crate) source: String,
}

/// The start of a nesting construct.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tag<'a> {
    Paragraph,
    /// Level 1..=6, already narrowed from cmark's `HeadingLevel`.
    Heading {
        level: u8,
    },
    /// `Some(n)` for an ordered list starting at `n`, `None` for unordered.
    List(Option<u64>),
    Item,
    /// `Some(kind)` for a GFM alert, `None` for a plain blockquote.
    BlockQuote(Option<AlertKind>),
    Table(Vec<Alignment>),
    TableHead,
    TableRow,
    TableCell,
    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link {
        kind: LinkKind,
        dest_url: CowStr<'a>,
    },
    Image {
        dest_url: CowStr<'a>,
        title: CowStr<'a>,
    },
}

/// Which of the two link shapes the Walker must render.
///
/// Replaces cmark's `LinkType`, on which the Walker discriminates exactly
/// once: wikilinks resolve through `Sections`, everything else through
/// `transform_link`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkKind {
    /// `[[target]]`. `has_pothole` is true for `[[target|display]]`, whose
    /// display text cmark supplies itself.
    Wiki {
        has_pothole: bool,
    },
    Other,
}

/// The end of a nesting construct: rw's own projection of [`Tag`].
///
/// Not `pulldown_cmark::TagEnd`. rw's `Tag` drops cmark variants and reshapes
/// others, so cmark's `TagEnd` would force the Walker to write arms for ends
/// whose starts can never arrive, and would re-import `HeadingLevel` — a type
/// this vocabulary claims to have dropped.
///
/// Unit variants except `List`, which carries the ordered flag `B::list_end`
/// needs (as cmark's own `TagEnd::List` does).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagEnd {
    Paragraph,
    Heading,
    List(bool),
    Item,
    BlockQuote,
    Table,
    TableHead,
    TableRow,
    TableCell,
    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link,
    Image,
}

/// `Event`'s size is a reviewed invariant, not an accident — it is moved out
/// of `next` and into `handle` on the plain-prose hot path, and becomes public
/// API when `rw-parser` is extracted. Mirrors cmark's own assertion form
/// (`lib.rs:422`); the `target_pointer_width` gate keeps it off 32-bit.
///
/// 160 = `DirectivePayload`'s 152 plus a discriminant word. It cannot be 152:
/// `CowStr`'s tag offers exactly one spare niche value, which
/// `Option<CowStr>` inside the payload already consumes, so an enum this wide
/// cannot pack its discriminant into the payload. Reaching 80 would mean
/// boxing the payloads — rejected, because each `Box::new` is a heap
/// allocation today's code does not make (~5 per render against the
/// benchmark fixtures, on a 113 baseline), trading a hard requirement for an
/// unmeasured one. Revisit only if `CodSpeed` shows the moves dominate; the
/// escape hatch is boxing-with-recycling, which buys the size back without
/// the allocation.
#[cfg(target_pointer_width = "64")]
const _STATIC_ASSERT_EVENT_SIZE: [(); 160] = [(); std::mem::size_of::<Event<'static>>()];
