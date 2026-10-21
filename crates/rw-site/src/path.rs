//! Which project files a page may name.

use rw_diagrams::diagram_language;
use rw_storage::file_content_type;

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
    use super::image_may_name;

    #[test]
    fn a_page_may_name_any_diagram_file() {
        for path in [
            "charts/sales.vl.json",
            "charts/sales.VG.JSON",
            "art.bob",
            "db.er",
        ] {
            assert!(image_may_name(path), "{path}");
        }
        for path in ["data.json", "db.erd"] {
            assert!(!image_may_name(path), "{path}");
        }
    }
}
