//! Trait-based markdown renderer with pluggable backends, extensible code block
//! processing, and directive syntax support.
//!
//! # Architecture
//!
//! [`MarkdownRenderer`] tokenizes markdown with [`rw_parser`], walks the
//! resulting event stream, and delegates format-specific rendering to a
//! [`RenderBackend`] implementation. What rw's markdown *is* — `CommonMark`
//! plus directive syntax — is `rw_parser`'s to define; this crate decides what
//! it renders to, and ships [`HtmlBackend`] for semantic HTML5 output with
//! relative link resolution. Other backends (e.g. Confluence XHTML) can be
//! implemented downstream.
//!
//! All output is delegated to the backend — the renderer handles event
//! walking and state management only. Backends override whichever
//! methods differ from the HTML5 defaults.
//!
//! ## Extension points
//!
//! - **Code block processors** ([`CodeBlockProcessor`]) — intercept fenced
//!   code blocks by language (e.g., diagram rendering via Kroki). A processor
//!   whose output isn't knowable during the walk — a diagram needs an HTTP
//!   round trip to Kroki — returns [`ProcessResult::Deferred`], reserving a
//!   hole at the current output offset; otherwise it returns inline HTML or
//!   passes through for normal syntax highlighting.
//!
//! Directive syntax (`:status` inline badges, `::::tabs`/`:::tab` blocks) is
//! recognized during tokenization by [`rw_parser`] and rendered as walker
//! built-ins. The directive set is fixed; [`RenderBackend`] is the extension
//! point for what those built-ins render to. A tab bar, whose markup depends on
//! content the walk has not reached yet (it needs every tab's label), reserves
//! a hole at its output offset and fills it after the walk, sharing the same
//! assembly pass as a deferred code block.
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
mod code_block;
mod comment;
mod config;
mod fills;
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

pub use backend::RenderBackend;
pub use code_block::{CodeBlockProcessor, ExtractedCodeBlock, ProcessResult};
pub use comment::render_comment_body;
pub use config::TitleResolver;
/// Types a [`CodeBlockProcessor::fills`] implementation uses to supply deferred
/// content: [`Fills`] collects a processor's hole content after the walk, keyed
/// by [`HoleKey`].
pub use fills::{Fills, HoleKey};
pub use html::HtmlBackend;
pub use pipeline::Pipeline;
/// Re-exported for use in [`RenderBackend::table_cell_start`] implementations.
pub use pulldown_cmark::Alignment;
pub use renderer::{MarkdownRenderer, RenderResult};
/// Re-exported from [`rw_parser`], which defines rw's markdown syntax. They
/// appear in [`RenderBackend`] and [`CodeBlockProcessor`] signatures, so a
/// backend or processor needs them without depending on the parser directly.
pub use rw_parser::{AlertKind, FenceAttrs};
/// Re-exported from [`rw_sections`] for use with
/// [`MarkdownRenderer::with_sections`].
///
/// Holds a map of section refs (e.g., `"domain:default/billing"`) to
/// filesystem paths, enabling wikilink resolution and cross-section link
/// annotation. Built by higher-level crates like `rw-site` from the site
/// configuration.
pub use rw_sections::Sections;
pub use search_document::SearchDocumentBackend;
pub use status::StatusColor;
pub use tabs::TabInfo;
pub use toc::TocEntry;
pub use util::{escape_html, escape_into};
