//! Tabbed content blocks for markdown.
//!
//! Implements CommonMark directive syntax for tabs:
//!
//! ```markdown
//! ::: tab macOS
//! Install with Homebrew.
//! ::: tab Linux
//! Install with apt.
//! :::
//! ```
//!
//! # Architecture
//!
//! The tabs system uses two-phase processing:
//!
//! 1. **Preprocessing** ([`TabsPreprocessor`]): Converts directive syntax to
//!    intermediate `<rw-tabs>` / `<rw-tab>` HTML elements that pass through
//!    pulldown-cmark unchanged.
//!
//! 2. **Post-processing** ([`TabsProcessor`]): Transforms the intermediate
//!    elements to accessible HTML with ARIA attributes.
//!
//! Note: `TabsProcessor` is a simple struct with explicit `post_process()` method,
//! not a `CodeBlockProcessor`. Tabs are container directives, not code blocks.
//!
//! # Usage
//!
//! ```
//! use rw_renderer::{HtmlBackend, MarkdownRenderer, TabsPreprocessor, TabsProcessor};
//!
//! let markdown = r#"
//! ::: tab macOS
//! Install with Homebrew.
//! ::: tab Linux
//! Install with apt.
//! :::
//! "#;
//!
//! // Phase 1: Preprocess directives
//! let mut preprocessor = TabsPreprocessor::new();
//! let processed = preprocessor.process(markdown);
//! let groups = preprocessor.into_groups();
//!
//! // Phase 2: Render markdown
//! let mut result = MarkdownRenderer::<HtmlBackend>::new()
//!     .render_markdown(&processed);
//!
//! // Phase 3: Post-process tabs
//! let mut tabs_processor = TabsProcessor::new(groups);
//! tabs_processor.post_process(&mut result.html);
//!
//! assert!(result.html.contains(r#"role="tablist""#));
//! ```

mod fence;
mod preprocessor;
mod processor;

pub use preprocessor::{TabMetadata, TabsGroup, TabsPreprocessor};
pub use processor::TabsProcessor;
