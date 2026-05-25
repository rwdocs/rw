//! Status badge inline directive.
//!
//! Implements the `:status[Label]{color=NAME}` inline directive — colored
//! pill labels mirroring Confluence's `status` macro.

use std::fmt;

use crate::directive::{
    DirectiveArgs, DirectiveContext, DirectiveOutput, InlineDirective, Replacements,
};

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
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, StatusDirective};
/// use rw_renderer::directive::DirectiveProcessor;
///
/// let processor = DirectiveProcessor::new().with_inline(StatusDirective::new());
/// let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_directives(processor);
///
/// let result = renderer.render_markdown(":status[On Track]{color=green}");
/// assert!(result.html.contains(r#"<span class="status status-green">On Track</span>"#));
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
        // Emit as a marker triple — the renderer routes the label through its
        // `text` method, which HTML-escapes it. Backends with stateful
        // `raw_html` (Confluence) see the open and close markers as discrete
        // events so they can translate to native macros.
        DirectiveOutput::marker(
            format!(r#"<rw-status data-color="{color}">"#),
            args.content().trim(),
            "</rw-status>",
        )
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
    use crate::{HtmlBackend, MarkdownRenderer};

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

    /// Render `input` through the full `MarkdownRenderer` pipeline.
    ///
    /// Inline directives are expanded during the pulldown-cmark event
    /// stream (see [`DirectiveProcessor::transform_events`]), so they only
    /// take effect end-to-end. `MarkdownRenderer` wraps single-paragraph
    /// input in `<p>…</p>` — the assertions below contain the resulting
    /// HTML as a substring.
    fn render(input: &str) -> String {
        let processor = DirectiveProcessor::new().with_inline(StatusDirective::new());
        let mut renderer = MarkdownRenderer::<HtmlBackend>::new().with_directives(processor);
        renderer.render_markdown(input).html
    }

    #[test]
    fn test_status_renders_span() {
        let html = render(":status[On Track]{color=green}");
        assert!(
            html.contains(r#"<span class="status status-green">On Track</span>"#),
            "got: {html}"
        );
    }

    #[test]
    fn test_status_color_defaults_to_grey() {
        let html = render(":status[Declined]");
        assert!(
            html.contains(r#"<span class="status status-grey">Declined</span>"#),
            "got: {html}"
        );
    }

    #[test]
    fn test_status_unknown_color_falls_back_to_grey() {
        let html = render(":status[X]{color=mauve}");
        assert!(
            html.contains(r#"<span class="status status-grey">X</span>"#),
            "got: {html}"
        );
    }

    #[test]
    fn test_status_escapes_label() {
        // The directive only sees text characters that pulldown-cmark
        // delivers via `Event::Text` (raw HTML like `<…>` is delivered as
        // `Event::Html` and bypasses the directive entirely). Of the HTML
        // metacharacters, the only one that pulldown-cmark still passes as
        // text inside a paragraph is `&` — and the directive must escape
        // it so the resulting `<span>` doesn't end up holding a literal
        // entity reference written by the user.
        let html = render(":status[A &amp; B]{color=red}");
        // The directive's input is the textual content as pulldown delivers
        // it: pulldown normalizes `&amp;` to `&`, which the directive must
        // re-escape back to `&amp;` before placing it in the span body.
        assert!(
            html.contains(r#"<span class="status status-red">A &amp; B</span>"#),
            "got: {html}"
        );
    }

    #[test]
    fn test_status_trims_label() {
        let html = render(":status[  Spaced  ]{color=blue}");
        assert!(
            html.contains(r#"<span class="status status-blue">Spaced</span>"#),
            "got: {html}"
        );
    }

    #[test]
    fn test_status_empty_label() {
        let html = render(":status[]{color=blue}");
        assert!(
            html.contains(r#"<span class="status status-blue"></span>"#),
            "got: {html}"
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
