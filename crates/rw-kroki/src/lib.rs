//! Diagram rendering via Kroki for RW.
//!
//! This crate provides diagram extraction and rendering for markdown documents:
//! - [`KrokiProvider`] implements `rw_diagrams::DiagramProvider`, turning
//!   diagram source into rendered content
//! - `DiagramProcessor` implements `CodeBlockProcessor` for extracting diagrams
//! - Parallel rendering via Kroki service (`PlantUML`, Mermaid, `GraphViz`, etc.)
//! - `PlantUML` preprocessing with `!include` resolution and DPI configuration
//! - HTML embedding with SVG scaling and link annotation
//!
//! # Architecture
//!
//! The crate is organized into modules:
//! - [`language`]: Diagram type definitions (`DiagramLanguage`, `DiagramFormat`, `ExtractedDiagram`)
//! - [`provider`]: [`KrokiProvider`] implementing the `DiagramProvider` trait
//! - [`processor`]: `DiagramProcessor` implementing `CodeBlockProcessor` trait
//! - [`kroki`]: Parallel HTTP rendering via Kroki service
//! - [`search`]: `SearchDiagramProcessor` producing plain text for the search index
//! - [`html_embed`]: HTML embedding with SVG scaling and link annotation
//!
//! # Example
//!
//! ```no_run
//! use rw_kroki::DiagramProcessor;
//! use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
//!
//! let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```";
//! let renderer = MarkdownRenderer::<HtmlBackend>::new();
//! let pipeline = Pipeline::new()
//!     .with_processor(DiagramProcessor::new("https://kroki.io"));
//!
//! // render auto-calls fills() on all processors
//! let result = renderer.render(markdown, pipeline);
//! ```

mod cache;
mod consts;
mod html_embed;
mod kroki;
mod language;
mod output;
mod processor;
mod provider;
mod scale;
mod search;
#[cfg(test)]
mod test_support;

pub use output::{DiagramOutput, RenderedDiagramInfo, TagGenerator};
pub use processor::DiagramProcessor;
pub use provider::KrokiProvider;
pub use search::SearchDiagramProcessor;
