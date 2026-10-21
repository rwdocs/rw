//! Status badge (`:status[Label]{color=NAME}`), a built-in inline element —
//! colored pill labels mirroring Confluence's `status` macro.

use std::fmt;

/// The directive name the walker recognizes as the built-in status badge.
pub(crate) const STATUS_NAME: &str = "status";

/// One of the six Confluence-native status colors. [`Default`] is Grey;
/// [`From<&str>`] parses a name case-insensitively and returns the default
/// for unknown or empty input. [`Display`](fmt::Display) writes the lowercase
/// name (the HTML backend's CSS class suffix).
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
    use crate::{HtmlBackend, MarkdownRenderer};

    #[test]
    fn test_name_round_trip() {
        for (name, color) in [
            ("grey", StatusColor::Grey),
            ("red", StatusColor::Red),
            ("yellow", StatusColor::Yellow),
            ("green", StatusColor::Green),
            ("blue", StatusColor::Blue),
            ("purple", StatusColor::Purple),
        ] {
            assert_eq!(StatusColor::from(name), color);
            assert_eq!(color.to_string(), name);
        }
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

    /// Render `input` through the full `MarkdownRenderer` pipeline.
    fn render(input: &str) -> String {
        MarkdownRenderer::<HtmlBackend>::new().render(input).html
    }

    #[test]
    fn test_status_escapes_label() {
        // cmark decodes `&amp;` to `&`, so the label must be re-escaped.
        let html = render(":status[A &amp; B]{color=red}");
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
        // The label must reach the heading's plain-text channel too, or the
        // heading id changes.
        let result =
            MarkdownRenderer::<HtmlBackend>::new().render("## Ship :status[On Track]{color=green}");
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
}
