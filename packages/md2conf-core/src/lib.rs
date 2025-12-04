//! Markdown to Confluence converter using pulldown-cmark.
//!
//! This crate provides a high-performance markdown parser and Confluence
//! storage format renderer, exposed to Python via PyO3.

mod confluence;
mod kroki;
mod plantuml;
mod plantuml_filter;
mod python;

pub use confluence::{ConfluenceRenderer, RenderResult};
pub use kroki::{DiagramRequest, RenderError, RenderedDiagram, render_all};
pub use plantuml::{load_config_file, prepare_diagram_source, resolve_includes};
pub use plantuml_filter::{DiagramInfo, PlantUmlFilter};
pub use python::md2conf_core;
