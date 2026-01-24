//! HTML entity to Unicode conversion.
//!
//! Converts named HTML entities to their Unicode equivalents for XML parsing.
//! Standard XML entities (amp, lt, gt, quot, apos) are preserved as-is.

use std::sync::LazyLock;

use regex::Regex;

/// Regex pattern for matching named HTML entities.
static ENTITY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&([a-zA-Z]+);").expect("invalid entity regex"));

/// Convert HTML entities to Unicode characters.
///
/// Replaces named HTML entities (e.g., `&nbsp;`, `&mdash;`) with their Unicode
/// equivalents. Standard XML entities (amp, lt, gt, quot, apos) are left unchanged.
pub fn convert_html_entities(html: &str) -> String {
    ENTITY_PATTERN
        .replace_all(html, |caps: &regex::Captures| {
            let entity_name = &caps[1];
            entity_to_unicode(entity_name)
                .map(String::from)
                .unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned()
}

/// Map HTML entity name to Unicode character.
fn entity_to_unicode(name: &str) -> Option<&'static str> {
    Some(match name {
        // Common entities
        "nbsp" => "\u{00a0}",
        "mdash" => "\u{2014}",
        "ndash" => "\u{2013}",
        "ldquo" => "\u{201c}",
        "rdquo" => "\u{201d}",
        "lsquo" => "\u{2018}",
        "rsquo" => "\u{2019}",
        "bull" => "\u{2022}",
        "hellip" => "\u{2026}",

        // Arrows
        "rarr" => "\u{2192}",
        "larr" => "\u{2190}",
        "harr" => "\u{2194}",
        "uarr" => "\u{2191}",
        "darr" => "\u{2193}",

        // Math symbols
        "le" => "\u{2264}",
        "ge" => "\u{2265}",
        "ne" => "\u{2260}",
        "plusmn" => "\u{00b1}",
        "times" => "\u{00d7}",
        "divide" => "\u{00f7}",

        // Legal symbols
        "copy" => "\u{00a9}",
        "reg" => "\u{00ae}",
        "trade" => "\u{2122}",

        // Currency
        "euro" => "\u{20ac}",
        "pound" => "\u{00a3}",
        "yen" => "\u{00a5}",
        "cent" => "\u{00a2}",

        // Misc symbols
        "deg" => "\u{00b0}",
        "para" => "\u{00b6}",
        "sect" => "\u{00a7}",
        "dagger" => "\u{2020}",
        "Dagger" => "\u{2021}",
        "laquo" => "\u{00ab}",
        "raquo" => "\u{00bb}",
        "iexcl" => "\u{00a1}",
        "iquest" => "\u{00bf}",

        // Fractions
        "frac14" => "\u{00bc}",
        "frac12" => "\u{00bd}",
        "frac34" => "\u{00be}",

        // Superscripts
        "sup1" => "\u{00b9}",
        "sup2" => "\u{00b2}",
        "sup3" => "\u{00b3}",

        // Other
        "acute" => "\u{00b4}",
        "micro" => "\u{00b5}",
        "middot" => "\u{00b7}",
        "cedil" => "\u{00b8}",
        "ordf" => "\u{00aa}",
        "ordm" => "\u{00ba}",

        // Unknown entity - return None to preserve as-is
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_nbsp() {
        assert_eq!(
            convert_html_entities("Hello&nbsp;World"),
            "Hello\u{00a0}World"
        );
    }

    #[test]
    fn test_convert_mdash() {
        assert_eq!(convert_html_entities("a&mdash;b"), "a\u{2014}b");
    }

    #[test]
    fn test_convert_multiple_entities() {
        assert_eq!(
            convert_html_entities("&copy; 2024 &mdash; All rights reserved"),
            "\u{00a9} 2024 \u{2014} All rights reserved"
        );
    }

    #[test]
    fn test_preserve_unknown_entities() {
        assert_eq!(convert_html_entities("&unknown;"), "&unknown;");
    }

    #[test]
    fn test_preserve_xml_entities() {
        // Standard XML entities should be preserved for XML parser
        assert_eq!(convert_html_entities("&amp;&lt;&gt;"), "&amp;&lt;&gt;");
    }

    #[test]
    fn test_no_entities() {
        assert_eq!(convert_html_entities("Hello World"), "Hello World");
    }
}
