//! Tabbed content blocks for markdown.
//!
//! Implements `CommonMark` directive syntax for tabs:
//!
//! ```markdown
//! :::tab[macOS]
//! Install with Homebrew.
//! :::tab[Linux]
//! Install with apt.
//! :::
//! ```
//!
//! # Architecture
//!
//! The tabs system uses two-phase processing:
//!
//! 1. **Preprocessing**: Converts directive syntax to intermediate `<rw-tabs>` /
//!    `<rw-tab>` HTML elements that pass through pulldown-cmark unchanged.
//!
//! 2. **Post-processing**: Transforms the intermediate elements to accessible
//!    HTML with ARIA attributes.
//!
//! ## Using the Directive API (Recommended)
//!
//! Use [`TabsDirective`] with [`DirectiveProcessor`](crate::directive::DirectiveProcessor):
//!
//! ```
//! use rw_renderer::directive::{DirectiveProcessor, DirectiveProcessorConfig};
//! use rw_renderer::TabsDirective;
//!
//! let config = DirectiveProcessorConfig::default();
//! let mut processor = DirectiveProcessor::new(config)
//!     .with_container(TabsDirective::new());
//!
//! let input = r#":::tab[macOS]
//! Install with Homebrew.
//! :::tab[Linux]
//! Install with apt.
//! :::"#;
//!
//! let output = processor.process(input);
//! let mut html = output;
//! processor.post_process(&mut html);
//!
//! assert!(html.contains(r#"role="tablist""#));
//! ```
//!
//! ## Legacy API
//!
//! [`TabsPreprocessor`] and [`TabsProcessor`] are still available for backward
//! compatibility but using the directive API is recommended for new code.

mod directive;
mod fence;
mod preprocessor;
mod processor;

pub use directive::TabsDirective;
pub(crate) use fence::FenceTracker;
pub use preprocessor::{TabMetadata, TabsGroup, TabsPreprocessor};
pub use processor::TabsProcessor;
