//! Pluggable directives API for [CommonMark generic directive syntax][spec].
//!
//! Directives extend markdown with custom inline, block, and wrapping
//! elements using a colon-based syntax that does not conflict with standard
//! CommonMark:
//!
//! | Type | Syntax | Trait |
//! |------|--------|-------|
//! | Inline | `:name[content]{attrs}` | [`InlineDirective`] |
//! | Leaf (self-contained block) | `::name[content]{attrs}` | [`LeafDirective`] |
//! | Container (wrapping block) | `:::name` … `:::` | [`ContainerDirective`] |
//!
//! [spec]: https://talk.commonmark.org/t/generic-directives-plugins-syntax/444
//!
//! # Architecture
//!
//! Processing is split into two phases because pulldown-cmark does not
//! understand directive syntax natively:
//!
//! 1. **Preprocessing** ([`DirectiveProcessor::process`]) — runs before
//!    pulldown-cmark parsing. Converts directives to intermediate HTML
//!    elements (e.g., `<rw-tabs>`) that pass through the parser unchanged,
//!    or expands `::include` directives into raw markdown for recursive
//!    processing.
//!
//! 2. **Post-processing** ([`DirectiveProcessor::post_process`]) — runs
//!    after rendering. Transforms intermediate elements to final accessible
//!    HTML using the [`Replacements`] collector for efficient single-pass
//!    string replacement.
//!
//! # Example
//!
//! ```
//! use std::path::Path;
//! use rw_renderer::directive::{
//!     DirectiveProcessor, DirectiveArgs, DirectiveContext, DirectiveOutput, InlineDirective,
//! };
//!
//! struct KbdDirective;
//!
//! impl InlineDirective for KbdDirective {
//!     fn name(&self) -> &str { "kbd" }
//!
//!     fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
//!         DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content()))
//!     }
//! }
//!
//! let mut processor = DirectiveProcessor::new()
//!     .with_inline(KbdDirective);
//!
//! let output = processor.process("Press :kbd[Ctrl+C] to copy.");
//! assert!(output.contains("<kbd>Ctrl+C</kbd>"));
//! ```

mod args;
mod container;
mod context;
mod inline;
mod leaf;
mod output;
mod parser;
mod processor;
mod replacements;

pub use args::DirectiveArgs;
pub use container::ContainerDirective;
pub use context::DirectiveContext;
pub use inline::InlineDirective;
pub use leaf::LeafDirective;
pub use output::DirectiveOutput;
pub use processor::{DirectiveProcessor, DirectiveProcessorConfig};
pub use replacements::Replacements;
