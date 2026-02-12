//! Diagram rendering via Kroki for RW.
//!
//! This crate provides diagram extraction and rendering for markdown documents:
//! - `DiagramProcessor` implements `CodeBlockProcessor` for extracting diagrams
//! - Parallel rendering via Kroki service (`PlantUML`, Mermaid, `GraphViz`, etc.)
//! - `PlantUML` preprocessing with `!include` resolution and DPI configuration
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
//! ```no_run
//! use rw_diagrams::DiagramProcessor;
//! use rw_renderer::{MarkdownRenderer, HtmlBackend};
//!
//! let markdown = "```plantuml\n@startuml\nA -> B\n@enduml\n```";
//! let mut renderer = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_processor(DiagramProcessor::new("https://kroki.io"));
//!
//! // render_markdown auto-calls post_process() on all processors
//! let result = renderer.render_markdown(markdown);
//! ```

mod cache;
mod consts;
mod html_embed;
mod kroki;
mod language;
mod meta_includes;
mod output;
mod plantuml;
mod processor;

pub use cache::DiagramKey;
pub use meta_includes::{EntityInfo, LinkConfig, MetaIncludeSource, resolve_meta_include};
pub use output::{DiagramOutput, DiagramTagGenerator, RenderedDiagramInfo};
pub use processor::{DiagramProcessor, to_extracted_diagram, to_extracted_diagrams};
