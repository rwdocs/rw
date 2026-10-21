//! Local images copied into the bundle as attachments.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rw_renderer::{ImageSrc, ProjectFiles};
use rw_storage::{Storage, file_content_type};
use sha2::{Digest, Sha256};

/// Where [`render`](fn@crate::render) finds the files a page's local images
/// name: diagram files to render, and image files to copy into the bundle.
#[derive(Clone)]
pub struct LocalImages {
    /// Places each image's file in the project.
    pub files: ProjectFiles,
    /// The markdown file's path, relative to the source directory of
    /// `files`, which relative image paths resolve against.
    pub source_path: String,
    /// Reads image files by their path in the project, to bundle them. `None`
    /// leaves every image file to the publisher, as its refusals, such as a
    /// symlink out of the project, leave that one.
    pub storage: Option<Arc<dyn Storage>>,
    /// The project root on disk, which diagram files (`![alt](flow.puml)`)
    /// are read from.
    pub project_dir: PathBuf,
}

impl fmt::Debug for LocalImages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalImages")
            .field("files", &self.files)
            .field("source_path", &self.source_path)
            .field("project_dir", &self.project_dir)
            .finish_non_exhaustive()
    }
}

/// Names each image file after its content, and keeps it until the page shows
/// which names it references: an image in a title H1 the backend leaves out
/// is read, but must not be bundled.
pub(crate) struct ImageAttachments {
    storage: Arc<dyn Storage>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The attachment name of each file read, or `None` for one that could
    /// not be.
    names: HashMap<String, Option<String>>,
    /// The project path and content of each attachment name.
    files: HashMap<String, (String, Vec<u8>)>,
}

impl ImageAttachments {
    pub(crate) fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            state: Mutex::default(),
        }
    }

    /// The project path and content of the attachment `name`, if an image
    /// was given that name.
    pub(crate) fn take(&self, name: &str) -> Option<(String, Vec<u8>)> {
        self.state.lock().files.remove(name)
    }
}

/// `image_<digest[..12]>.<ext>`: hashing the content keeps two `logo.png`
/// files in different folders apart, and one file referenced twice one
/// attachment.
fn attachment_name(path: &str, content: &[u8]) -> String {
    let digest = hex::encode(Sha256::digest(content));
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    format!("image_{}.{extension}", &digest[..12])
}

/// Whether `name` has the shape [`attachment_name`] gives, so that a file
/// by that name in the bundle directory is one an earlier render wrote.
pub(crate) fn is_attachment_name(name: &str) -> bool {
    let Some((digest, extension)) = name
        .strip_prefix("image_")
        .and_then(|rest| rest.split_once('.'))
    else {
        return false;
    };
    digest.len() == 12
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && !extension.bytes().any(|b| b.is_ascii_uppercase())
        && file_content_type(name).is_some()
}

/// The [`ImageSrc`] for one render, sharing its [`ImageAttachments`] with the
/// caller that writes them out afterwards.
pub(crate) struct ImageNames(pub(crate) Arc<ImageAttachments>);

impl ImageSrc for ImageNames {
    fn image_src(&self, path: &str, _src: &str) -> Option<String> {
        let mut state = self.0.state.lock();
        if let Some(name) = state.names.get(path) {
            return name.clone();
        }
        let name = self.0.storage.read_file(path).ok().map(|content| {
            let name = attachment_name(path, &content);
            state
                .files
                .entry(name.clone())
                .or_insert_with(|| (path.to_owned(), content));
            name
        });
        state.names.insert(path.to_owned(), name.clone());
        name
    }
}

#[cfg(test)]
mod tests {
    use rw_storage::MockStorage;

    use super::*;

    fn names(storage: MockStorage) -> ImageNames {
        ImageNames(Arc::new(ImageAttachments::new(Arc::new(storage))))
    }

    #[test]
    fn an_attachment_is_named_after_its_content_and_lowercased_extension() {
        let names = names(MockStorage::new().with_project_file("img/Logo.PNG", "logo"));
        let digest = hex::encode(Sha256::digest(b"logo"));
        assert_eq!(
            names.image_src("img/Logo.PNG", "Logo.PNG"),
            Some(format!("image_{}.png", &digest[..12]))
        );
    }

    #[test]
    fn the_same_content_at_two_paths_is_one_attachment() {
        let names = names(
            MockStorage::new()
                .with_project_file("a/logo.png", "same")
                .with_project_file("b/logo.png", "same"),
        );
        let a = names.image_src("a/logo.png", "a/logo.png").unwrap();
        let b = names.image_src("b/logo.png", "b/logo.png").unwrap();
        assert_eq!(a, b);
        assert_eq!(
            names.0.take(&a).map(|(path, _)| path).as_deref(),
            Some("a/logo.png")
        );
        assert_eq!(names.0.take(&a), None);
    }

    #[test]
    fn only_names_an_image_gets_are_attachment_names() {
        assert!(is_attachment_name(&attachment_name("a/Logo.SVG", b"logo")));
        for name in [
            "image_notes.txt",
            "image_0123456789ab.txt",
            "image_0123456789ab.PNG",
            "image_0123456789AB.png",
            "image_0123456789a.png",
            "image_0123456789abc.png",
            "image_0123456789ab",
            "diagram_0123456789ab.png",
        ] {
            assert!(!is_attachment_name(name), "{name}");
        }
    }

    #[test]
    fn a_file_storage_cannot_read_keeps_its_src() {
        let names = names(MockStorage::new());
        assert_eq!(names.image_src("missing.png", "missing.png"), None);
    }
}
