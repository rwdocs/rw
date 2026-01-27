//! Site structure and page rendering for RW.
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
//! use std::sync::Arc;
//! use rw_site::{SiteLoader, SiteLoaderConfig};
//! use rw_storage::FsStorage;
//!
//! let storage = Arc::new(FsStorage::new(PathBuf::from("docs")));
//! let config = SiteLoaderConfig {
//!     cache_dir: Some(PathBuf::from(".cache")),
//! };
//! let loader = SiteLoader::new(storage, config);
//! let site = loader.reload_if_needed();
//! let nav = site.navigation();
//! ```
//!
//! # Page Rendering
//!
//! ```ignore
//! use std::path::{Path, PathBuf};
//! use std::sync::Arc;
//! use rw_site::{PageRenderer, PageRendererConfig};
//! use rw_storage::FsStorage;
//!
//! let storage = Arc::new(FsStorage::new(PathBuf::from("docs")));
//! let config = PageRendererConfig {
//!     cache_dir: Some(PathBuf::from(".cache")),
//!     kroki_url: Some("https://kroki.io".to_string()),
//!     ..Default::default()
//! };
//! let renderer = PageRenderer::new(storage, config);
//! let result = renderer.render(Path::new("guide.md"), "guide")?;
//! ```

mod page_cache;
mod renderer;
pub(crate) mod site;
mod site_cache;
pub(crate) mod site_loader;

pub use renderer::{PageRenderResult, PageRenderer, PageRendererConfig, RenderError};
pub use site::{BreadcrumbItem, NavItem, Page, Site};
pub use site_loader::{SiteLoader, SiteLoaderConfig};
