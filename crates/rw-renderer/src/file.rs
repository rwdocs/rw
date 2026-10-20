//! Placing the file a local image names in the project.
//!
//! The path arithmetic is the renderer's; where the source directory sits in
//! the project, and which files it takes, is the caller's
//! [`FileResolver`](crate::FileResolver).

use std::borrow::Cow;

use percent_encoding::{AsciiSet, percent_decode_str, utf8_percent_encode};

use crate::config::{FileRef, FileRefusal, RenderConfig};
use crate::link::PATH;

/// Browsers read a `\` in a url path as `/`.
const SEPARATORS: [char; 2] = ['/', '\\'];

/// What a served file's path must percent-encode. Its `/`s are left alone:
/// [`resolve`] splits on them, so no segment holds one.
const FILE_PATH: &AsciiSet = &PATH.add(b'\\');

/// Whether `src` has a scheme (`https:`, `data:`) or a host (`//host/…`), so
/// that it names no file of the project.
fn is_external(src: &str) -> bool {
    if src.starts_with(SEPARATORS) && src[1..].starts_with(SEPARATORS) {
        return true;
    }
    src.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// `src` split before its `?query` or `#fragment`, which no file name has.
fn split_query(src: &str) -> (&str, &str) {
    src.split_at(src.find(['?', '#']).unwrap_or(src.len()))
}

/// The project file the image `src` names, or the warning for one the
/// resolver refuses. `None` when `src` names no file, or the renderer can't place it.
pub(crate) fn resolve(cfg: &RenderConfig, src: &str) -> Option<Result<String, String>> {
    let resolver = cfg.file_resolver.as_deref()?;
    if is_external(src) {
        return None;
    }
    let (path, _) = split_query(src);
    let (dir, path, from_root) = if let Some(path) = path.strip_prefix(SEPARATORS) {
        ("", path, true)
    } else {
        let source_path = cfg.source_path.as_deref()?;
        let dir = source_path.rsplit_once('/').map_or("", |(dir, _)| dir);
        (dir, path, false)
    };

    // `dir` is a file path; only the image's path is url text. A segment that
    // doesn't decode to UTF-8 stays as written, like a malformed escape.
    let decoded: Vec<Cow<'_, str>> = path
        .split(SEPARATORS)
        .map(|segment| {
            percent_decode_str(segment)
                .decode_utf8()
                .unwrap_or(Cow::Borrowed(segment))
        })
        .collect();
    if matches!(decoded.last().map(AsRef::as_ref), Some("" | "." | "..")) {
        // A directory, or nothing at all.
        return None;
    }
    if decoded.iter().any(|segment| segment.contains(SEPARATORS)) {
        return Some(Err(format!(
            "image `{src}` has an encoded `/` or `\\` in a file name"
        )));
    }

    let mut ups = 0;
    let mut segments: Vec<Cow<'_, str>> = Vec::new();
    for segment in dir.split('/').map(Cow::Borrowed).chain(decoded) {
        match &*segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    ups += 1;
                }
            }
            _ => segments.push(segment),
        }
    }

    let path = format!("{}{}", "../".repeat(ups), segments.join("/"));
    let file = if from_root {
        FileRef::Project(&path)
    } else {
        FileRef::Source(&path)
    };
    Some(
        resolver
            .project_file(file)
            .map_err(|refusal| match refusal {
                FileRefusal::OutsideProject => format!("image `{src}` is outside the project"),
                FileRefusal::NotAllowed => {
                    format!("image `{src}` is not a supported image type")
                }
            }),
    )
}

/// The url the project file `path` is served at, with the query and fragment
/// of the image `src` that named it. `None` without a files base.
pub(crate) fn url(cfg: &RenderConfig, path: &str, src: &str) -> Option<String> {
    let base = cfg.files_base.as_deref()?;
    let (_, suffix) = split_query(src);
    Some(format!(
        "{base}/{}{suffix}",
        utf8_percent_encode(path, FILE_PATH)
    ))
}

#[cfg(test)]
pub(crate) mod resolvers {
    use std::path::Path;

    use crate::config::{FileRef, FileRefusal, FileResolver};

    /// Echoes every file back, as `source:` or `project:` plus its path.
    pub(crate) struct Echo;

    impl FileResolver for Echo {
        fn project_file(&self, file: FileRef<'_>) -> Result<String, FileRefusal> {
            Ok(match file {
                FileRef::Source(path) => format!("source:{path}"),
                FileRef::Project(path) => format!("project:{path}"),
            })
        }
    }

    /// [`Echo`] for `.png` files that stay inside their base; refuses the rest.
    pub(crate) struct PngsInside;

    impl FileResolver for PngsInside {
        fn project_file(&self, file: FileRef<'_>) -> Result<String, FileRefusal> {
            let (FileRef::Source(path) | FileRef::Project(path)) = file;
            if path.starts_with("../") {
                Err(FileRefusal::OutsideProject)
            } else if Path::new(path).extension().is_none_or(|ext| ext != "png") {
                Err(FileRefusal::NotAllowed)
            } else {
                Echo.project_file(file)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolvers::{Echo, PngsInside};
    use super::*;

    fn cfg(source_path: Option<&str>) -> RenderConfig {
        let mut cfg = RenderConfig::new();
        cfg.source_path = source_path.map(ToOwned::to_owned);
        cfg.file_resolver = Some(Box::new(PngsInside));
        cfg
    }

    fn placed(source_path: &str, src: &str) -> Option<String> {
        let mut cfg = cfg(Some(source_path));
        cfg.file_resolver = Some(Box::new(Echo));
        resolve(&cfg, src).map(Result::unwrap)
    }

    #[test]
    fn relative_paths_resolve_from_the_source_files_directory() {
        for (source, src, file) in [
            ("guide.md", "img/a.png", "source:img/a.png"),
            ("guide/index.md", "img/a.png", "source:guide/img/a.png"),
            ("guide/README.md", "./img/a.png", "source:guide/img/a.png"),
            ("a/b/c.md", "../img/./a.png", "source:a/img/a.png"),
            ("a/b.md", "x/../../img//a.png", "source:img/a.png"),
            ("a/b.md", "img\\a.png", "source:a/img/a.png"),
            ("a/b.md", "x\\..\\a.png", "source:a/a.png"),
            ("a.md", "../a.png", "source:../a.png"),
            ("a/b.md", "../../../a.png", "source:../../a.png"),
            ("../README.md", "docs/img/a.png", "source:../docs/img/a.png"),
            ("../README.md", "../a.png", "source:../../a.png"),
            // A `\` in the page's own directory is part of a file name.
            ("a\\b/i.md", "c.png", "source:a\\b/c.png"),
        ] {
            assert_eq!(
                placed(source, src).as_deref(),
                Some(file),
                "{src} from {source}"
            );
        }
    }

    #[test]
    fn a_leading_slash_means_the_project_root() {
        for (src, file) in [
            ("/logo.png", "project:logo.png"),
            (
                "/.github/assets/logo.png",
                "project:.github/assets/logo.png",
            ),
            ("/a/../logo.png", "project:logo.png"),
            ("\\logo.png", "project:logo.png"),
            ("/../logo.png", "project:../logo.png"),
        ] {
            assert_eq!(placed("a/b/c.md", src).as_deref(), Some(file), "{src}");
        }
    }

    #[test]
    fn query_and_fragment_name_no_part_of_the_file() {
        for (src, file) in [
            ("a.png?v=2", "source:g/a.png"),
            ("a.png#x", "source:g/a.png"),
            ("a.png?v=2#x/../y", "source:g/a.png"),
            ("/a.png?v=2", "project:a.png"),
        ] {
            assert_eq!(placed("g/i.md", src).as_deref(), Some(file), "{src}");
        }
        assert_eq!(split_query("a.png?v=2#x"), ("a.png", "?v=2#x"));
        assert_eq!(split_query("a.png#x?y"), ("a.png", "#x?y"));
    }

    #[test]
    fn percent_encoded_paths_name_the_decoded_file() {
        for (src, file) in [
            ("a%20b.png", "source:a b.png"),
            ("%D0%B3%D0%B0%D0%B9%D0%B4.png", "source:гайд.png"),
            ("гайд.png", "source:гайд.png"),
            ("100%25.png", "source:100%.png"),
            ("x/%2E%2E/a.png", "source:a.png"),
            ("%2e/a.png", "source:a.png"),
            // Malformed escapes, and escapes that aren't UTF-8, stay as written.
            ("100%.png", "source:100%.png"),
            ("a%zz.png", "source:a%zz.png"),
            ("a%2.png", "source:a%2.png"),
            ("a%FF.png", "source:a%FF.png"),
        ] {
            assert_eq!(placed("i.md", src).as_deref(), Some(file), "{src}");
        }
    }

    #[test]
    fn an_encoded_slash_makes_no_segment() {
        for src in ["a%2Fb.png", "x/..%2F..%2F.env", "/a%2fb.png", "a%5Cb.png"] {
            assert_eq!(
                resolve(&cfg(Some("i.md")), src),
                Some(Err(format!(
                    "image `{src}` has an encoded `/` or `\\` in a file name"
                ))),
            );
        }
    }

    #[test]
    fn external_and_fileless_sources_are_left_alone() {
        for src in [
            "https://example.com/a.png",
            "HTTP://example.com/a.png",
            "//example.com/a.png",
            "/\\example.com/a.png",
            "\\\\example.com/a.png",
            "data:image/png;base64,iVBORw0KGgo=",
            "ftp://example.com/a.png",
            "",
            "#x",
            "?v=2",
            ".",
            "a/..",
            "a/%2E%2E",
            "img/",
            "/",
        ] {
            assert_eq!(resolve(&cfg(Some("g/i.md")), src), None, "{src:?}");
        }
        // A colon after a slash starts no scheme.
        assert_eq!(
            placed("i.md", "img/a:b.png").as_deref(),
            Some("source:img/a:b.png")
        );
    }

    #[test]
    fn refusals_warn() {
        let cfg = cfg(Some("i.md"));
        assert_eq!(
            resolve(&cfg, "../a.png"),
            Some(Err("image `../a.png` is outside the project".to_owned()))
        );
        assert_eq!(
            resolve(&cfg, "/a.bmp?x"),
            Some(Err(
                "image `/a.bmp?x` is not a supported image type".to_owned()
            ))
        );
    }

    #[test]
    fn served_urls_encode_the_path_and_keep_the_query_and_fragment() {
        let mut cfg = RenderConfig::new();
        assert_eq!(url(&cfg, "a.png", "a.png"), None);
        cfg.files_base = Some("/_api/files".to_owned());
        for (path, src, served) in [
            ("docs/a.png", "a.png", "/_api/files/docs/a.png"),
            (
                ".github/l.svg",
                "/.github/l.svg",
                "/_api/files/.github/l.svg",
            ),
            ("a b.png", "a%20b.png", "/_api/files/a%20b.png"),
            ("100%.png", "100%25.png", "/_api/files/100%25.png"),
            ("a#b.png", "a%23b.png", "/_api/files/a%23b.png"),
            ("a?b.png", "a%3Fb.png", "/_api/files/a%3Fb.png"),
            ("a\\b/c.png", "c.png", "/_api/files/a%5Cb/c.png"),
            (
                "гайд.png",
                "гайд.png",
                "/_api/files/%D0%B3%D0%B0%D0%B9%D0%B4.png",
            ),
            ("a.png", "a.png?v=2#x", "/_api/files/a.png?v=2#x"),
            ("a.png", "a.png#x?y", "/_api/files/a.png#x?y"),
        ] {
            assert_eq!(url(&cfg, path, src).as_deref(), Some(served), "{path}");
        }
    }

    #[test]
    fn resolving_needs_a_resolver_and_relative_paths_a_source_path() {
        assert_eq!(resolve(&cfg(None), "a.png"), None);
        assert_eq!(
            resolve(&cfg(None), "/a.png"),
            Some(Ok("project:a.png".to_owned()))
        );
        let mut cfg = cfg(Some("i.md"));
        cfg.file_resolver = None;
        assert_eq!(resolve(&cfg, "a.png"), None);
        assert_eq!(resolve(&cfg, "/a.png"), None);
    }
}
