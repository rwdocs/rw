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
//! 1. **Event walk**: The tabs container is recognized during the pulldown-cmark
//!    event walk and emits intermediate `<rw-tabs>` / `<rw-tab>` markers.
//!
//! 2. **Post-processing**: Transforms the intermediate elements to accessible
//!    HTML with ARIA attributes.
//!
//! Register [`TabsDirective`] on a
//! [`DirectiveProcessor`](crate::directive::DirectiveProcessor), then render
//! through [`MarkdownRenderer`](crate::MarkdownRenderer):
//!
//! ```
//! use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
//! use rw_renderer::directive::DirectiveProcessor;
//! use rw_renderer::TabsDirective;
//!
//! let directives = DirectiveProcessor::new().with_container(TabsDirective::new());
//! let md = ":::tab[macOS]\n\nInstall with Homebrew.\n\n:::tab[Linux]\n\nInstall with apt.\n\n:::";
//! let result = MarkdownRenderer::<HtmlBackend>::new()
//!     .render(md, Pipeline::new().with_directives(directives));
//! assert!(result.html.contains(r#"role="tablist""#));
//! ```

mod directive;

pub use directive::TabsDirective;
