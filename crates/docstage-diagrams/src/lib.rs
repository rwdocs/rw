//! Diagram rendering via Kroki for docstage.
//!
//! This crate provides diagram extraction and rendering for markdown documents:
//! - `DiagramProcessor` implements `CodeBlockProcessor` for extracting diagrams
//! - Parallel rendering via Kroki service (PlantUML, Mermaid, GraphViz, etc.)
//! - PlantUML preprocessing with `!include` resolution and DPI configuration
//! - HTML embedding with SVG scaling and placeholder replacement
//!
//! # Architecture
//!
//! The crate is organized into modules:
//! - [`language`]: Diagram type definitions (`DiagramLanguage`, `DiagramFormat`, `ExtractedDiagram`)
//! - [`processor`]: `DiagramProcessor` implementing `CodeBlockProcessor` trait
//! - [`kroki`]: Parallel HTTP rendering via Kroki service
//! - [`plantuml`]: PlantUML-specific preprocessing
//! - [`html_embed`]: HTML embedding with SVG scaling and placeholder replacement
//!
//! # Example
//!
//! ```ignore
//! use pulldown_cmark::Parser;
//! use docstage_diagrams::DiagramProcessor;
//! use docstage_renderer::{MarkdownRenderer, HtmlBackend};
//!
//! let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```";
//! let parser = Parser::new(markdown);
//! let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_processor(DiagramProcessor::new().kroki_url("https://kroki.io"));
//!
//! let result = renderer.render(parser);
//! let html = renderer.finalize(result.html); // Renders diagrams inline
//! ```

mod cache;
mod consts;
mod html_embed;
mod kroki;
mod language;
mod output;
mod plantuml;
mod processor;

pub use cache::{DiagramCache, FileCache, NullCache};
pub use output::{
    DiagramOutput, DiagramTagGenerator, FigureTagGenerator, ImgTagGenerator, RenderedDiagramInfo,
};
pub use processor::DiagramProcessor;
