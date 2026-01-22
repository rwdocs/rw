//! High-performance markdown renderer for Docstage.
//!
//! This crate provides a `CommonMark` parser with multiple output formats:
//! - **Confluence XHTML**: Storage format for Confluence REST API
//! - **Semantic HTML5**: Clean HTML with table of contents and heading anchors
//!
//! It also provides site structure management:
//! - **Site**: Document hierarchy with efficient path lookups
//! - **`SiteLoader`**: Filesystem scanning and caching
//! - **Navigation**: Tree builder for UI presentation
//!
//! # Quick Start
//!
//! ```ignore
//! use docstage_core::MarkdownConverter;
//!
//! let converter = MarkdownConverter::new().extract_title(true);
//! let result = converter.convert_html("# Hello\n\nWorld!");
//! assert_eq!(result.title, Some("Hello".to_string()));
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
//!
//! # Diagram Support
//!
//! Multiple diagram languages are supported via Kroki: `PlantUML`, Mermaid, `GraphViz`, etc.
//!
//! When converting to Confluence format, diagram code blocks are automatically
//! rendered via the Kroki service and replaced with image macros:
//!
//! ```ignore
//! use std::path::Path;
//! use docstage_core::MarkdownConverter;
//!
//! let converter = MarkdownConverter::new();
//! let result = converter.convert(
//!     "```plantuml\n@startuml\nA -> B\n@enduml\n```",
//!     "https://kroki.io",
//!     Path::new("/tmp/diagrams"),
//! )?;
//! ```
//!
//! For HTML output with rendered diagrams:
//!
//! ```ignore
//! let result = converter.convert_html_with_diagrams(
//!     "```mermaid\ngraph TD\n  A --> B\n```",
//!     "https://kroki.io",
//!     None,  // cache_dir
//!     None,  // base_path
//! );
//! // Result contains inline SVG diagrams
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
pub use converter::{ConvertResult, HtmlConvertResult, MarkdownConverter};
pub use docstage_renderer::RenderResult;
pub use navigation::{NavItem, build_navigation};
pub use page_renderer::{PageRenderResult, PageRenderer, PageRendererConfig, RenderError};
pub use site::{BreadcrumbItem, Page, Site, SiteBuilder};
pub use site_cache::{FileSiteCache, NullSiteCache, SiteCache};
pub use site_loader::{SiteLoader, SiteLoaderConfig};
