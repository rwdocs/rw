//! Markdown to Confluence converter using pulldown-cmark.
//!
//! This crate provides a high-performance markdown parser and Confluence
//! storage format renderer, exposed to Python via PyO3.

mod confluence;
mod plantuml;
mod python;

pub use confluence::{ConfluenceRenderer, DiagramInfo, RenderResult};
pub use plantuml::{load_config_file, prepare_diagram_source, resolve_includes};
pub use python::md2conf_core;
