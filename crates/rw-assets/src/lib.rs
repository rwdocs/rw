//! Frontend asset serving for the RW documentation engine.
//!
//! Provides a single API for accessing frontend assets in both embedded and
//! filesystem modes:
//!
//! - **`embed` feature on**: Assets are compiled into the binary via `rust-embed`
//! - **`embed` feature off**: Assets are read from `frontend/dist/` at runtime

use std::borrow::Cow;
use std::path::Path;

/// Embedded frontend assets (only available with `embed` feature).
#[cfg(feature = "embed")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../frontend/dist"]
#[prefix = ""]
struct Assets;

/// Directory for filesystem-based asset serving (dev mode).
#[cfg(not(feature = "embed"))]
const DEV_DIR: &str = "frontend/dist";

/// Get a frontend asset by path (relative to `frontend/dist/`).
///
/// Returns the file contents if the asset exists, `None` otherwise.
#[cfg(feature = "embed")]
pub fn get(path: &str) -> Option<Cow<'static, [u8]>> {
    Assets::get(path).map(|f| f.data)
}

/// Get a frontend asset by path (relative to `frontend/dist/`).
///
/// Returns the file contents if the asset exists, `None` otherwise.
#[cfg(not(feature = "embed"))]
pub fn get(path: &str) -> Option<Cow<'static, [u8]>> {
    let full_path = Path::new(DEV_DIR).join(path);
    std::fs::read(&full_path).ok().map(Cow::Owned)
}

/// Iterate all available asset paths.
#[cfg(feature = "embed")]
pub fn iter() -> impl Iterator<Item = Cow<'static, str>> {
    Assets::iter()
}

/// Iterate all available asset paths.
#[cfg(not(feature = "embed"))]
pub fn iter() -> impl Iterator<Item = Cow<'static, str>> {
    walk_dir(Path::new(DEV_DIR)).into_iter().map(Cow::Owned)
}

/// Return the MIME type string for the given file path.
pub fn mime_for(path: &str) -> &'static str {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // Leak the string so we get a `&'static str` — there are only a bounded
    // number of MIME types so this doesn't grow unboundedly in practice.
    Box::leak(mime.to_string().into_boxed_str())
}

/// Recursively walk a directory and return paths relative to `base`.
#[cfg(not(feature = "embed"))]
fn walk_dir(base: &Path) -> Vec<String> {
    let mut result = Vec::new();
    walk_dir_inner(base, base, &mut result);
    result
}

#[cfg(not(feature = "embed"))]
fn walk_dir_inner(base: &Path, dir: &Path, result: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir_inner(base, &path, result);
        } else if let Ok(rel) = path.strip_prefix(base) {
            // Normalize to forward slashes
            result.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_for_known_types() {
        assert_eq!(mime_for("style.css"), "text/css");
        assert_eq!(mime_for("app.js"), "text/javascript");
        assert_eq!(mime_for("index.html"), "text/html");
        assert_eq!(mime_for("image.png"), "image/png");
    }

    #[test]
    fn test_mime_for_unknown_type() {
        assert_eq!(mime_for("file.unknown_ext_xyz"), "application/octet-stream");
    }

    #[test]
    fn test_get_nonexistent_asset() {
        assert!(get("nonexistent_file_that_does_not_exist.txt").is_none());
    }
}
