//! High-performance markdown renderer for Docstage.
//!
//! This crate provides a markdown parser and Confluence storage format renderer.

mod confluence;
mod converter;
mod kroki;
mod plantuml;
mod plantuml_filter;

pub use confluence::{ConfluenceRenderer, RenderResult};
pub use converter::{ConvertResult, DiagramInfo, MarkdownConverter, create_image_tag};
pub use kroki::{DiagramRequest, RenderError, RenderedDiagram, render_all};
pub use plantuml::{load_config_file, prepare_diagram_source};
pub use plantuml_filter::{ExtractedDiagram, PlantUmlFilter};
