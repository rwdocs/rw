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
//! A tab bar can only be rendered once every tab in the group is known, which
//! is not until the walk passes the group's closing `:::`. Tabs therefore emit
//! no markup for the bar during the walk; they reserve a *hole* — a recorded
//! offset in the output buffer — and fill it afterwards:
//!
//! 1. **Event walk**: each `:::tab[Label]` returns
//!    [`DirectiveOutput::Deferred`](crate::directive::DirectiveOutput::Deferred),
//!    reserving a hole for the group's tab bar (first tab only) and one for its
//!    own panel opening, and recording the label.
//!
//! 2. **Assembly**: after the walk, `fills()` renders the accessible ARIA
//!    markup for every hole, and the walker splices it in at the recorded
//!    offsets. No intermediate markers are ever emitted, so nothing can leak
//!    into the output.
//!
//! # Unclosed groups
//!
//! A group left unclosed by missing `:::` extends to the end of the document:
//! the directive processor closes it at end of input, so its markup stays
//! balanced (and a warning is emitted). Because the group has no explicit end,
//! everything after the last `:::tab[…]` is part of that tab's panel — content
//! the author meant to follow the group is absorbed into the last tab. That
//! panel is `hidden` unless its tab is the selected one, so the trailing
//! content can disappear from view until the reader clicks that tab. The fix is
//! to close the group with `:::`.
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
