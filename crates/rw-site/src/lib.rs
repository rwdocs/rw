//! Site structure and page rendering for RW.
//!
//! This crate provides:
//! - [`Site`]: Unified site structure and page rendering
//! - Navigation tree building for UI presentation
//!
//! # Quick Start
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use rw_site::{Site, PageRendererConfig};
//! use rw_cache::NullCache;
//! use rw_storage_fs::FsStorage;
//!
//! let storage = Arc::new(FsStorage::new(PathBuf::from("docs")));
//! let config = PageRendererConfig::default();
//! let cache = Arc::new(NullCache);
//! let site = Arc::new(Site::new(storage, cache, config));
//!
//! // Get navigation (root scope)
//! let nav = site.navigation("");
//!
//! // Render a page
//! let result = site.render("guide")?;
//! # Ok(())
//! # }
//! ```

pub(crate) mod page;
pub(crate) mod site;
pub(crate) mod site_state;
mod typed_page_registry;

pub use page::{BreadcrumbItem, Page, PageRenderResult, PageRendererConfig, RenderError};
pub use site::Site;
pub use site_state::{NavItem, Navigation, ScopeInfo, SectionInfo};
pub use typed_page_registry::TypedPageRegistry;

// Re-export TocEntry from rw-renderer for convenience
pub use rw_renderer::TocEntry;
