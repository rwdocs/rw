//! Placing the file a local image names in the project.
//!
//! The path arithmetic is the renderer's; which files the project takes is
//! the caller's to say through a [`FileResolver`], such as [`ProjectFiles`].

use std::borrow::Cow;

use percent_encoding::{percent_decode_str, utf8_percent_encode};

use crate::config::{FileRefusal, FileResolver, ImageSrc, RenderConfig};
use crate::link::{PATH, fold};

/// Browsers read a `\` in a url path as `/`.
const SEPARATORS: [char; 2] = ['/', '\\'];

/// Whether `src` has a scheme (`https:`, `data:`) or a host (`//host/…`), so
/// that it names no file of the project.
///
/// # Examples
///
/// ```
/// use rw_renderer::is_external;
///
/// assert!(is_external("https://example.com/a.png"));
/// assert!(is_external("//example.com/a.png"));
/// assert!(is_external("ftp://example.com/a.png"));
/// assert!(!is_external("img/a.png"));
/// assert!(!is_external("/logo.png"));
/// ```
#[must_use]
pub fn is_external(src: &str) -> bool {
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
pub(crate) fn split_query(src: &str) -> (&str, &str) {
    src.split_at(src.find(['?', '#']).unwrap_or(src.len()))
}

/// [`image_file`] with the renderer's resolver and source path.
pub(crate) fn resolve(cfg: &RenderConfig, src: &str) -> Option<Result<String, String>> {
    image_file(
        cfg.file_resolver.as_deref()?,
        cfg.source_path.as_deref(),
        src,
    )
}

/// The project file the image `src` on the page at `source_path` names, as
/// `resolver` places it, or the warning for one it refuses. The renderer
/// resolves every image with this.
///
/// `source_path` is relative to the project root, as
/// [`MarkdownRenderer::with_source_path`](crate::MarkdownRenderer::with_source_path)
/// takes it. `None` when `src` names no file, or is relative and there is no
/// `source_path`.
///
/// # Examples
///
/// ```
/// use rw_renderer::{ProjectFiles, image_file};
///
/// let files = ProjectFiles::new(|_| true);
///
/// assert_eq!(
///     image_file(&files, Some("docs/guide/intro.md"), "my%20flow.puml?v=2"),
///     Some(Ok("docs/guide/my flow.puml".to_owned()))
/// );
/// assert_eq!(image_file(&files, None, "/flow.puml"), Some(Ok("flow.puml".to_owned())));
/// assert_eq!(image_file(&files, None, "https://example.com/a.png"), None);
/// ```
#[must_use]
pub fn image_file(
    resolver: &dyn FileResolver,
    source_path: Option<&str>,
    src: &str,
) -> Option<Result<String, String>> {
    if is_external(src) {
        return None;
    }
    let (path, _) = split_query(src);
    let (dir, path) = if let Some(path) = path.strip_prefix(SEPARATORS) {
        ("", path)
    } else {
        let source_path = source_path?;
        let dir = source_path.rsplit_once('/').map_or("", |(dir, _)| dir);
        (dir, path)
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

    let (ups, segments) = fold(dir.split('/').map(Cow::Borrowed).chain(decoded));
    let path = format!("{}{}", "../".repeat(ups), segments.join("/"));
    Some(
        resolver
            .project_file(&path)
            .map_err(|refusal| match refusal {
                FileRefusal::OutsideProject => format!("image `{src}` is outside the project"),
                FileRefusal::NotAllowed => {
                    format!("image `{src}` is not a supported image type")
                }
            }),
    )
}

/// `path`, `/`-separated and relative to the project root, folded, or `None`
/// when it is absolute or leaves the project.
#[must_use]
pub fn project_path(path: &str) -> Option<String> {
    if path.starts_with('/') {
        return None;
    }
    let (0, segments) = fold(path.split('/')) else {
        return None;
    };
    Some(segments.join("/"))
}

/// A [`FileResolver`] for the files of a project that `allowed` accepts.
///
/// # Examples
///
/// ```
/// use rw_renderer::{FileRefusal, FileResolver, ProjectFiles};
///
/// let files = ProjectFiles::new(|path| path.ends_with(".png"));
///
/// assert_eq!(files.project_file("docs/img/a.png").as_deref(), Ok("docs/img/a.png"));
/// assert_eq!(files.project_file("../a.png"), Err(FileRefusal::OutsideProject));
/// assert_eq!(files.project_file(".env"), Err(FileRefusal::NotAllowed));
/// ```
#[derive(Debug, Clone)]
pub struct ProjectFiles {
    allowed: fn(&str) -> bool,
}

impl ProjectFiles {
    /// `allowed` gets each file's path as
    /// [`project_file`](FileResolver::project_file) would return it: folded
    /// and relative to the project root, never the image's `src`.
    #[must_use]
    pub fn new(allowed: fn(&str) -> bool) -> Self {
        Self { allowed }
    }
}

impl FileResolver for ProjectFiles {
    fn project_file(&self, path: &str) -> Result<String, FileRefusal> {
        let path = project_path(path).ok_or(FileRefusal::OutsideProject)?;
        if !(self.allowed)(&path) {
            return Err(FileRefusal::NotAllowed);
        }
        Ok(path)
    }
}

/// Serves each project file under a url prefix, keeping the query and
/// fragment of the image `src` that named it.
pub(crate) struct FilesBase(pub(crate) String);

impl ImageSrc for FilesBase {
    fn image_src(&self, path: &str, src: &str) -> Option<String> {
        let (_, suffix) = split_query(src);
        Some(format!(
            "{}/{}{suffix}",
            self.0,
            utf8_percent_encode(path, PATH)
        ))
    }
}

#[cfg(test)]
pub(crate) mod resolvers {
    use std::path::Path;

    use crate::config::{FileRefusal, FileResolver};

    /// Echoes every file's path back.
    pub(crate) struct Echo;

    impl FileResolver for Echo {
        fn project_file(&self, path: &str) -> Result<String, FileRefusal> {
            Ok(path.to_owned())
        }
    }

    /// [`Echo`] for `.png` files inside the project; refuses the rest.
    pub(crate) struct PngsInside;

    impl FileResolver for PngsInside {
        fn project_file(&self, path: &str) -> Result<String, FileRefusal> {
            if path.starts_with("../") {
                Err(FileRefusal::OutsideProject)
            } else if Path::new(path).extension().is_none_or(|ext| ext != "png") {
                Err(FileRefusal::NotAllowed)
            } else {
                Echo.project_file(path)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::resolvers::{Echo, PngsInside};
    use super::*;

    fn cfg(source_path: Option<&str>) -> RenderConfig {
        RenderConfig {
            source_path: source_path.map(ToOwned::to_owned),
            file_resolver: Some(Box::new(PngsInside)),
            ..RenderConfig::default()
        }
    }

    fn placed(source_path: &str, src: &str) -> Option<String> {
        let mut cfg = cfg(Some(source_path));
        cfg.file_resolver = Some(Box::new(Echo));
        resolve(&cfg, src).map(Result::unwrap)
    }

    #[test]
    fn relative_paths_resolve_from_the_source_files_directory() {
        for (source, src, file) in [
            ("guide.md", "img/a.png", "img/a.png"),
            ("guide/index.md", "img/a.png", "guide/img/a.png"),
            ("guide/README.md", "./img/a.png", "guide/img/a.png"),
            ("a/b/c.md", "../img/./a.png", "a/img/a.png"),
            ("a/b.md", "x/../../img//a.png", "img/a.png"),
            ("a/b.md", "img\\a.png", "a/img/a.png"),
            ("a/b.md", "x\\..\\a.png", "a/a.png"),
            ("a.md", "../a.png", "../a.png"),
            ("a/b.md", "../../../a.png", "../../a.png"),
            ("../README.md", "docs/img/a.png", "../docs/img/a.png"),
            ("../README.md", "../a.png", "../../a.png"),
            // A `\` in the page's own directory is part of a file name.
            ("a\\b/i.md", "c.png", "a\\b/c.png"),
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
            ("/logo.png", "logo.png"),
            ("/.github/assets/logo.png", ".github/assets/logo.png"),
            ("/a/../logo.png", "logo.png"),
            ("\\logo.png", "logo.png"),
            ("/../logo.png", "../logo.png"),
        ] {
            assert_eq!(placed("a/b/c.md", src).as_deref(), Some(file), "{src}");
        }
    }

    #[test]
    fn query_and_fragment_name_no_part_of_the_file() {
        for (src, file) in [
            ("a.png?v=2", "g/a.png"),
            ("a.png#x", "g/a.png"),
            ("a.png?v=2#x/../y", "g/a.png"),
            ("/a.png?v=2", "a.png"),
        ] {
            assert_eq!(placed("g/i.md", src).as_deref(), Some(file), "{src}");
        }
        assert_eq!(split_query("a.png?v=2#x"), ("a.png", "?v=2#x"));
        assert_eq!(split_query("a.png#x?y"), ("a.png", "#x?y"));
    }

    #[test]
    fn percent_encoded_paths_name_the_decoded_file() {
        for (src, file) in [
            ("a%20b.png", "a b.png"),
            ("%D0%B3%D0%B0%D0%B9%D0%B4.png", "гайд.png"),
            ("гайд.png", "гайд.png"),
            ("100%25.png", "100%.png"),
            ("x/%2E%2E/a.png", "a.png"),
            ("%2e/a.png", "a.png"),
            // Malformed escapes, and escapes that aren't UTF-8, stay as written.
            ("100%.png", "100%.png"),
            ("a%zz.png", "a%zz.png"),
            ("a%2.png", "a%2.png"),
            ("a%FF.png", "a%FF.png"),
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
            Some("img/a:b.png")
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
        let base = FilesBase("/_api/files".to_owned());
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
            assert_eq!(base.image_src(path, src).as_deref(), Some(served), "{path}");
        }
    }

    #[test]
    fn resolving_needs_a_resolver_and_relative_paths_a_source_path() {
        assert_eq!(resolve(&cfg(None), "a.png"), None);
        assert_eq!(resolve(&cfg(None), "/a.png"), Some(Ok("a.png".to_owned())));
        let mut cfg = cfg(Some("i.md"));
        cfg.file_resolver = None;
        assert_eq!(resolve(&cfg, "a.png"), None);
        assert_eq!(resolve(&cfg, "/a.png"), None);
    }

    fn image(path: &str) -> bool {
        Path::new(path)
            .extension()
            .is_some_and(|ext| ext == "png" || ext == "svg")
    }

    #[test]
    fn project_files_place_files_in_the_project() {
        let files = ProjectFiles::new(image);
        for (file, placed) in [
            ("docs/img/a.png", Ok("docs/img/a.png")),
            (".github/a.svg", Ok(".github/a.svg")),
            ("../a.png", Err(FileRefusal::OutsideProject)),
            ("a.bmp", Err(FileRefusal::NotAllowed)),
            (".env", Err(FileRefusal::NotAllowed)),
            ("../.env", Err(FileRefusal::OutsideProject)),
        ] {
            assert_eq!(
                files.project_file(file),
                placed.map(ToOwned::to_owned),
                "{file}"
            );
        }
    }

    #[test]
    fn project_paths_are_folded_and_stay_in_the_project() {
        for (path, kept) in [
            ("", Some("")),
            ("site/docs", Some("site/docs")),
            ("a/../docs/", Some("docs")),
            ("..", None),
            ("docs/../..", None),
            ("/abs/docs", None),
        ] {
            assert_eq!(project_path(path).as_deref(), kept, "{path}");
        }
    }
}
