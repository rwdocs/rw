//! Pluggable directives API for [CommonMark generic directive syntax][spec].
//!
//! Directives extend markdown with custom inline, block, and wrapping
//! elements using a colon-based syntax that does not conflict with standard
//! `CommonMark`:
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
//! Inline directives are expanded by the renderer itself: as it iterates the
//! pulldown-cmark event stream it scans `Event::Text` content for
//! `:name[…]` syntax and dispatches handlers directly into its backend.
//! Inline code spans, code blocks, and raw HTML pass through unchanged.
//!
//! # Path Resolution Sandbox
//!
//! Directive handlers that read files (e.g. `::include`) should call
//! [`DirectiveContext::resolve_path`] to resolve a user-supplied path.
//! The method rejects absolute paths, Windows-specific prefixes,
//! `..` segments that would escape the base directory, and control
//! bytes in the input. See [`ResolveError`] for the full failure
//! taxonomy. [`DirectiveContext::read`] does **not** sandbox on its own
//! — handlers must run `resolve_path` first.
//!
//! # Example
//!
//! The easiest way to see inline directives in action is through the full
//! [`MarkdownRenderer`](crate::MarkdownRenderer) pipeline:
//!
//! ```
//! use rw_renderer::{HtmlBackend, MarkdownRenderer};
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
//! let processor = DirectiveProcessor::new().with_inline(KbdDirective);
//! let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_directives(processor);
//!
//! let result = renderer.render_markdown("Press :kbd[Ctrl+C] to copy.");
//! assert!(result.html.contains("<kbd>Ctrl+C</kbd>"));
//! ```

mod args;
mod container;
mod context;
mod inline;
mod leaf;
mod output;
pub(crate) mod parser;
mod processor;
mod replacements;

pub use args::DirectiveArgs;
pub use container::ContainerDirective;
pub use context::{DirectiveContext, ResolveError};
pub use inline::InlineDirective;
pub use leaf::LeafDirective;
pub use output::DirectiveOutput;
pub use processor::{DirectiveProcessor, DirectiveProcessorConfig};
pub use replacements::Replacements;
