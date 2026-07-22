//! Tokenizer for rw's markdown dialect: `CommonMark` plus directive syntax.
//!
//! [`Parser`] wraps `pulldown_cmark` and emits [`Event`]s. It recognizes
//! **syntax** and nothing else — it holds no directive registry, consults no
//! handler, and cannot tell a directive name some renderer knows from one
//! nobody does. What any of it *means* is the caller's to decide.
//!
//! ```
//! use rw_parser::{Event, Parser, Tag};
//!
//! let mut parser = Parser::new("Hello *world*", false, false);
//! assert_eq!(parser.next(), Some(Event::Start(Tag::Paragraph)));
//! ```
//!
//! # `next` lends, and so is not `Iterator`
//!
//! [`Parser::next`] borrows `&mut self` for as long as the event lives, which
//! `Iterator` cannot express. That is what lets a text run be handed out as a
//! `CowStr::Borrowed` into a buffer the Parser reuses, instead of an allocation
//! per run. Consume each event before asking for the next.
//!
//! No owning adapter is provided. One would have to deep-copy every borrowed
//! segment, and nothing needs it yet; lending does not preclude adding it.
//!
//! # `Event` is a lossy projection, not a faithful one
//!
//! The vocabulary is shaped for rw's renderer, and drops what that renderer
//! does not use. A consumer wanting the whole document will not find it here:
//!
//! * **YAML frontmatter is swallowed whole**, its text included, so the
//!   directive scanner never sees metadata.
//! * **A `[[target]]` with no `|display` part loses its display text.** rw resolves
//!   wikilink text from a section registry the Parser has no access to, so it
//!   suppresses cmark's. `[[target|display]]` keeps it.
//! * **Block and inline raw HTML arrive as the same [`Event::RawHtml`]**, and
//!   `HtmlBlock` and `FootnoteDefinition` tags are dropped.
//!
//! # Two `{…}` grammars, and they disagree
//!
//! [`DirectiveArgs`] and [`parse_fence_info`] both read what looks like one
//! `{#id .class key=value}` microsyntax. It is not one: the directive grammar
//! walks characters, the fence grammar splits on whitespace first. So
//! `{k="two words"}` survives only in a directive, `{.a.b}` is two classes
//! there and one class on a fence, and a bare `{flag}` is kept on a fence and
//! dropped in a directive. Reconciling them would change rendered output, so
//! they are deliberately left apart.

mod alert;
mod directive;
mod event;
mod fence;
mod parser;

pub use alert::AlertKind;
pub use directive::DirectiveArgs;
pub use directive::line::{Directive, InlineMatch, parse_line};
pub use event::{
    BlockDirectivePayload, CodeBlockPayload, Event, InlineDirectivePayload, LinkKind, Tag, TagEnd,
};
/// Re-exported from `pulldown_cmark`: both appear in [`Event`] and [`Tag`], so
/// a consumer can name them without depending on `pulldown_cmark` itself — and
/// without risking two incompatible copies in one dependency graph.
pub use pulldown_cmark::{Alignment, CowStr};

pub use fence::{FenceAttrs, parse_fence_info};
pub use parser::Parser;
