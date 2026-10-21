//! Which project files a page may name.

use std::collections::HashSet;

use rw_diagrams::diagram_language;
use rw_parser::{Event, LinkKind, Parser, Tag};
use rw_renderer::{ProjectFiles, image_file};
use rw_storage::{download_content_type, image_content_type};

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
    image_content_type(path).is_some() || diagram_language(path).is_some()
}

/// Adds to `files` each download a link in `markdown`, read from the project
/// file `source_path`, names, placed as its images are.
pub(crate) fn linked_downloads(markdown: &str, source_path: &str, files: &mut HashSet<String>) {
    let downloads = ProjectFiles::new(|path| download_content_type(path).is_some());
    // Wikilinks and directives as the page renders, so the same links count.
    let mut parser = Parser::new(markdown, true, true);
    while let Some(event) = parser.next() {
        if let Event::Start(Tag::Link {
            kind: LinkKind::Other,
            dest_url,
        }) = event
            && let Some(Ok(path)) = image_file(&downloads, Some(source_path), &dest_url)
        {
            files.insert(path);
        }
    }
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
        for path in ["data.json", "db.erd", "spec.pdf"] {
            assert!(!image_may_name(path), "{path}");
        }
    }
}
