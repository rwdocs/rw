//! Pluggable directives API for CommonMark directive syntax.
//!
//! This module provides a trait-based extensibility system for handling CommonMark
//! directives (inline `:name`, leaf `::name`, and container `:::name`).
//!
//! # Architecture
//!
//! The directive system uses a two-phase processing model:
//!
//! 1. **Preprocessing** ([`DirectiveProcessor::process`]): Converts directive syntax
//!    to intermediate HTML elements that pass through pulldown-cmark unchanged.
//!
//! 2. **Post-processing** ([`DirectiveProcessor::post_process`]): Transforms
//!    intermediate elements to final HTML using the [`Replacements`] collector
//!    for single-pass string replacement.
//!
//! # Directive Types
//!
//! - **Inline** ([`InlineDirective`]): `:name[content]{attrs}` - inline elements
//! - **Leaf** ([`LeafDirective`]): `::name[content]{attrs}` - self-contained blocks
//! - **Container** ([`ContainerDirective`]): `:::name` ... `:::` - wrapping blocks
//!
//! # Example
//!
//! ```
//! use std::path::Path;
//! use rw_renderer::directive::{
//!     DirectiveProcessor, DirectiveProcessorConfig, DirectiveArgs,
//!     DirectiveContext, DirectiveOutput, InlineDirective,
//! };
//!
//! struct KbdDirective;
//!
//! impl InlineDirective for KbdDirective {
//!     fn name(&self) -> &str { "kbd" }
//!
//!     fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
//!         DirectiveOutput::html(format!("<kbd>{}</kbd>", args.content))
//!     }
//! }
//!
//! let config = DirectiveProcessorConfig::default();
//! let mut processor = DirectiveProcessor::new(config)
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
