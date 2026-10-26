//! The project files [`Storage::read_file`](crate::Storage::read_file) may
//! serve and [`Storage::read_include`](crate::Storage::read_include) may read.

use std::fmt;
use std::path::Path;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// The project root also holds `.env`, `.git/` and `.rw/comments/sqlite.db`,
/// and no extension in either list matches them.
const IMAGES: [(&str, &str); 7] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("svg", "image/svg+xml"),
];

/// Never a type a browser opens as a document on rw's origin (html, xml),
/// one config and secrets live in (txt, json, yaml, toml), or a macro-enabled
/// Office format.
const DOWNLOADS: [(&str, &str); 6] = [
    ("pdf", "application/pdf"),
    (
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    (
        "xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (
        "pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    ("csv", "text/csv"),
    ("zip", "application/zip"),
];

/// The largest project file [`Storage::read_file`](crate::Storage::read_file)
/// serves or [`Storage::read_include`](crate::Storage::read_include) reads,
/// since each holds the whole file in memory.
pub const MAX_FILE_SIZE: u64 = 32 * 1024 * 1024;

/// Why [`Storage::find_file`](crate::Storage::find_file) finds no file to
/// serve, worded to follow the file's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingFile {
    /// Missing, or refused for its type or place.
    NotFound,
    /// Larger than [`MAX_FILE_SIZE`].
    TooLarge,
    /// Found only because the filesystem ignores case, as on macOS and
    /// Windows but not on Linux or S3. `on_disk` is the file's project path
    /// as stored, `/`-separated.
    Misspelled { on_disk: String },
}

impl fmt::Display for MissingFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::TooLarge => write!(f, "is larger than {} MiB", MAX_FILE_SIZE >> 20),
            Self::Misspelled { on_disk } => {
                write!(f, "not found; the file on disk is `{on_disk}`")
            }
        }
    }
}

/// The content type of a project file, from its extension (matched
/// case-insensitively), or `None` when storage must refuse to serve it: an
/// [image](image_content_type) or a [download](download_content_type).
#[must_use]
pub fn file_content_type(path: &str) -> Option<&'static str> {
    image_content_type(path).or_else(|| download_content_type(path))
}

/// [`file_content_type`] of an image, which a page shows.
#[must_use]
pub fn image_content_type(path: &str) -> Option<&'static str> {
    content_type(&IMAGES, path)
}

/// [`file_content_type`] of a file a page links to, such as a pdf or a
/// spreadsheet.
#[must_use]
pub fn download_content_type(path: &str) -> Option<&'static str> {
    content_type(&DOWNLOADS, path)
}

/// Whether [`Storage::read_include`](crate::Storage::read_include) may read
/// `path`: its extension is `md`, matched case-insensitively.
#[must_use]
pub fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Whether a browser should save the file at `path` rather than show it: a
/// [download](download_content_type) other than a pdf, which browsers show
/// in their own viewer, even under `Content-Security-Policy: sandbox`.
#[must_use]
pub fn is_attachment(path: &str) -> bool {
    download_content_type(path).is_some_and(|content_type| content_type != "application/pdf")
}

/// RFC 8187's `attr-char`, which `filename*` leaves unencoded.
const ATTR_CHAR: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'!')
    .remove(b'#')
    .remove(b'$')
    .remove(b'&')
    .remove(b'+')
    .remove(b'-')
    .remove(b'.')
    .remove(b'^')
    .remove(b'_')
    .remove(b'`')
    .remove(b'|')
    .remove(b'~');

/// The `Content-Disposition` that saves an [attachment](is_attachment) under
/// its own name, spelled in ASCII too for browsers that ignore `filename*`.
/// `None` for a file a browser shows.
#[must_use]
pub fn content_disposition(path: &str) -> Option<String> {
    if !is_attachment(path) {
        return None;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let ascii: String = name
        .chars()
        .map(|c| match c {
            ' '..='~' if c != '"' && c != '\\' => c,
            _ => '_',
        })
        .collect();
    Some(format!(
        "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
        utf8_percent_encode(name, ATTR_CHAR)
    ))
}

fn content_type(types: &[(&str, &'static str)], path: &str) -> Option<&'static str> {
    let extension = Path::new(path).extension()?.to_str()?;
    types
        .iter()
        .find(|(allowed, _)| allowed.eq_ignore_ascii_case(extension))
        .map(|&(_, content_type)| content_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_misspelled_file_is_not_found_with_its_spelling_on_disk() {
        let missing = MissingFile::Misspelled {
            on_disk: "docs/img/logo.png".to_owned(),
        };

        assert_eq!(
            format!("image `docs/img/Logo.PNG` {missing}"),
            "image `docs/img/Logo.PNG` not found; the file on disk is `docs/img/logo.png`"
        );
    }

    #[test]
    fn allowed_extensions_map_to_their_content_type() {
        for (path, content_type) in [
            ("a.png", "image/png"),
            ("a.jpg", "image/jpeg"),
            ("a.jpeg", "image/jpeg"),
            ("a.gif", "image/gif"),
            ("a.webp", "image/webp"),
            ("a.avif", "image/avif"),
            ("a.svg", "image/svg+xml"),
            ("docs/img/Flow.PNG", "image/png"),
            ("a.JpEg", "image/jpeg"),
            (".github/assets/logo.svg", "image/svg+xml"),
        ] {
            assert_eq!(file_content_type(path), Some(content_type), "{path}");
            assert_eq!(image_content_type(path), Some(content_type), "{path}");
            assert_eq!(download_content_type(path), None, "{path}");
        }
    }

    #[test]
    fn downloads_are_served_but_are_not_images() {
        for (path, content_type) in [
            ("a.pdf", "application/pdf"),
            (
                "a.docx",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            ),
            (
                "a.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
            (
                "a.pptx",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            ),
            ("data/customers.CSV", "text/csv"),
            ("a.zip", "application/zip"),
        ] {
            assert_eq!(file_content_type(path), Some(content_type), "{path}");
            assert_eq!(download_content_type(path), Some(content_type), "{path}");
            assert_eq!(image_content_type(path), None, "{path}");
        }
    }

    #[test]
    fn other_files_are_refused() {
        for path in [
            "",
            ".env",
            ".png",
            "README.md",
            ".rw/comments/sqlite.db",
            ".git/config",
            "img.png/notes",
            "logo.png.bak",
            "logo",
            "index.html",
            "page.htm",
            "page.xhtml",
            "feed.xml",
            "app.js",
            "notes.txt",
            "openapi.json",
            "config.yaml",
            "config.yml",
            "rw.toml",
            "main.rs",
            "Makefile",
            "macro.docm",
            "macro.xlsm",
            "macro.pptm",
            "spec.pdf.bak",
        ] {
            assert_eq!(file_content_type(path), None, "{path}");
        }
    }

    #[test]
    fn markdown_is_any_case_of_the_md_extension() {
        for path in [
            "a.md",
            "docs/_partials/Note.MD",
            "x.Md",
            ".github/README.md",
        ] {
            assert!(is_markdown(path), "{path}");
        }
        for path in [
            "",
            "md",
            ".md",
            "a.markdown",
            "a.md.bak",
            "a.mdx",
            "notes.txt",
        ] {
            assert!(!is_markdown(path), "{path}");
        }
    }
}
