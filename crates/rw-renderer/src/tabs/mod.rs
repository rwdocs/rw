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
//! 1. **Event walk**: on `::::tabs` the walker reserves a hole for the group's
//!    tab bar. Each nested `:::tab[Label]` opens the panel inline through the
//!    backend (`<div role="tabpanel">`) and records the label; its close emits
//!    the panel's closing `</div>`.
//!
//! 2. **Assembly**: after the walk, the walker renders the accessible ARIA
//!    markup for each group's tab bar through the backend and splices it in at
//!    the recorded offset. No intermediate markers are ever emitted, so nothing
//!    can leak into the output.
//!
//! # Unclosed groups
//!
//! A `::::tabs` group left unclosed by a missing `::::` extends to the end of
//! the document (or its enclosing blockquote/list item): the parser
//! synthesizes the close there, so its markup stays balanced (and a warning is
//! emitted). A `:::tab` item left unclosed behaves the same way at the item
//! level. In that case, everything after the last `:::tab` is absorbed into
//! that panel — which is `hidden` unless it's the selected (first) tab, so
//! the trailing content can disappear from view until the reader clicks that
//! tab. The fix is to close each `:::tab` and the enclosing `::::tabs`.
//!
//! # A walker built-in, not a registered directive
//!
//! Tabs are recognized by the [`Walker`](crate::MarkdownRenderer) itself — like
//! the `:status` badge — rather than through a
//! [`ContainerDirective`](crate::directive::ContainerDirective) registered on a
//! [`DirectiveProcessor`](crate::directive::DirectiveProcessor). The walker owns
//! the tab state and reserves the bar hole; the backend supplies the markup
//! through its tab methods ([`tabs_open`](crate::RenderBackend::tabs_open),
//! [`tab_panel_open`](crate::RenderBackend::tab_panel_open), and their closers),
//! so a backend that does not support tabs (e.g. the search-document backend)
//! renders their content without any chrome. A processor still has to be present
//! on the [`Pipeline`](crate::Pipeline) so directive syntax is tokenized, but no
//! tab handler needs registering:
//!
//! ```
//! use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
//! use rw_renderer::directive::DirectiveProcessor;
//!
//! let md = "::::tabs\n\n:::tab[macOS]\n\nInstall with Homebrew.\n\n:::\n\n:::tab[Linux]\n\nInstall with apt.\n\n:::\n\n::::";
//! let result = MarkdownRenderer::<HtmlBackend>::new()
//!     .render(md, Pipeline::new().with_directives(DirectiveProcessor::new()));
//! assert!(result.html.contains(r#"role="tablist""#));
//! ```

/// One tab within a group, as handed to the backend's tab methods.
pub struct TabInfo {
    /// Document-global tab id, used in element ids.
    pub id: usize,
    /// Display label from `:::tab[Label]`.
    pub label: String,
    /// First tab in its group (selected, not `hidden`).
    pub is_first: bool,
}

/// Directive name of a tab group container (`::::tabs`).
pub(crate) const TABS_NAME: &str = "tabs";
/// Directive name of a tab item (`:::tab[Label]`).
pub(crate) const TAB_NAME: &str = "tab";
