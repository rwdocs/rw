//! Site structure and page rendering for RW.
//!
//! This crate provides:
//! - [`Site`]: Unified site structure and page rendering
//! - Navigation tree building for UI presentation
//!
//! # Quick Start
//!
//! ```ignore
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use rw_site::{Site, SiteConfig};
//! use rw_cache::NullCache;
//! use rw_storage_fs::FsStorage;
//!
//! let storage = Arc::new(FsStorage::new(PathBuf::from("docs")));
//! let config = SiteConfig::default();
//! let cache = Arc::new(NullCache);
//! let site = Arc::new(Site::new(storage, config, cache));
//!
//! // Get navigation (root scope)
//! let nav = site.navigation("");
//!
//! // Render a page
//! let result = site.render("/guide")?;
//! ```

pub(crate) mod site;
pub(crate) mod site_state;

pub use site::{PageRenderResult, RenderError, Site, SiteConfig};
pub use site_state::{BreadcrumbItem, NavItem, Navigation, Page, ScopeInfo, SectionInfo};

// Re-export TocEntry from rw-renderer for convenience
pub use rw_renderer::TocEntry;
