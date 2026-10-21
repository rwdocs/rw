//! Which project files a page may name.

use std::collections::{BTreeMap, HashSet};
use std::mem;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rw_diagrams::diagram_language;
use rw_parser::{Event, LinkKind, Parser, Tag, TagEnd};
use rw_renderer::{
    HtmlBackend, IncludeError, IncludeReader, MarkdownRenderer, ProjectFiles, image_file,
};
use rw_storage::{Storage, StorageError, download_content_type, image_content_type};

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
    // Alt text drops the links in it.
    let mut in_alt = 0_usize;
    while let Some(event) = parser.next() {
        match event {
            Event::Start(Tag::Image { .. }) => in_alt += 1,
            Event::End(TagEnd::Image) => in_alt -= 1,
            Event::Start(Tag::Link {
                kind: LinkKind::Other,
                dest_url,
            }) if in_alt == 0 => {
                if let Some(Ok(path)) = image_file(&downloads, Some(source_path), &dest_url) {
                    files.insert(path);
                }
            }
            _ => {}
        }
    }
}

/// Where one page's `::include`s come from: the includes the storage read
/// with the page, or else the storage on demand.
pub(crate) struct PageIncludes {
    pub(crate) storage: Arc<dyn Storage>,
    /// [`PageText::includes`](rw_storage::PageText::includes).
    pub(crate) published: Option<BTreeMap<String, String>>,
}

impl PageIncludes {
    pub(crate) fn read(
        &self,
        path: &str,
        record: Option<&dyn Fn(&Path)>,
    ) -> Result<String, StorageError> {
        match &self.published {
            Some(published) => published
                .get(path)
                .cloned()
                .ok_or_else(|| StorageError::not_found(path)),
            None => self.storage.read_include(path, record),
        }
    }
}

/// The path and text of each markdown file that `markdown`, read from the
/// project file `source_path`, includes when it renders.
pub(crate) fn included_files(
    page_includes: PageIncludes,
    markdown: &str,
    source_path: &str,
) -> Vec<(String, String)> {
    if !markdown.contains("::include") {
        return Vec::new();
    }
    let included = Arc::new(Mutex::new(Vec::new()));
    // Rendering places each include as the page's render does.
    let _ = MarkdownRenderer::<HtmlBackend>::new()
        .with_source_path(source_path)
        .with_include_reader(IncludedFiles {
            page_includes,
            included: Arc::clone(&included),
        })
        .render(markdown);
    mem::take(&mut *included.lock())
}

struct IncludedFiles {
    page_includes: PageIncludes,
    included: Arc<Mutex<Vec<(String, String)>>>,
}

impl IncludeReader for IncludedFiles {
    fn read(&self, path: &str) -> Result<String, IncludeError> {
        let text = self
            .page_includes
            .read(path, None)
            .map_err(|_| IncludeError::NotFound)?;
        self.included.lock().push((path.to_owned(), text.clone()));
        Ok(text)
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
