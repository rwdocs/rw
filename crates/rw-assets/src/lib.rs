//! Frontend asset serving for the RW documentation engine.
//!
//! Provides a single API for accessing frontend assets in both embedded and
//! filesystem modes:
//!
//! - **`embed` feature on**: Assets are compiled into the binary via `rust-embed`
//! - **`embed` feature off**: Assets are read from `packages/viewer/dist/` at runtime

use std::borrow::Cow;
#[cfg(not(feature = "embed"))]
use std::path::{Component, Path};

/// Embedded frontend assets (only available with `embed` feature).
#[cfg(feature = "embed")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../packages/viewer/dist"]
#[prefix = ""]
struct Assets;

/// Directory for filesystem-based asset serving (dev mode).
///
/// Uses `CARGO_MANIFEST_DIR` so the binary finds assets regardless of the
/// working directory it is launched from.
#[cfg(not(feature = "embed"))]
const DEV_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/viewer/dist");

/// Get a frontend asset by path (relative to `packages/viewer/dist/`).
///
/// Returns the file contents if the asset exists, `None` otherwise.
#[cfg(feature = "embed")]
pub fn get(path: &str) -> Option<Cow<'static, [u8]>> {
    Assets::get(path).map(|f| f.data)
}

/// Get a frontend asset by path (relative to `packages/viewer/dist/`).
///
/// Returns the file contents if the asset exists, `None` otherwise.
#[cfg(not(feature = "embed"))]
pub fn get(path: &str) -> Option<Cow<'static, [u8]>> {
    read_within(Path::new(DEV_DIR), path)
}

/// `path` is the raw request path, so a `..` or root component would escape
/// `root`.
#[cfg(not(feature = "embed"))]
fn read_within(root: &Path, path: &str) -> Option<Cow<'static, [u8]>> {
    let relative = Path::new(path);
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return None;
    }
    std::fs::read(root.join(relative)).ok().map(Cow::Owned)
}

/// Return the MIME type string for the given file path.
pub fn mime_for(path: &str) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
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

    #[cfg(not(feature = "embed"))]
    mod read_within {
        use super::*;

        use std::fs;
        use std::path::PathBuf;

        use tempfile::TempDir;

        /// `<tmp>/secret` sits beside the `<tmp>/dist` root.
        fn fixture() -> (TempDir, PathBuf) {
            let tmp = TempDir::new().unwrap();
            let root = tmp.path().join("dist");
            fs::create_dir_all(root.join("assets")).unwrap();
            fs::write(root.join("index.html"), "index").unwrap();
            fs::write(root.join("assets/app.js"), "app").unwrap();
            fs::write(tmp.path().join("secret"), "secret").unwrap();
            (tmp, root)
        }

        #[test]
        fn reads_nested_asset() {
            let (_tmp, root) = fixture();
            assert_eq!(
                read_within(&root, "assets/app.js").as_deref(),
                Some(&b"app"[..])
            );
        }

        #[test]
        fn rejects_parent_dir() {
            let (_tmp, root) = fixture();
            for path in ["../secret", "assets/../../secret", "assets/../index.html"] {
                assert!(read_within(&root, path).is_none(), "for {path}");
            }
        }

        #[test]
        fn rejects_absolute_path() {
            let (tmp, root) = fixture();
            let secret = tmp.path().join("secret");
            assert!(read_within(&root, secret.to_str().unwrap()).is_none());
        }

        #[test]
        fn rejects_current_dir() {
            let (_tmp, root) = fixture();
            assert!(read_within(&root, "./index.html").is_none());
        }
    }

    #[test]
    fn test_get_nonexistent_asset() {
        assert!(get("nonexistent_file_that_does_not_exist.txt").is_none());
    }

    /// The viewer build emits its third-party notice into the directory this
    /// crate embeds, so the notice travels into the binary with the assets.
    ///
    /// Skipped when the frontend has not been built — `cargo test` must not
    /// require an npm build to pass. CI builds the bundle first, so the
    /// assertion is enforced there.
    #[test]
    fn third_party_notice_ships_with_the_viewer_bundle() {
        if get("index.html").is_none() {
            return;
        }

        let notice = get("THIRD-PARTY-JS.md").expect(
            "viewer bundle is built but carries no THIRD-PARTY-JS.md; \
             did rollup-plugin-license stop running?",
        );
        let text = String::from_utf8_lossy(&notice);

        assert!(
            text.contains("# Third-Party Licenses (JavaScript)"),
            "THIRD-PARTY-JS.md exists but does not look like a real notice \
             (missing its heading); got: {text:?}"
        );
    }
}
