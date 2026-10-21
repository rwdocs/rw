//! Conversions between internal site paths and the URL paths consumers use,
//! and which project files a page may name.

use rw_diagrams::diagram_language;
use rw_storage::file_content_type;

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

/// Whether a page's image may name the project file at `path`: an image file,
/// which is shown, or a diagram file, which is rendered.
///
/// ```
/// # use rw_site::image_may_name;
/// assert!(image_may_name("img/flow.PNG"));
/// assert!(image_may_name("diagrams/flow.puml"));
/// assert!(!image_may_name(".env"));
/// ```
#[must_use]
pub fn image_may_name(path: &str) -> bool {
    file_content_type(path).is_some() || diagram_language(path).is_some()
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
