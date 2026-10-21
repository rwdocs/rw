//! Tokenizer for rw's markdown dialect: `CommonMark` plus directive syntax.
//!
//! [`Parser`] wraps `pulldown_cmark` and emits [`Event`]s. It recognizes
//! **syntax** and nothing else: it cannot tell a directive name a consumer
//! acts on from one it ignores. What any of it *means* is the caller's to
//! decide.
//!
//! ```
//! use rw_parser::{Event, Parser, Tag};
//!
//! let mut parser = Parser::new("Hello *world*", false, false);
//! assert_eq!(parser.next(), Some(Event::Start(Tag::Paragraph { lone_image: false })));
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
//! * **YAML frontmatter never reaches the event stream**, so the directive
//!   scanner never sees metadata. [`Parser::frontmatter`] returns its text.
//! * **A `[[target]]` with no `|display` part loses its display text.** rw resolves
//!   wikilink text from a section registry the Parser has no access to, so it
//!   suppresses cmark's. `[[target|display]]` keeps it.
//! * **Block and inline raw HTML arrive as the same [`Event::RawHtml`]**, and
//!   `HtmlBlock` and `FootnoteDefinition` tags are dropped.

mod alert;
mod attrs;
mod directive;
mod event;
mod frontmatter;
mod head;
mod html;
mod parser;
mod rewrite;

pub use alert::AlertKind;
pub use directive::DirectiveArgs;
pub use directive::line::{Directive, InlineMatch};
pub use event::{
    BlockDirectivePayload, CodeBlockPayload, Event, InlineDirectivePayload, LinkKind, LiteralCause,
    Tag, TagEnd,
};
/// Re-exported from `pulldown_cmark`: both appear in [`Event`] and [`Tag`], so
/// a consumer can name them without depending on `pulldown_cmark` itself — and
/// without risking two incompatible copies in one dependency graph.
pub use pulldown_cmark::{Alignment, CowStr};

pub use attrs::Attrs;
pub use head::PageHead;
pub use parser::Parser;
pub use rewrite::{Fences, Rewrite, rewrite};
