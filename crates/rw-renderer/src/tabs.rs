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
//! A tab bar needs every label in the group, so `::::tabs` reserves a hole for
//! it and the walker fills it at the group's close.
//!
//! # Unclosed groups
//!
//! A `::::tabs` group or `:::tab` item left unclosed extends to the end of the
//! document (or its enclosing blockquote or list item), with a warning.
//! Trailing content then lands in the last panel, which is `hidden` unless it
//! is the first tab.
//!
//! # A walker built-in
//!
//! Tabs are recognized by the walker itself, like the `:status` badge. The
//! backend supplies the markup through its tab methods
//! ([`tabs_open`](crate::RenderBackend::tabs_open),
//! [`tab_panel_open`](crate::RenderBackend::tab_panel_open), and their closers),
//! so a backend without tab support renders the content without chrome:
//!
//! ```
//! use rw_renderer::{HtmlBackend, MarkdownRenderer};
//!
//! let md = "::::tabs\n\n:::tab[macOS]\n\nInstall with Homebrew.\n\n:::\n\n:::tab[Linux]\n\nInstall with apt.\n\n:::\n\n::::";
//! let result = MarkdownRenderer::<HtmlBackend>::new().render(md);
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
