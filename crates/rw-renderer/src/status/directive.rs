//! Status badge inline directive.
//!
//! Implements the `:status[Label]{color=NAME}` inline directive — colored
//! pill labels mirroring Confluence's `status` macro.

use std::fmt;

use crate::directive::{
    DirectiveArgs, DirectiveContext, DirectiveOutput, InlineDirective, Replacements,
};
use crate::state::escape_html;

/// One of the six Confluence-native status colors. [`Default`] is Grey;
/// [`From<&str>`] parses a name case-insensitively and returns the default
/// for unknown or empty input. [`Display`](fmt::Display) writes the lowercase
/// name (used for the `data-color` attribute and CSS class suffix).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StatusColor {
    #[default]
    Grey,
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl StatusColor {
    /// All six colors, in palette order. Used to register post-processing
    /// replacements for every possible `<rw-status>` marker.
    const ALL: [StatusColor; 6] = [
        StatusColor::Grey,
        StatusColor::Red,
        StatusColor::Yellow,
        StatusColor::Green,
        StatusColor::Blue,
        StatusColor::Purple,
    ];
}

impl From<&str> for StatusColor {
    fn from(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "red" => Self::Red,
            "yellow" => Self::Yellow,
            "green" => Self::Green,
            "blue" => Self::Blue,
            "purple" => Self::Purple,
            // "grey", "gray", unknown values, and the empty string -> default (Grey).
            _ => Self::default(),
        }
    }
}

impl fmt::Display for StatusColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Grey => "grey",
            Self::Red => "red",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Purple => "purple",
        })
    }
}

/// Inline directive for status badges: `:status[Label]{color=NAME}`.
///
/// `process` emits a neutral `<rw-status data-color="X">label</rw-status>`
/// marker. `post_process` rewrites that marker to
/// `<span class="status status-X">…</span>` for HTML output; the Confluence
/// backend translates the same marker into a native `status` macro.
///
/// # Example
///
/// ```
/// use rw_renderer::directive::DirectiveProcessor;
/// use rw_renderer::StatusDirective;
///
/// let mut processor = DirectiveProcessor::new()
///     .with_inline(StatusDirective::new());
///
/// let mut html = processor.process(":status[On Track]{color=green}");
/// processor.post_process(&mut html);
/// assert_eq!(html, r#"<span class="status status-green">On Track</span>"#);
/// ```
#[derive(Debug, Default)]
pub struct StatusDirective;

impl StatusDirective {
    /// Create a new status directive handler.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl InlineDirective for StatusDirective {
    fn name(&self) -> &'static str {
        "status"
    }

    fn process(&mut self, args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
        let color = StatusColor::from(args.get("color").unwrap_or_default());
        // The label becomes element content and is re-parsed as markdown by
        // pulldown-cmark, so escape it to neutralize any HTML metacharacters.
        let label = escape_html(args.content().trim());
        DirectiveOutput::html(format!(
            r#"<rw-status data-color="{color}">{label}</rw-status>"#
        ))
    }

    fn post_process(&mut self, replacements: &mut Replacements) {
        // The open marker maps purely from its color, and the close marker is
        // constant — register all six unconditionally; Replacements skips any
        // pattern not present in the output.
        for color in StatusColor::ALL {
            replacements.add(
                format!(r#"<rw-status data-color="{color}">"#),
                format!(r#"<span class="status status-{color}">"#),
            );
        }
        replacements.add("</rw-status>", "</span>");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directive::DirectiveProcessor;

    #[test]
    fn test_from_known_colors() {
        assert_eq!(StatusColor::from("grey"), StatusColor::Grey);
        assert_eq!(StatusColor::from("red"), StatusColor::Red);
        assert_eq!(StatusColor::from("yellow"), StatusColor::Yellow);
        assert_eq!(StatusColor::from("green"), StatusColor::Green);
        assert_eq!(StatusColor::from("blue"), StatusColor::Blue);
        assert_eq!(StatusColor::from("purple"), StatusColor::Purple);
    }

    #[test]
    fn test_from_is_case_insensitive_and_trims() {
        assert_eq!(StatusColor::from("GREEN"), StatusColor::Green);
        assert_eq!(StatusColor::from("  Green  "), StatusColor::Green);
    }

    #[test]
    fn test_from_unknown_falls_back_to_grey() {
        assert_eq!(StatusColor::from("mauve"), StatusColor::Grey);
        assert_eq!(StatusColor::from(""), StatusColor::Grey);
    }

    #[test]
    fn test_default_is_grey() {
        assert_eq!(StatusColor::default(), StatusColor::Grey);
    }

    #[test]
    fn test_display() {
        assert_eq!(StatusColor::Grey.to_string(), "grey");
        assert_eq!(StatusColor::Red.to_string(), "red");
        assert_eq!(StatusColor::Yellow.to_string(), "yellow");
        assert_eq!(StatusColor::Green.to_string(), "green");
        assert_eq!(StatusColor::Blue.to_string(), "blue");
        assert_eq!(StatusColor::Purple.to_string(), "purple");
    }

    fn render(input: &str) -> String {
        let mut processor = DirectiveProcessor::new().with_inline(StatusDirective::new());
        let mut html = processor.process(input);
        processor.post_process(&mut html);
        html
    }

    #[test]
    fn test_status_renders_span() {
        assert_eq!(
            render(":status[On Track]{color=green}"),
            r#"<span class="status status-green">On Track</span>"#
        );
    }

    #[test]
    fn test_status_color_defaults_to_grey() {
        assert_eq!(
            render(":status[Declined]"),
            r#"<span class="status status-grey">Declined</span>"#
        );
    }

    #[test]
    fn test_status_unknown_color_falls_back_to_grey() {
        assert_eq!(
            render(":status[X]{color=mauve}"),
            r#"<span class="status status-grey">X</span>"#
        );
    }

    #[test]
    fn test_status_escapes_label() {
        let html = render(":status[<script>]{color=red}");
        assert!(html.contains("&lt;script&gt;"), "got: {html}");
        assert!(!html.contains("<script>"), "got: {html}");
    }

    #[test]
    fn test_status_trims_label() {
        assert_eq!(
            render(":status[  Spaced  ]{color=blue}"),
            r#"<span class="status status-blue">Spaced</span>"#
        );
    }

    #[test]
    fn test_status_empty_label() {
        assert_eq!(
            render(":status[]{color=blue}"),
            r#"<span class="status status-blue"></span>"#
        );
    }

    #[test]
    fn test_multiple_status_badges_on_one_line() {
        let html = render(":status[A]{color=red} and :status[B]{color=blue}");
        assert!(
            html.contains(r#"<span class="status status-red">A</span>"#),
            "got: {html}"
        );
        assert!(
            html.contains(r#"<span class="status status-blue">B</span>"#),
            "got: {html}"
        );
    }
}
