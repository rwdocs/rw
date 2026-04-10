//! Manages the document hierarchy and page rendering pipeline for RW.
//!
//! This crate turns a flat list of documents from a [`Storage`](rw_storage::Storage) backend into a
//! navigable site with parent/child relationships, breadcrumbs, section-scoped
//! navigation, and on-demand markdown-to-HTML rendering with caching.
//!
//! # Core types
//!
//! - [`Site`] — the main entry point. Owns storage, cache, and renderer;
//!   provides [`navigation`](Site::navigation), [`render`](Site::render), and
//!   page lookup methods. Designed for shared ownership (`Arc<Site>`) and
//!   concurrent access.
//! - [`PageRendererConfig`] — controls title extraction, diagram rendering
//!   (Kroki URL, DPI), and `PlantUML` include directories.
//! - [`PageRenderResult`] — the output of rendering a page: HTML, title,
//!   table of contents, breadcrumbs, and metadata.
//! - [`Navigation`] — a scoped navigation tree with [`NavItem`] children,
//!   current [`ScopeInfo`], and optional parent scope for back-navigation.
//!
//! # Sections and scoped navigation
//!
//! A **section** is a sub-site with its own navigation sidebar. Any page
//! becomes a section root when its metadata includes a `kind` field (e.g.,
//! `kind: domain`). This `kind` value becomes the section's kind in its
//! [section ref](crate#sections-and-scoped-navigation) and in
//! [`Section::kind`]. Kind values are freeform strings.
//!
//! Sections are identified by **section ref** strings with the format
//! `kind:namespace/name` (e.g., `"domain:default/billing"`). The namespace
//! is currently always `default`; it is reserved for future use. The name
//! is the last path segment of the section root's URL path.
//!
//! When navigating, sections act as boundaries — [`Site::navigation`] scoped
//! to a section shows only that section's children, with child sections
//! appearing as leaf nodes (not expanded). The currently viewed section is
//! called the **scope** (see [`ScopeInfo`]); the [`Navigation`] response
//! includes the current scope and a parent scope for "back" navigation.
//!
//! Every site has an implicit **root section** with kind `"section"` and
//! name `"root"` (ref `"section:default/root"`). It is used as the scope
//! when no explicit section is defined at the site root, and as the
//! fallback for [`Site::get_section_ref`] when a page has no section
//! ancestor.
//!
//! See [`Section`] and [`Sections`] (re-exported from [`rw_sections`]) for
//! the ref format and lookup API.
//!
//! # Virtual pages
//!
//! A **virtual page** is a page with no markdown content
//! ([`has_content`](PageRenderResult::has_content) is `false`). Virtual pages
//! appear when a directory has child content but no `index.md` of its own.
//! They render as a bare `<h1>` title and participate in navigation and
//! breadcrumbs like any other page.
//!
//! # Crate consumers
//!
//! This crate is used by `rw-server` (the HTTP server) and `rw-napi`
//! (the Node.js native addon). Both wrap [`Site`] in `Arc` and call
//! [`Site::invalidate`] when the underlying storage changes.
//!
//! # Lazy reload
//!
//! [`Site`] uses a lazy reload pattern. Callers signal that the underlying
//! storage has changed by calling [`Site::invalidate`], but the actual reload
//! happens on the next read operation ([`navigation`](Site::navigation),
//! [`render`](Site::render), etc.). Concurrent readers continue using the
//! previous snapshot until the reload completes.
//!
//! # Examples
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use rw_site::{Site, PageRendererConfig};
//! use rw_cache::NullCache;
//! use rw_storage_fs::FsStorage;
//!
//! // Create a site backed by the local filesystem
//! let storage = Arc::new(FsStorage::new(PathBuf::from("docs")));
//! let config = PageRendererConfig::default();
//! let cache = Arc::new(NullCache);
//! let site = Arc::new(Site::new(storage, cache, config));
//!
//! // Fetch root navigation (triggers initial load from storage)
//! let nav = site.navigation(None)?;
//!
//! // Render a page by URL path (without leading slash)
//! let result = site.render("guide")?;
//! println!("Title: {:?}", result.title);
//! println!("HTML length: {}", result.html.len());
//! # Ok(())
//! # }
//! ```

pub(crate) mod page;
pub(crate) mod site;
pub(crate) mod site_state;

pub use page::{BreadcrumbItem, PageRenderResult, PageRendererConfig, RenderError, SearchDocument};

/// A section identity consisting of a freeform `kind` and a `name`
/// (the last path segment of the section root). Parsed from and
/// serialized to section ref strings like `"domain:default/billing"`.
pub use rw_sections::Section;

/// A parsed section ref broken into its `kind`, `namespace`, and `name`
/// components. See the [sections overview](crate#sections-and-scoped-navigation).
pub use rw_sections::SectionPath;

/// A map from URL paths to [`Section`] values, supporting lookup by
/// section ref string and prefix-based path matching.
pub use rw_sections::Sections;

pub use site::Site;
pub use site_state::{NavItem, Navigation, ScopeInfo};

/// A heading entry for building a table-of-contents sidebar.
///
/// Contains the heading `title`, `id` (anchor), and `level` (2–6).
pub use rw_renderer::TocEntry;
