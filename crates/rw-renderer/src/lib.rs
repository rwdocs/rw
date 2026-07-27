//! Trait-based markdown renderer with pluggable backends and directive syntax
//! support.
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
//! [`RenderBackend`] is the only one. Every construct the renderer knows —
//! headings, tables, `:status` badges, `::::tabs` groups, diagrams — is a
//! built-in whose *markup* the backend decides.
//!
//! Directive syntax (`:status` inline badges, `::::tabs`/`:::tab` blocks) is
//! recognized during tokenization by [`rw_parser`]; the directive set is fixed.
//!
//! What a caller *does* supply is content the renderer cannot produce itself.
//! A fence whose language a [`DiagramRouter`] claims (configured through
//! [`MarkdownRenderer::with_diagram_languages`]) is not rendered during the
//! walk: it reserves a hole at its output offset and becomes a
//! [`DiagramRequest`]. [`MarkdownRenderer::begin`] hands back a [`RenderPass`]
//! listing those requests; the caller resolves them wherever the bytes live,
//! and [`RenderPass::finish`] turns each [`Resolutions`] entry into markup
//! through the backend and fills the holes. The renderer does no I/O and never
//! holds a provider — only the router that recognises which fence languages are
//! diagrams. [`MarkdownRenderer::render`] is the one-call form of that round
//! trip for a caller with nothing to add between the halves.
//!
//! A tab bar, whose markup depends on content the walk has not reached yet (it
//! needs every tab's label), reserves a hole the same way and fills it after the
//! walk, sharing one assembly pass with the diagrams.
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
//! use rw_renderer::{HtmlBackend, MarkdownRenderer, Providers};
//!
//! let markdown = "# Hello\n\n**Bold** text with a [link](other.md).";
//! let result = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_title_extraction()
//!     .with_base_path("/docs/guide")
//!     .render(markdown, &Providers::empty());
//!
//! assert_eq!(result.title.as_deref(), Some("Hello"));
//! assert!(result.html.contains("<strong>Bold</strong>"));
//! assert!(result.html.contains(r#"<a href="/docs/guide/other">"#));
//! ```
//!
//! # Feature flags
//!
//! - **`serde`** — enables `Serialize`/`Deserialize` on [`TocEntry`] for
//!   JSON serialization in HTTP API responses.

mod backend;
mod comment;
mod config;
mod diagram;
mod fills;
mod holes;
mod html;
mod link;
mod pass;
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
pub use comment::render_comment_body;
pub use config::TitleResolver;
pub use diagram::{DiagramLink, DiagramView};
pub use html::HtmlBackend;
pub use pass::RenderPass;
/// Re-exported for use in [`RenderBackend::table_cell_start`] implementations.
pub use pulldown_cmark::Alignment;
pub use renderer::{MarkdownRenderer, RenderResult};
/// Re-exported from [`rw_diagrams`], which defines the vocabulary providers and
/// backends share. They appear in [`RenderBackend::diagram`]'s signature and in
/// the two-phase API ([`MarkdownRenderer::begin`], [`RenderPass`],
/// [`MarkdownRenderer::with_diagram_languages`], [`MarkdownRenderer::render`]),
/// so a backend or a caller driving a pass needs them without depending on the
/// diagram crate directly.
pub use rw_diagrams::{
    Asset, DiagramContent, DiagramRequest, DiagramRouter, Providers, Resolutions, Size,
};
/// Re-exported from [`rw_parser`], which defines rw's markdown syntax. It
/// appears in [`RenderBackend::alert_start`]'s signature, so a backend needs it
/// without depending on the parser directly.
pub use rw_parser::AlertKind;
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
