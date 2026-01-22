//! High-performance markdown renderer for Docstage.
//!
//! This crate provides:
//! - **Confluence conversion**: [`MarkdownConverter`] for Confluence XHTML storage format
//! - **HTML rendering**: [`PageRenderer`] for semantic HTML5 with caching
//! - **Site management**: Document hierarchy with efficient path lookups
//!
//! # Confluence Conversion
//!
//! ```ignore
//! use std::path::Path;
//! use docstage_core::MarkdownConverter;
//!
//! let converter = MarkdownConverter::new()
//!     .prepend_toc(true)
//!     .extract_title(true);
//!
//! let result = converter.convert(
//!     "# Hello\n\n```plantuml\nA -> B\n```",
//!     Some("https://kroki.io"),
//!     Some(Path::new("/tmp/diagrams")),
//! );
//! ```
//!
//! # HTML Rendering
//!
//! For HTML output, use [`PageRenderer`] which provides file-based caching:
//!
//! ```ignore
//! use std::path::PathBuf;
//! use docstage_core::{PageRenderer, PageRendererConfig};
//!
//! let config = PageRendererConfig {
//!     cache_dir: Some(PathBuf::from(".cache")),
//!     kroki_url: Some("https://kroki.io".to_string()),
//!     ..Default::default()
//! };
//! let renderer = PageRenderer::new(config);
//! let result = renderer.render(Path::new("docs/guide.md"), "guide")?;
//! ```
//!
//! # Site Structure
//!
//! ```ignore
//! use std::path::PathBuf;
//! use docstage_core::site_loader::{SiteLoader, SiteLoaderConfig};
//! use docstage_core::navigation::build_navigation;
//!
//! let config = SiteLoaderConfig {
//!     source_dir: PathBuf::from("docs"),
//!     cache_dir: Some(PathBuf::from(".cache")),
//! };
//! let mut loader = SiteLoader::new(config);
//! let site = loader.load(true);
//! let nav = build_navigation(site);
//! ```

mod confluence_tags;
mod converter;
pub mod navigation;
mod page_cache;
mod page_renderer;
pub mod site;
mod site_cache;
pub mod site_loader;
pub mod updater;

#[allow(deprecated)]
pub use converter::{ConvertResult, MarkdownConverter};
pub use docstage_renderer::RenderResult;
pub use navigation::{NavItem, build_navigation};
pub use page_renderer::{PageRenderResult, PageRenderer, PageRendererConfig, RenderError};
pub use site::{BreadcrumbItem, Page, Site, SiteBuilder};
pub use site_cache::{FileSiteCache, NullSiteCache, SiteCache};
pub use site_loader::{SiteLoader, SiteLoaderConfig};
