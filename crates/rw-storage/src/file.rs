//! The project files [`Storage::read_file`](crate::Storage::read_file) may
//! serve.

use std::path::Path;

/// The allowlist itself: the project root also holds `.env`, `.git/` and
/// `.rw/comments/sqlite.db`, and no extension here matches them.
const CONTENT_TYPES: [(&str, &str); 7] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("svg", "image/svg+xml"),
];

/// The content type of a project file, from its extension (matched
/// case-insensitively), or `None` when storage must refuse to serve it.
#[must_use]
pub fn file_content_type(path: &str) -> Option<&'static str> {
    let extension = Path::new(path).extension()?.to_str()?;
    CONTENT_TYPES
        .iter()
        .find(|(allowed, _)| allowed.eq_ignore_ascii_case(extension))
        .map(|&(_, content_type)| content_type)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        ] {
            assert_eq!(file_content_type(path), None, "{path}");
        }
    }
}
