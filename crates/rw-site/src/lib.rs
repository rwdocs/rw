//! Site structure and page rendering for RW.
//!
//! This crate provides:
//! - [`Site`]: Unified site structure and page rendering
//! - [`SiteState`]: Document hierarchy with efficient path lookups
//! - Navigation tree building for UI presentation
//!
//! # Quick Start
//!
//! ```ignore
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use rw_site::{Site, SiteConfig};
//! use rw_storage::FsStorage;
//!
//! let storage = Arc::new(FsStorage::new(PathBuf::from("docs")));
//! let config = SiteConfig {
//!     cache_dir: Some(PathBuf::from(".cache")),
//!     version: "1.0.0".to_string(),
//!     ..Default::default()
//! };
//! let site = Arc::new(Site::new(storage, config));
//!
//! // Load site structure
//! let state = site.reload_if_needed();
//! let nav = state.navigation();
//!
//! // Render a page
//! let result = site.render("/guide")?;
//! ```

mod page_cache;
pub(crate) mod site;
mod site_cache;
pub(crate) mod site_state;

pub use site::{PageRenderResult, RenderError, Site, SiteConfig};
pub use site_state::{BreadcrumbItem, NavItem, Page, SiteState};

// Re-export TocEntry from rw-renderer for convenience
pub use rw_renderer::TocEntry;
