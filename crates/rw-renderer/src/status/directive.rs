//! Status badge inline directive.
//!
//! Implements the `:status[Label]{color=NAME}` inline directive — colored
//! pill labels mirroring Confluence's `status` macro.

use std::fmt;

use crate::directive::{DirectiveArgs, DirectiveContext, DirectiveOutput, InlineDirective, Marker};

/// Semantic name of the marker `StatusDirective` emits. Backends match on this
/// to render a status badge their own way.
pub const STATUS_MARKER: &str = "status";

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

impl From<&str> for StatusColor {
    fn from(name: &str) -> Self {
        const NAMED: [(&str, StatusColor); 5] = [
            ("red", StatusColor::Red),
            ("yellow", StatusColor::Yellow),
            ("green", StatusColor::Green),
            ("blue", StatusColor::Blue),
            ("purple", StatusColor::Purple),
        ];
        let name = name.trim();
        NAMED
            .into_iter()
            .find(|(label, _)| name.eq_ignore_ascii_case(label))
            .map_or_else(
                // "grey", "gray", unknown values, and the empty string -> default (Grey).
                Self::default,
                |(_, color)| color,
            )
    }
}

/// Reads a status marker's `color` attribute. Both backends translate the same
/// marker, so the derivation lives here rather than being repeated per backend.
impl From<&Marker> for StatusColor {
    fn from(marker: &Marker) -> Self {
        Self::from(marker.attr("color").unwrap_or_default())
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
/// `process` emits a backend-neutral [`Marker`] named [`STATUS_MARKER`] with a
/// normalized `color` attribute. `HtmlBackend` renders it as
/// `<span class="status status-X">…</span>`; the Confluence backend translates
/// the same marker into a native `status` macro.
///
/// # Example
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline, StatusDirective};
/// use rw_renderer::directive::DirectiveProcessor;
///
/// let processor = DirectiveProcessor::new().with_inline(StatusDirective::new());
/// let renderer = MarkdownRenderer::<HtmlBackend>::new();
///
/// let result = renderer.render(
///     ":status[On Track]{color=green}",
///     Pipeline::new().with_directives(processor),
/// );
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
        // The label goes in the marker body, not in an attr: the renderer
        // routes bodies through `text`, which the backend HTML-escapes. Attrs
        // are not escaped — a backend interpolating one must validate it.
        DirectiveOutput::marker(
            Marker::new(STATUS_MARKER).with_attr("color", color.to_string()),
            args.content().trim(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directive::DirectiveProcessor;
    use crate::{HtmlBackend, MarkdownRenderer, Pipeline};

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
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        renderer
            .render(input, Pipeline::new().with_directives(processor))
            .html
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

    #[test]
    fn test_status_inside_heading_renders_into_the_heading_body() {
        // The marker is markup, so it routes to the heading's rendered_html
        // buffer — not to the TOC text or the slug id.
        let html = render("# Ship :status[On Track]{color=green}\n");
        assert!(
            html.contains(r#"<span class="status status-green">On Track</span>"#),
            "got: {html}"
        );
        assert!(html.contains("<h1"), "got: {html}");
    }

    #[test]
    fn test_status_inside_image_alt_text_drops_markup() {
        // CommonMark: alt text is plain text. The marker is suppressed while
        // its label survives, so no markup leaks into the alt attribute.
        let html = render("![before :status[Badge]{color=red} after](x.png)\n");
        assert!(!html.contains("<span"), "markup leaked into alt: {html}");
        assert!(
            !html.contains("status-red"),
            "markup leaked into alt: {html}"
        );
        assert!(html.contains("Badge"), "label lost: {html}");
    }

    #[test]
    fn test_html_backend_unset_color_renders_grey() {
        use crate::directive::Marker;
        use crate::{HtmlBackend, RenderBackend};

        // Matches the Confluence backend rather than emitting a `status-`
        // class no stylesheet matches.
        let mut out = String::new();
        HtmlBackend::marker_open(&Marker::new(STATUS_MARKER), &mut out);
        assert_eq!(out, r#"<span class="status status-grey">"#);
    }

    #[test]
    fn test_html_backend_color_cannot_inject_markup() {
        use crate::directive::Marker;
        use crate::{HtmlBackend, RenderBackend};

        // Normalizing through StatusColor means the emitted class is one of
        // six literals by construction, whatever the attribute holds.
        let mut out = String::new();
        HtmlBackend::marker_open(
            &Marker::new(STATUS_MARKER).with_attr("color", r#"x"><script>alert(1)</script>"#),
            &mut out,
        );
        assert_eq!(out, r#"<span class="status status-grey">"#);
    }

    #[test]
    fn test_html_backend_ignores_unrecognized_marker() {
        use crate::directive::Marker;
        use crate::{HtmlBackend, RenderBackend};

        let mut open = String::new();
        let mut close = String::new();
        let marker = Marker::new("kbd");
        HtmlBackend::marker_open(&marker, &mut open);
        HtmlBackend::marker_close(&marker, &mut close);
        assert!(open.is_empty(), "got: {open}");
        assert!(close.is_empty(), "got: {close}");
    }
}
