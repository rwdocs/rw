//! Conversions between internal site paths and the URL paths consumers use,
//! and arithmetic on project file paths.

/// Convert an internal path to the URL path form consumers expect.
///
/// This crate stores paths without a leading slash (`"guide"`,
/// `"domain/page"`, `""` for the root page), while the HTTP API, the viewer,
/// and `@rwdocs/core` all expect them with one (`"/guide"`, `"/domain/page"`,
/// `"/"`). Every surface that serializes a site path crosses that boundary, so
/// the conversion lives here rather than being restated per consumer.
///
/// ```
/// # use rw_site::to_url_path;
/// assert_eq!(to_url_path(""), "/");
/// assert_eq!(to_url_path("guide"), "/guide");
/// ```
#[must_use]
pub fn to_url_path(path: &str) -> String {
    format!("/{path}")
}

/// `path` joined onto `base`, with `.` and `..` folded, or `None` when that
/// leaves the project. Both are `/`-separated and relative to the project root.
pub(crate) fn join_in_project(base: &str, path: &str) -> Option<String> {
    let mut segments = Vec::new();
    for segment in base.split('/').chain(path.split('/')) {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            _ => segments.push(segment),
        }
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_url_path_empty_returns_root() {
        assert_eq!(to_url_path(""), "/");
    }

    #[test]
    fn to_url_path_adds_leading_slash() {
        assert_eq!(to_url_path("guide"), "/guide");
        assert_eq!(to_url_path("guide/setup"), "/guide/setup");
    }
}
