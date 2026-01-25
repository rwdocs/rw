//! Tabbed content blocks for markdown.
//!
//! Implements CommonMark directive syntax for tabs:
//!
//! ```markdown
//! ::: tabs
//! ::: tab macOS
//! Install with Homebrew.
//! :::
//! ::: tab Linux
//! Install with apt.
//! :::
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
//! # Usage
//!
//! ```
//! use rw_renderer::{HtmlBackend, MarkdownRenderer, TabsPreprocessor, TabsProcessor};
//!
//! let markdown = r#"
//! ::: tabs
//! ::: tab macOS
//! Install with Homebrew.
//! :::
//! ::: tab Linux
//! Install with apt.
//! :::
//! :::
//! "#;
//!
//! // Phase 1: Preprocess directives
//! let mut preprocessor = TabsPreprocessor::new();
//! let processed = preprocessor.process(markdown);
//! let groups = preprocessor.into_groups();
//!
//! // Phase 2: Render with post-processor
//! let result = MarkdownRenderer::<HtmlBackend>::new()
//!     .with_processor(TabsProcessor::new(groups))
//!     .render_markdown(&processed);
//!
//! assert!(result.html.contains(r#"role="tablist""#));
//! ```

mod fence;
mod preprocessor;
mod processor;

pub use preprocessor::{TabMetadata, TabsGroup, TabsPreprocessor};
pub use processor::TabsProcessor;
