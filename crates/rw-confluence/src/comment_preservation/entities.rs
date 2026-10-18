//! Named entities that storage format may reference.
//!
//! Storage format is XHTML with HTML's entities but no DTD declaring them, so
//! an XML parser knows only XML's five.

/// The replacement text of the entity `name`, or `None` if it is not known.
pub fn resolve_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",

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

        _ => return None,
    })
}
