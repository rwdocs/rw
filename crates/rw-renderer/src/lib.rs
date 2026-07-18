//! Trait-based markdown renderer with pluggable backends, extensible code block
//! processing, and directive syntax support.
//!
//! # Architecture
//!
//! [`MarkdownRenderer`] walks [pulldown-cmark] events and delegates
//! format-specific rendering to a [`RenderBackend`] implementation.
//! This crate ships [`HtmlBackend`] for semantic HTML5 output with relative
//! link resolution; other backends (e.g., Confluence XHTML) can be
//! implemented downstream.
//!
//! All output is delegated to the backend — the renderer handles event
//! walking and state management only. Backends override whichever
//! methods differ from the HTML5 defaults.
//!
//! ## Extension points
//!
//! - **Code block processors** ([`CodeBlockProcessor`]) — intercept fenced
//!   code blocks by language (e.g., diagram rendering via Kroki). Processors
//!   return a [`ProcessResult`]: a placeholder for deferred work, inline HTML,
//!   or pass-through for normal syntax highlighting.
//!
//! - **Directives** ([`directive`] module) — [CommonMark generic directives]
//!   syntax (`:inline`, `::leaf`, `:::container`). Directives are recognized
//!   during the event walk and dispatched straight to the backend. A handler
//!   whose markup depends on content the walk has not reached yet — a tab strip
//!   needs every tab's label — returns [`DirectiveOutput::Deferred`](directive::DirectiveOutput::Deferred), reserving
//!   a hole at the current output offset; a single assembly pass after the walk
//!   splices in the content each handler then supplies.
//!
//! [pulldown-cmark]: https://docs.rs/pulldown-cmark
//! [CommonMark generic directives]: https://talk.commonmark.org/t/generic-directives-plugins-syntax/444
//!
//! ## Wikilinks
//!
//! When enabled via [`MarkdownRenderer::with_wikilinks`], the renderer supports
//! `[[target]]` syntax for section-stable internal links that survive directory
//! reorganization. Wikilinks are resolved through [`Sections`] (set via
//! [`MarkdownRenderer::with_sections`]) and display text is looked up via a
//! [`TitleResolver`] (set via [`MarkdownRenderer::with_title_resolver`]).
//! Each piece degrades gracefully when omitted:
//!
//! - Without [`Sections`], all wikilinks render as broken links
//!   (`class="rw-broken-link"`)
//! - Without a [`TitleResolver`], display text falls back to the last path
//!   segment (e.g., `[[domain:billing::overview]]` displays as "overview")
//! - Without [`MarkdownRenderer::with_wikilinks`], `[[...]]` syntax is not
//!   parsed — pulldown-cmark treats it as plain text
//!
//! Supported syntax forms:
//!
//! | Syntax | Description |
//! |--------|-------------|
//! | `[[kind:name::path]]` | Cross-section link (e.g., `[[domain:billing::overview]]`) |
//! | `[[kind:name]]` | Link to a section root (e.g., `[[domain:billing]]`) |
//! | `[[name]]` | Short form — section kind defaults to `"section"` |
//! | `[[::path]]` | Current-section link — resolved relative to `base_path` |
//! | `[[::]]` | Current-section root |
//! | `[[#fragment]]` | Same-page fragment link |
//! | `[[target\|display text]]` | Any form above with explicit display text |
//!
//! Unresolved wikilinks render with a `class="rw-broken-link"` indicator.
//! When no explicit display text is given, the renderer tries (in order):
//! the [`TitleResolver`], the last path segment, the section name, or the
//! raw href.
//!
//! # Examples
//!
//! Render markdown to HTML:
//!
//! ```
//! use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
//!
//! let markdown = "# Hello\n\n**Bold** text with a [link](other.md).";
//! let result = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_title_extraction()
//!     .with_base_path("/docs/guide")
//!     .render(markdown, Pipeline::new());
//!
//! assert_eq!(result.title.as_deref(), Some("Hello"));
//! assert!(result.html.contains("<strong>Bold</strong>"));
//! assert!(result.html.contains(r#"<a href="/docs/guide/other">"#));
//! ```
//!
//! Add a custom code block processor:
//!
//! ```
//! use rw_renderer::{
//!     CodeBlockProcessor, FenceAttrs, HtmlBackend, MarkdownRenderer, Pipeline, ProcessResult,
//! };
//!
//! struct MathProcessor;
//!
//! impl CodeBlockProcessor for MathProcessor {
//!     fn process(
//!         &mut self,
//!         language: &str,
//!         _attrs: &FenceAttrs,
//!         source: &str,
//!         _index: usize,
//!     ) -> ProcessResult {
//!         if language == "math" {
//!             ProcessResult::Inline(format!(r#"<div class="math">{source}</div>"#))
//!         } else {
//!             ProcessResult::PassThrough
//!         }
//!     }
//! }
//!
//! let renderer = MarkdownRenderer::<HtmlBackend>::new();
//! let pipeline = Pipeline::new().with_processor(MathProcessor);
//! let result = renderer.render("```math\nx^2 + y^2 = z^2\n```", pipeline);
//! assert!(result.html.contains(r#"class="math"#));
//! ```
//!
//! # Feature flags
//!
//! - **`serde`** — enables `Serialize`/`Deserialize` on [`TocEntry`] for
//!   JSON serialization in HTTP API responses.

mod backend;
mod bundle;
mod code_block;
mod comment;
mod config;
pub mod directive;
mod holes;
mod html;
mod link;
mod pipeline;
mod renderer;
mod scope;
mod search_document;
mod status;
mod table;
pub(crate) mod tabs;
mod toc;
mod util;
mod walker;
mod wikilink;

pub use backend::{AlertKind, RenderBackend};
pub use bundle::bundle_markdown;
pub use code_block::{CodeBlockProcessor, ExtractedCodeBlock, FenceAttrs, ProcessResult};
pub use comment::render_comment_body;
pub use config::TitleResolver;
pub use html::HtmlBackend;
pub use pipeline::Pipeline;
/// Re-exported for use in [`RenderBackend::table_cell_start`] implementations.
pub use pulldown_cmark::Alignment;
pub use renderer::{MarkdownRenderer, RenderResult};
/// Re-exported from [`rw_sections`] for use with
/// [`MarkdownRenderer::with_sections`].
///
/// Holds a map of section refs (e.g., `"domain:default/billing"`) to
/// filesystem paths, enabling wikilink resolution and cross-section link
/// annotation. Built by higher-level crates like `rw-site` from the site
/// configuration.
pub use rw_sections::Sections;
pub use search_document::SearchDocumentBackend;
pub use status::{STATUS_MARKER, StatusColor, StatusDirective};
pub use tabs::TabsDirective;
pub use toc::TocEntry;
pub use util::{escape_html, escape_into};
