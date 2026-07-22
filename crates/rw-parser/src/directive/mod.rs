//! rw's directive syntax: the argument grammar and the line parsers.
//!
//! `:name[content]{attrs}` inline, `::name[…]` as a whole block, and
//! `:::name[…]` … `:::` wrapping one. A directive name no renderer knows
//! parses exactly like one it does — recognizing the syntax is all this does.

mod args;
pub(crate) mod line;

pub use args::DirectiveArgs;
