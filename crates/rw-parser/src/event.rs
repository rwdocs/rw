//! The event vocabulary produced by [`Parser`](crate::parser::Parser).
//!
//! # The boundary
//!
//! These types name syntax, never meaning: a `ContainerDirectiveStart` says a
//! `:::name[…]{…}` opener was seen, not that `name` means anything.
//!
//! Follows `pulldown_cmark`'s structural shape — `Start(Tag)` / `End(TagEnd)`
//! plus leaf events — extended with rw's own syntactic constructs
//! (directives, whole code blocks) and narrowed to what rw actually renders.
//!
//! # Why cmark variants are missing
//!
//! * `FootnoteDefinition`, `FootnoteReference`, `InlineMath`, `DisplayMath` —
//!   rw's parser options (`parser::cmark_options`) enable neither footnotes
//!   nor math, so cmark never emits them.
//! * `MetadataBlock` — off too. The Parser splits the page's frontmatter off
//!   before cmark sees the rest, and keeps it out of the event stream.
//! * `HtmlBlock` — emitted, but dropped here. Its raw contents still arrive,
//!   as [`Event::RawHtml`].
//!
//! The last two make this a lossy projection; the crate docs list every case.

use pulldown_cmark::{Alignment, CowStr};

use crate::alert::AlertKind;
use crate::attrs::Attrs;
use crate::directive::DirectiveArgs;
use crate::directive::line::{Escapes, InlineMatch, parse_escaped_line};

/// A single syntactic event.
///
/// `CowStr<'a>` is covariant, so a source-borrowed event coerces down to the
/// `&mut self` borrow returned by [`Parser::next`](crate::parser::Parser::next).
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
    /// `::name[content]{attrs}` occupying a whole paragraph line.
    LeafDirective(BlockDirectivePayload),
    /// `:::name[content]{attrs}` opening a container.
    ContainerDirectiveStart {
        payload: BlockDirectivePayload,
        /// Three or more: `:::name` and `::::name` open the same container,
        /// and only this says which was written, so a consumer rendering the
        /// opener literally can reproduce it.
        colon_count: usize,
    },
    /// A `:::` run closing a container, paired to its opener by the parser.
    ContainerDirectiveEnd {
        /// The paired opener's name; `None` for a stray `:::` — one with no
        /// open container, or one inside a blockquote or list item that the
        /// innermost open container encloses rather than opened in.
        name: Option<String>,
        /// A literal closer is rebuilt as `":".repeat(colon_count)`, so this
        /// must hold any run length. `0` for a parser-synthesized close.
        colon_count: usize,
        /// `true` when the parser synthesized this close at a block or EOF
        /// boundary (the source had no matching `:::`); `false` for an explicit
        /// `:::` closer.
        implicit: bool,
    },
    /// A whole line shaped like a block directive that is not one. It still
    /// arrives as ordinary text or raw HTML; this event only flags it.
    LiteralDirectiveLine {
        /// The line as written, from its first colon.
        line: CowStr<'a>,
        cause: LiteralCause,
    },

    /// A fenced or indented code block, body included.
    CodeBlock(CodeBlockPayload),
}

/// Why an [`Event::LiteralDirectiveLine`] is text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralCause {
    /// It continues the block above it, which a directive line cannot
    /// interrupt: the text of a tight list item, a lazy line of a quote or
    /// list item, a table, an HTML block, or a setext heading. Flagged once
    /// per block.
    Glued,
    /// It is the first line of a tight list item, which holds no paragraph
    /// a directive line could start. Also flagged once per block.
    TightItemStart,
    /// A paragraph's first line has text after the name or groups
    /// (`:::note Title`). Further into a paragraph, such a line is likelier
    /// prose.
    TextAfterName,
    /// Inline markup in the `[label]` or `{attrs}`.
    MarkupInGroup,
}

/// The syntax of one inline directive occurrence.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineDirectivePayload<'a> {
    /// Owned, moved out of the `Directive` the syntax parser produced.
    pub name: String,
    pub args: DirectiveArgs,
    /// The byte-exact source slice, which is what a consumer renders when it
    /// renders the directive literally: `args` keeps only what a directive
    /// reads, so the syntax cannot be rebuilt from it.
    pub raw: CowStr<'a>,
}

/// The syntax of one leaf or container-opening directive occurrence.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDirectivePayload {
    pub name: String,
    pub args: DirectiveArgs,
    /// The line as written after its leading colons (`" note[x]{b=2 a=1}"`).
    /// A literal is the colons — `::` for a leaf, `colon_count` of them for a
    /// container — followed by this.
    pub after_colons: String,
    /// Where [`after_colons`](Self::after_colons) holds characters decoded
    /// from an escape or entity, sorted.
    pub(crate) escapes: Vec<usize>,
}

impl BlockDirectivePayload {
    /// Whether this leaf reads as a directive even among prose: it has a
    /// `[…]` or `{…}` group and a name that starts with a letter, which
    /// `::before` and `::1` do not.
    #[must_use]
    pub fn is_unambiguous_leaf(&self) -> bool {
        self.after_colons.len() > self.name.len() && self.name.starts_with(char::is_alphabetic)
    }

    /// The first inline directive in [`after_colons`](Self::after_colons)
    /// at or after byte `from`, for rendering the line literally. Its range
    /// indexes `after_colons`. An escaped or encoded colon opens none, as in a
    /// text run.
    #[must_use]
    pub fn inline_directive(&self, from: usize) -> Option<InlineMatch> {
        let escapes = Escapes::new(&self.escapes, from);
        let mut matched = parse_escaped_line(&self.after_colons[from..], escapes)?;
        matched.range = from + matched.range.start..from + matched.range.end;
        Some(matched)
    }
}

/// A code block and its body.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlockPayload {
    /// The info string's first word before any `{…}` block; `None` for a bare
    /// fence.
    pub language: Option<String>,
    pub attrs: Attrs,
    /// Never borrowed from the source; a `CowStr` would realloc per fence.
    pub source: String,
}

/// The start of a nesting construct.
#[derive(Debug, Clone, PartialEq)]
pub enum Tag<'a> {
    Paragraph {
        /// One image, surrounding whitespace aside, is all the paragraph's
        /// source holds. Its `Start(Image)` is the very next event, so a
        /// consumer rendering it as a block learns its `dest_url` before it
        /// has to write a `<p>`. Keep it a flag: a payload with drop glue here
        /// slows every render.
        ///
        /// Judged on what is left once directive lines are split out, so the
        /// image in `:::tab[A]` / `![a](x.puml)` / `:::` is lone. Of the lines
        /// that can follow it there, only a `:::` closer is foreseen: before
        /// an opener or leaf line the flag stays unset. A tight list item has
        /// no paragraph, so an image alone in one is never marked.
        lone_image: bool,
    },
    Heading {
        /// 1..=6, already narrowed from cmark's `HeadingLevel`.
        level: u8,
        /// An H1 with nothing above it but frontmatter and HTML comments: the
        /// page's title, when it shows any text. Never set by
        /// [`Parser::fragment`](crate::parser::Parser::fragment).
        opens_page: bool,
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
/// `List` and `BlockQuote` repeat what their [`Tag`] carried, so a consumer
/// can close the right construct without a stack of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagEnd {
    Paragraph,
    Heading,
    List(bool),
    Item,
    /// The same kind its [`Tag::BlockQuote`] carried.
    BlockQuote(Option<AlertKind>),
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

/// Every event moves by value on the hot path, so a size change is a layout
/// regression to investigate, not a constant to recompute. 128 is
/// `ContainerDirectiveStart`, the widest variant; the discriminant sits in a
/// niche. Boxing the payloads does not speed rendering and adds allocations.
#[cfg(target_pointer_width = "64")]
const _STATIC_ASSERT_EVENT_SIZE: [(); 128] = [(); std::mem::size_of::<Event<'static>>()];
