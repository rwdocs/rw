//! The event vocabulary produced by [`Parser`](crate::parser::Parser).
//!
//! # The boundary
//!
//! These types name syntax, never meaning: a `ContainerDirectiveStart` says a
//! `:::name[…]{…}` opener was seen, not that any handler exists for `name`.
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
//! * `HtmlBlock` — emitted, but dropped here. Its raw contents still arrive,
//!   as [`Event::RawHtml`].
//! * `MetadataBlock` — the whole block is swallowed, its text included, so the
//!   directive scanner never sees YAML.
//!
//! The last two make this a *lossy* projection rather than a faithful one; the
//! crate docs list every case.

use pulldown_cmark::{Alignment, CowStr};

use crate::alert::AlertKind;
use crate::directive::DirectiveArgs;
use crate::fence::FenceAttrs;

/// A single syntactic event.
///
/// One lifetime parameter is enough, and is what makes lending work:
/// `CowStr<'a>` is covariant, so a source-borrowed event coerces down to the
/// short `&mut self` borrow returned by
/// [`Parser::next`](crate::parser::Parser::next), and a run-borrowed event is
/// simply built at that shorter lifetime.
#[derive(Debug, Clone, PartialEq)]
pub enum Event<'a> {
    Start(Tag<'a>),
    End(TagEnd),
    Text(CowStr<'a>),
    Code(CowStr<'a>),
    /// Raw HTML, block or inline. One variant, because rw renders both
    /// identically; a consumer needing the distinction cannot recover it here.
    RawHtml(CowStr<'a>),
    SoftBreak,
    HardBreak,
    Rule,
    TaskListMarker(bool),

    /// `:name[content]{attrs}` seen in a text run.
    InlineDirective(InlineDirectivePayload<'a>),
    /// `::name[content]{attrs}` occupying a whole paragraph.
    LeafDirective(BlockDirectivePayload),
    /// `:::name[content]{attrs}` opening a container.
    ContainerDirectiveStart(BlockDirectivePayload),
    /// A bare `:::` run closing a container.
    ContainerDirectiveEnd {
        /// `usize`, not a narrower type: a literal closer is reconstructed as
        /// `":".repeat(colon_count)`, so the count is output-visible and a
        /// 300-colon line must survive it.
        colon_count: usize,
    },

    /// A fenced or indented code block, body included.
    ///
    /// A leaf rather than `Start`/`Text`/`End`: a code block is consumed whole
    /// — to be handed to a processor, or emitted — so a split shape would only
    /// make every consumer re-accumulate what the Parser already assembled.
    CodeBlock(CodeBlockPayload),
}

/// The syntax of one inline directive occurrence.
///
/// Carries `raw` where [`BlockDirectivePayload`] carries `colon_count`.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineDirectivePayload<'a> {
    /// Owned, moved out of the `Directive` the syntax parser produced.
    pub name: String,
    pub args: DirectiveArgs,
    /// The byte-exact source slice.
    ///
    /// An inline directive no handler claims is emitted as this slice, never
    /// reconstructed, because `DirectiveArgs::to_syntax` is not a round-trip:
    /// it drops empty brackets, sorts attributes by key and re-quotes their
    /// values, respaces `{.a.b}`, and discards unrecognized barewords. Block
    /// directives carry no slice and are reconstructed instead.
    pub raw: CowStr<'a>,
}

/// The syntax of one leaf or container-opening directive occurrence.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDirectivePayload {
    /// Owned, moved out of the `Directive` the syntax parser produced.
    pub name: String,
    pub args: DirectiveArgs,
    /// A container opener's leading colon count: `:::name` and `::::name` open
    /// the same container, and only the source says which was written.
    ///
    /// No consumer reads it today: rw's renderer reconstructs an unclaimed
    /// opener in `DirectiveProcessor::dispatch_container_start` with a
    /// hardcoded `:::`, so `::::name` round-trips as `:::name` — this is the
    /// datum that would fix it. Fixed at `0` for a leaf, where it says
    /// nothing: `parse_leaf_line` accepts exactly two colons.
    pub colon_count: usize,
}

/// A code block and its body.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlockPayload {
    /// As `parse_fence_info` returns it today; `None` for a bare fence.
    pub language: Option<String>,
    pub attrs: FenceAttrs,
    /// Moved out of the Parser's accumulator. `String`, not `CowStr`: it can
    /// never be borrowed, and `CowStr` would cost an `into_boxed_str` realloc
    /// per fence for nothing.
    pub source: String,
}

/// The start of a nesting construct.
#[derive(Debug, Clone, PartialEq)]
pub enum Tag<'a> {
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

/// Which of the two link shapes was written.
///
/// Replaces cmark's `LinkType`, whose finer distinctions rw does not act on:
/// a wikilink resolves through a section registry the Parser has no access
/// to, and every other link is transformed the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
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
/// others, so cmark's `TagEnd` would force an exhaustive match to write arms
/// for ends whose starts can never arrive, and would re-import `HeadingLevel`
/// — a type this vocabulary claims to have dropped.
///
/// Unit variants except `List`, which carries the ordered flag a consumer
/// needs to close the right kind of list (as cmark's own `TagEnd::List` does).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagEnd {
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

/// `Event`'s size is a reviewed invariant, not an accident — an event is moved
/// out of `next` and into the consumer on the plain-prose hot path. Mirrors
/// cmark's own assertion form; the `target_pointer_width` gate keeps it off
/// 32-bit.
///
/// 144 is exactly [`InlineDirectivePayload`], the widest variant: the
/// discriminant rides in a niche inside the payload and costs nothing. A
/// change here is a layout regression to investigate, not a constant to
/// recompute.
///
/// Boxing the payloads would take this to roughly 48 bytes, below cmark's own
/// 80. It was tried and measured: render time moved less than 0.5% on both
/// benchmark fixtures — inside their run-to-run spread, and with the two
/// fixtures straddling zero — while adding exactly 5 heap allocations per
/// render against a gated baseline of 113. So the size is not what makes an
/// event cost what it costs, and shrinking it buys nothing worth an
/// allocation. Don't re-run that experiment; measure something else.
#[cfg(target_pointer_width = "64")]
const _STATIC_ASSERT_EVENT_SIZE: [(); 144] = [(); std::mem::size_of::<Event<'static>>()];
