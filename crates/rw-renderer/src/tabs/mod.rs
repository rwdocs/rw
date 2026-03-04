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
//! Use [`TabsDirective`] with [`DirectiveProcessor`](crate::directive::DirectiveProcessor):
//!
//! ```
//! use rw_renderer::directive::DirectiveProcessor;
//! use rw_renderer::TabsDirective;
//!
//! let mut processor = DirectiveProcessor::new()
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

mod directive;
mod fence;

pub use directive::TabsDirective;
pub(crate) use fence::FenceTracker;
