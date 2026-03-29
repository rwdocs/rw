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
//! Common elements (tables, lists, inline formatting) are handled by the
//! generic renderer; format-specific elements (code blocks, blockquotes,
//! images) are delegated to the backend.
//!
//! ## Extension points
//!
//! - **Code block processors** ([`CodeBlockProcessor`]) — intercept fenced
//!   code blocks by language (e.g., diagram rendering via Kroki). Processors
//!   return a [`ProcessResult`]: a placeholder for deferred work, inline HTML,
//!   or pass-through for normal syntax highlighting.
//!
//! - **Directives** ([`directive`] module) — [CommonMark generic directives]
//!   syntax (`:inline`, `::leaf`, `:::container`). Directives are preprocessed
//!   before pulldown-cmark parsing because pulldown-cmark does not understand
//!   directive syntax natively; a post-processing pass then transforms
//!   intermediate elements into final HTML.
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
//! use rw_renderer::{MarkdownRenderer, HtmlBackend};
//!
//! let markdown = "# Hello\n\n**Bold** text with a [link](other.md).";
//! let result = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_title_extraction()
//!     .with_base_path("/docs/guide")
//!     .render_markdown(markdown);
//!
//! assert_eq!(result.title.as_deref(), Some("Hello"));
//! assert!(result.html.contains("<strong>Bold</strong>"));
//! assert!(result.html.contains(r#"<a href="/docs/guide/other">"#));
//! ```
//!
//! Add a custom code block processor:
//!
//! ```
//! use std::collections::HashMap;
//! use rw_renderer::{CodeBlockProcessor, HtmlBackend, MarkdownRenderer, ProcessResult};
//!
//! struct MathProcessor;
//!
//! impl CodeBlockProcessor for MathProcessor {
//!     fn process(
//!         &mut self,
//!         language: &str,
//!         _attrs: &HashMap<String, String>,
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
//! let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_processor(MathProcessor);
//!
//! let result = renderer.render_markdown("```math\nx^2 + y^2 = z^2\n```");
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
pub mod directive;
mod html;
mod renderer;
mod state;
pub(crate) mod tabs;
mod util;

pub use backend::{AlertKind, RenderBackend};
pub use bundle::bundle_markdown;
pub use code_block::{CodeBlockProcessor, ExtractedCodeBlock, ProcessResult};
pub use html::HtmlBackend;
pub use renderer::{MarkdownRenderer, RenderResult, TitleResolver};
/// Re-exported from [`rw_sections`] for use with
/// [`MarkdownRenderer::with_sections`].
///
/// Holds a map of section refs (e.g., `"domain:default/billing"`) to
/// filesystem paths, enabling wikilink resolution and cross-section link
/// annotation. Built by higher-level crates like `rw-site` from the site
/// configuration.
pub use rw_sections::Sections;
pub use state::{TocEntry, escape_html};
pub use tabs::TabsDirective;
