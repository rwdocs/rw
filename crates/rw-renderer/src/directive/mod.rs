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
//! Directive syntax is recognized as the markdown is tokenized — there is no
//! separate pre-pass over the source text. Each recognized directive reaches
//! the renderer as an event of its own, which dispatches it to its handler, and
//! a final assembly pass fills in the content no handler could emit during the
//! walk:
//!
//! - **Inline directives** are split out of the surrounding text: `:name[…]`
//!   arrives as its own event and is dispatched straight into the backend, so
//!   the text around it is literal by construction. Inline code spans, code
//!   blocks, and raw HTML pass through unchanged. An inline directive that
//!   wraps a label in backend-specific markup returns
//!   [`DirectiveOutput::Marker`] — a semantic [`Marker`] the backend renders
//!   itself via `marker_open`/`marker_close` — rather than emitting markup that
//!   would reach every backend verbatim.
//!
//! - **Leaf and container directives** are recognized when their delimiter
//!   paragraph is tokenized (`::name` for a leaf, `:::name` … `:::` for a
//!   container), and likewise arrive as events. Because they ride the same
//!   stream as the markdown structure around them, they respect
//!   markdown block structure — a delimiter indented into a code block or
//!   inside a fenced block is left literal, and each delimiter must stand as
//!   its own blank-line-separated paragraph. Handlers emit HTML directly, a
//!   [`Marker`] the backend renders itself, or deferred content assembled after
//!   the walk (below).
//!
//! - **Assembly** fills the holes reserved during the walk. A leaf or container
//!   handler whose markup depends on content it has not seen yet — a tab strip
//!   needs every tab label, which only the closing `:::` reveals — returns
//!   [`DirectiveOutput::Deferred`] instead of HTML. That reserves a hole at the
//!   current output offset; once the walk completes, the handler's
//!   [`fills`](ContainerDirective::fills) hook supplies the hole's content and
//!   assembly splices every hole into the output in one pass, without scanning
//!   or rewriting the rendered HTML. Inline directives have no hole hook: they
//!   emit [`Marker`]s the backend renders directly.
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
//! use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
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
//! let renderer = MarkdownRenderer::<HtmlBackend>::new();
//!
//! let result = renderer.render(
//!     "Press :kbd[Ctrl+C] to copy.",
//!     Pipeline::new().with_directives(processor),
//! );
//! assert!(result.html.contains("<kbd>Ctrl+C</kbd>"));
//! ```

mod container;
mod context;
pub(crate) mod fills;
mod inline;
mod leaf;
mod marker;
mod output;
pub(crate) mod processor;

pub use container::ContainerDirective;
pub use context::{DirectiveContext, ResolveError};
pub use fills::{Fills, HoleKey, Part};
pub use inline::InlineDirective;
pub use leaf::LeafDirective;
pub use marker::Marker;
pub use output::DirectiveOutput;
pub use processor::{DirectiveProcessor, DirectiveProcessorConfig};
/// Re-exported from [`rw_parser`]: the `[content]{attrs}` a directive was
/// written with is syntax, and every handler trait here takes it.
pub use rw_parser::DirectiveArgs;
