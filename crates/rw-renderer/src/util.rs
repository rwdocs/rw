//! Shared utility functions for markdown rendering.

use pulldown_cmark::HeadingLevel;

/// Compute a relative URL from one page URL to another (RFC 3986).
///
/// Both `from` and `to` are URL paths without leading slash. Per RFC 3986 the
/// last segment of `from` is the current document — the base directory is
/// everything before it.
///
/// # Examples
///
/// ```
/// use rw_renderer::relative_path;
///
/// assert_eq!(relative_path("a/b", "a/c"), "c");
/// assert_eq!(relative_path("", "domains/billing"), "domains/billing");
/// assert_eq!(relative_path("guide", "guide"), "guide");
/// ```
pub fn relative_path(from: &str, to: &str) -> String {
    let from_segs: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    let to_segs: Vec<&str> = to.split('/').filter(|s| !s.is_empty()).collect();

    // RFC 3986: last segment of `from` is the document, drop it to get the base directory.
    // Trailing slash means the document is empty — all segments are the directory.
    let from_dir = if from.ends_with('/') || from_segs.is_empty() {
        &from_segs[..]
    } else {
        &from_segs[..from_segs.len() - 1]
    };

    let common = from_dir
        .iter()
        .zip(&to_segs)
        .take_while(|(a, b)| a == b)
        .count();

    let ups = from_dir.len() - common;
    let remaining = &to_segs[common..];

    let ups_part = "../".repeat(ups);
    let down_part = remaining.join("/");

    let result = format!("{ups_part}{down_part}");
    if result.is_empty() {
        "./".to_owned()
    } else {
        result
    }
}

/// Convert heading level enum to number (1-6).
#[must_use]
pub(crate) fn heading_level_to_num(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_path_deep_to_shallow() {
        assert_eq!(
            relative_path("domains/billing/adrs/adr-151", "domains/billing"),
            "../"
        );
    }

    #[test]
    fn test_relative_path_shallow_to_deep() {
        assert_eq!(
            relative_path("domains/billing", "domains/billing/adrs/ADR-147"),
            "billing/adrs/ADR-147"
        );
    }

    #[test]
    fn test_relative_path_root_to_nested() {
        assert_eq!(relative_path("", "domains/billing"), "domains/billing");
    }

    #[test]
    fn test_relative_path_nested_to_root() {
        assert_eq!(relative_path("domains/billing", ""), "../");
    }

    #[test]
    fn test_relative_path_same_page() {
        assert_eq!(relative_path("guide", "guide"), "guide");
    }

    #[test]
    fn test_relative_path_siblings() {
        assert_eq!(relative_path("guide", "faq"), "faq");
    }

    #[test]
    fn test_relative_path_siblings_nested() {
        assert_eq!(relative_path("a/b", "a/c"), "c");
    }

    #[test]
    fn test_relative_path_both_empty() {
        assert_eq!(relative_path("", ""), "./");
    }

    #[test]
    fn test_relative_path_trailing_slash_is_directory() {
        assert_eq!(
            relative_path("domains/billing/", "domains/billing/adrs/ADR-147"),
            "adrs/ADR-147"
        );
    }

    #[test]
    fn test_relative_path_trailing_slash_up() {
        assert_eq!(
            relative_path("domains/billing/adrs/", "domains/billing"),
            "../"
        );
    }

    #[test]
    fn test_relative_path_trailing_slash_same() {
        assert_eq!(relative_path("guide/", "guide"), "./");
    }

    #[test]
    fn test_relative_path_root_trailing_slash() {
        assert_eq!(relative_path("/", "domains/billing"), "domains/billing");
    }
}
