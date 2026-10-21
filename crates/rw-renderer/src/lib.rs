//! Trait-based markdown renderer with pluggable backends and directive syntax
//! support.
//!
//! # Architecture
//!
//! [`MarkdownRenderer`] tokenizes markdown with [`rw_parser`], walks the
//! resulting event stream, and delegates format-specific rendering to a
//! [`RenderBackend`] implementation. What rw's markdown *is* — `CommonMark`
//! plus directive syntax — is `rw_parser`'s to define; this crate decides what
//! it renders to, and ships [`HtmlBackend`] for semantic HTML5 output. Other
//! backends override whichever methods differ from the HTML5 defaults.
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
//! and [`RenderPass::finish`] turns each resolution into markup
//! through the backend and fills the holes. The renderer does no I/O.
//! [`MarkdownRenderer::render`] is `begin` + `finish` with nothing
//! resolved, for a caller with no diagrams to resolve. An image alone in its
//! paragraph whose file has a diagram extension the router claims becomes a
//! request the same way, naming the file rather than holding its text.
//!
//! Which page a markdown file is published as is the caller's to say too: a
//! [`LinkResolver`] (set via [`MarkdownRenderer::with_link_resolver`]) gets
//! the file a link points at, folded against the page's source path,
//! and answers with the page's url. The renderer knows no file-naming rules.
//! Likewise a [`FileResolver`] (set via [`MarkdownRenderer::with_file_resolver`])
//! gets the file a local image names, relative to the project root, and
//! answers with its path in the project or why it refuses the file. Those
//! paths come back as [`RenderResult::files`]. [`ProjectFiles`] is that
//! resolver, taking the files a predicate accepts. An [`ImageSrc`] (set via
//! [`MarkdownRenderer::with_image_src`]) then picks the `src` each placed
//! image renders with. [`image_file`] places one image the same way without
//! rendering.
//!
//! A tab bar, whose markup depends on content the walk has not reached yet (it
//! needs every tab's label), reserves a hole the same way and fills it once its
//! group closes, sharing one assembly pass with the diagrams.
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
//!   segment (e.g., `[[domain:billing::overview]]` displays as "overview"),
//!   and a link to a missing page in a known section is not caught
//! - Without [`MarkdownRenderer::with_wikilinks`], `[[...]]` syntax is not
//!   parsed and stays plain text
//!
//! Supported syntax forms:
//!
//! | Syntax | Description |
//! |--------|-------------|
//! | `[[kind:name::path]]` | Cross-section link (e.g., `[[domain:billing::overview]]`) |
//! | `[[kind:name]]` | Link to a section root (e.g., `[[domain:billing]]`) |
//! | `[[name]]` | Short form — a section of any kind; broken when several kinds share the name |
//! | `[[::path]]` | Current-section link — resolved relative to `base_path` |
//! | `[[::]]` | Current-section root |
//! | `[[#fragment]]` | Same-page fragment link |
//! | `[[target\|display text]]` | Any form above with explicit display text |
//!
//! A wikilink to no section, or to a page the [`TitleResolver`] doesn't know,
//! renders with a `class="rw-broken-link"` indicator and adds a warning.
//! When no explicit display text is given, the renderer tries (in order):
//! the [`TitleResolver`], the last path segment, the section name, or the
//! raw href.
//!
//! # Examples
//!
//! Render markdown to HTML:
//!
//! ```
//! use rw_renderer::{HtmlBackend, MarkdownRenderer};
//!
//! let markdown = "# Hello\n\n**Bold** text with a [link](https://example.com).";
//! let result = MarkdownRenderer::<HtmlBackend>::new().render(markdown);
//!
//! assert_eq!(result.title.as_deref(), Some("Hello"));
//! assert!(result.html.contains("<strong>Bold</strong>"));
//! assert!(result.html.contains(r#"<a href="https://example.com">"#));
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
mod file;
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
pub use config::{FileRefusal, FileResolver, ImageSrc, LinkResolver, TitleResolver};
pub use diagram::{DiagramLink, DiagramView};
pub use file::{ProjectFiles, image_file, is_external};
pub use html::HtmlBackend;
pub use link::encode_url_path;
pub use pass::RenderPass;
pub use renderer::{MarkdownRenderer, RenderResult};
/// Re-exported from [`rw_diagrams`] for [`RenderBackend::diagram`] and the
/// two-phase API ([`MarkdownRenderer::begin`], [`RenderPass`]).
pub use rw_diagrams::{Asset, DiagramContent, DiagramRequest, DiagramRouter, Size};
/// Re-exported from [`rw_parser`] for [`RenderBackend::alert_start`].
pub use rw_parser::AlertKind;
/// Re-exported for use in [`RenderBackend::table_cell_start`] implementations.
pub use rw_parser::Alignment;
/// Re-exported from [`rw_sections`] for use with
/// [`MarkdownRenderer::with_sections`].
pub use rw_sections::Sections;
pub use search_document::SearchDocumentBackend;
pub use status::StatusColor;
pub use tabs::TabInfo;
pub use toc::TocEntry;
pub use util::{escape_html, escape_into, replace_controls_into};
