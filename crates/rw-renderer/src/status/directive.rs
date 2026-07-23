//! Status badge inline directive.
//!
//! Implements the `:status[Label]{color=NAME}` inline directive — colored
//! pill labels mirroring Confluence's `status` macro.

use std::fmt;

/// The directive name the walker recognizes as the built-in status badge.
pub(crate) const STATUS_NAME: &str = "status";

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
    /// Inline directives are recognized by the parser and dispatched by the
    /// walker, so they only take effect end-to-end. `MarkdownRenderer` wraps
    /// single-paragraph input in `<p>…</p>` — the assertions below contain the
    /// resulting HTML as a substring.
    fn render(input: &str) -> String {
        MarkdownRenderer::<HtmlBackend>::new()
            .render(
                input,
                Pipeline::new().with_directives(DirectiveProcessor::new()),
            )
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
        // The badge's wrapper markup lands in the heading body; its label flows
        // through `text`, so it also contributes to the slug id and TOC title
        // (asserted by `test_status_inside_heading_sets_slug_and_toc`).
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
    fn test_status_inside_heading_sets_slug_and_toc() {
        // Discriminator: the badge label must reach the heading's plain-text
        // channel (id slug + TOC title), not only the rendered markup. A design
        // that routes the whole badge through the markup buffer drops the label
        // here and silently changes the heading id.
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "# Ship :status[On Track]{color=green}",
            Pipeline::new().with_directives(DirectiveProcessor::new()),
        );
        assert!(
            result.html.contains(r#"id="ship-on-track""#),
            "heading id must include the badge label; got: {}",
            result.html
        );
        assert_eq!(
            result.toc.first().map(|e| e.title.as_str()),
            Some("Ship On Track"),
            "TOC title must include the badge label; got: {:?}",
            result.toc
        );
        // The badge still renders as markup inside the heading body.
        assert!(
            result
                .html
                .contains(r#"<span class="status status-green">On Track</span>"#),
            "got: {}",
            result.html
        );
    }

    #[test]
    fn test_status_renders_with_an_empty_processor() {
        // Status is built-in: registering ANY processor (even one with no inline
        // handlers of its own) suffices — matches the Confluence pipeline's shape,
        // which never registers a `:status` handler either. Paired with
        // `test_status_stays_literal_without_a_processor` to pin the gate.
        let result = MarkdownRenderer::<HtmlBackend>::new().render(
            "Delivery is :status[On Track]{color=green}.",
            Pipeline::new().with_directives(DirectiveProcessor::new()),
        );
        assert!(
            result
                .html
                .contains(r#"<span class="status status-green">On Track</span>"#),
            "got: {}",
            result.html
        );
    }

    #[test]
    fn test_status_stays_literal_without_a_processor() {
        // No processor (e.g. comment bodies) => directive syntax is not tokenized,
        // so status stays literal instead of being stripped to its label.
        let result = MarkdownRenderer::<HtmlBackend>::new()
            .render(":status[On Track]{color=green}", Pipeline::new());
        assert!(
            result.html.contains(":status[On Track]{color=green}"),
            "status should stay literal without a processor; got: {}",
            result.html
        );
        assert!(
            !result.html.contains("status-green"),
            "got: {}",
            result.html
        );
    }
}
