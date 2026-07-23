//! Tabbed content blocks for markdown.
//!
//! Implements `CommonMark` directive syntax for tabs: an outer `::::tabs` group
//! wrapping self-closing `:::tab[Label]` items.
//!
//! ```markdown
//! ::::tabs
//! :::tab[macOS]
//! Install with Homebrew.
//! :::
//! :::tab[Linux]
//! Install with apt.
//! :::
//! ::::
//! ```
//!
//! # Architecture
//!
//! A tab bar can only be rendered once every tab in the group is known, which
//! is not until the walk passes the group's closing `::::`. `::::tabs`
//! therefore emits no markup for the bar during the walk; it reserves a
//! *hole* — a recorded offset in the output buffer — and fills it afterwards:
//!
//! 1. **Event walk**: `::::tabs` returns
//!    [`DirectiveOutput::Deferred`](crate::directive::DirectiveOutput::Deferred),
//!    reserving a hole for the group's tab bar. Each nested `:::tab[Label]` is
//!    an ordinary self-closing container: its `start` emits the panel's
//!    opening `<div role="tabpanel">` inline and records the label, its `end`
//!    the panel's closing `</div>`.
//!
//! 2. **Assembly**: after the walk, `fills()` renders the accessible ARIA
//!    markup for the tab bar, and the walker splices it in at the recorded
//!    offset. No intermediate markers are ever emitted, so nothing can leak
//!    into the output.
//!
//! # Unclosed groups
//!
//! A `::::tabs` group left unclosed by a missing `::::` extends to the end of
//! the document (or its enclosing blockquote/list item): the directive
//! processor closes it there, so its markup stays balanced (and a warning is
//! emitted). A `:::tab` item left unclosed behaves the same way at the item
//! level. In that case, everything after the last `:::tab` is absorbed into
//! that panel — which is `hidden` unless it's the selected (first) tab, so
//! the trailing content can disappear from view until the reader clicks that
//! tab. The fix is to close each `:::tab` and the enclosing `::::tabs`.
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
//! let md = "::::tabs\n\n:::tab[macOS]\n\nInstall with Homebrew.\n\n:::\n\n:::tab[Linux]\n\nInstall with apt.\n\n:::\n\n::::";
//! let result = MarkdownRenderer::<HtmlBackend>::new()
//!     .render(md, Pipeline::new().with_directives(directives));
//! assert!(result.html.contains(r#"role="tablist""#));
//! ```

mod directive;

pub use directive::TabsDirective;
