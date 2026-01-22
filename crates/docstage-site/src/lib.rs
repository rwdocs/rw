//! Site structure and page rendering for Docstage.
//!
//! This crate provides:
//! - **Site structure**: Document hierarchy with efficient path lookups
//! - **Page rendering**: [`PageRenderer`] for HTML with file-based caching
//! - **Navigation**: Tree building for UI presentation
//!
//! # Site Structure
//!
//! ```ignore
//! use std::path::PathBuf;
//! use docstage_site::{SiteLoader, SiteLoaderConfig, build_navigation};
//!
//! let config = SiteLoaderConfig {
//!     source_dir: PathBuf::from("docs"),
//!     cache_dir: Some(PathBuf::from(".cache")),
//! };
//! let mut loader = SiteLoader::new(config);
//! let site = loader.load(true);
//! let nav = build_navigation(site);
//! ```
//!
//! # Page Rendering
//!
//! ```ignore
//! use std::path::PathBuf;
//! use docstage_site::{PageRenderer, PageRendererConfig};
//!
//! let config = PageRendererConfig {
//!     cache_dir: Some(PathBuf::from(".cache")),
//!     kroki_url: Some("https://kroki.io".to_string()),
//!     ..Default::default()
//! };
//! let renderer = PageRenderer::new(config);
//! let result = renderer.render(Path::new("docs/guide.md"), "guide")?;
//! ```

pub mod navigation;
mod page_cache;
mod renderer;
pub mod site;
mod site_cache;
pub mod site_loader;

pub use navigation::{NavItem, build_navigation};
pub use renderer::{PageRenderResult, PageRenderer, PageRendererConfig, RenderError};
pub use site::{BreadcrumbItem, Page, Site, SiteBuilder};
pub use site_cache::{FileSiteCache, NullSiteCache, SiteCache};
pub use site_loader::{SiteLoader, SiteLoaderConfig};
