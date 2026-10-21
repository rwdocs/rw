//! Placing the file a local image or link names in the project.
//!
//! The path arithmetic is the renderer's; which files the project takes is
//! the caller's to say through a [`FileResolver`], such as [`ProjectFiles`].

use std::borrow::Cow;
use std::path::Path;

use percent_encoding::{percent_decode_str, utf8_percent_encode};

use crate::config::{FileRefusal, FileResolver, ImageSrc, RenderConfig};
use crate::link::PATH;

/// Browsers read a `\` in a url path as `/`.
pub(crate) const SEPARATORS: [char; 2] = ['/', '\\'];

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

/// The extension of the file name `src` ends with, as written.
pub(crate) fn file_extension(src: &str) -> Option<&str> {
    let (path, _) = split_query(src);
    let name = path.rsplit(SEPARATORS).next().unwrap_or(path);
    Path::new(name)
        .extension()?
        .to_str()
        .filter(|extension| !extension.is_empty())
}

/// The url `path` as a file path from the directory `dir`, [folded](fold).
/// Each segment is percent-decoded once (`%2E%2E` is `..`) unless that isn't
/// UTF-8. `None` when one decodes to hold a separator or NUL.
pub(crate) fn file_path(dir: &str, path: &str) -> Option<String> {
    let decoded = path
        .split(SEPARATORS)
        .map(|segment| {
            let decoded = percent_decode_str(segment)
                .decode_utf8()
                .unwrap_or(Cow::Borrowed(segment));
            (!decoded.contains(['/', '\\', '\0'])).then_some(decoded)
        })
        .collect::<Option<Vec<_>>>()?;
    Some(fold(dir, decoded))
}

/// `segments` after the directory `dir`, folded, with a `../` per step above
/// `dir`'s root.
pub(crate) fn fold<'a>(dir: &'a str, segments: impl IntoIterator<Item = Cow<'a, str>>) -> String {
    let mut ups = 0;
    let mut folded = Vec::new();
    for segment in dir.split('/').map(Cow::Borrowed).chain(segments) {
        match &*segment {
            "" | "." => {}
            ".." => ups += usize::from(folded.pop().is_none()),
            _ => folded.push(segment),
        }
    }
    format!("{}{}", "../".repeat(ups), folded.join("/"))
}

/// [`image_file`] with the renderer's resolver.
pub(crate) fn resolve(
    cfg: &RenderConfig,
    source_path: Option<&str>,
    src: &str,
) -> Option<Result<String, String>> {
    image_file(cfg.file_resolver.as_deref()?, source_path, src)
}

/// Why a `src` names no file of the project.
#[derive(Debug)]
pub(crate) enum Unplaced {
    Refused(FileRefusal),
    /// A segment decodes to hold a `/`, `\` or NUL.
    EncodedSeparator,
}

impl Unplaced {
    /// The warning for the image or link (`noun`) `src`.
    pub(crate) fn warning(&self, noun: &str, src: &str) -> String {
        match self {
            Self::EncodedSeparator => {
                format!("{noun} `{src}` has an encoded `/`, `\\` or NUL in a file name")
            }
            Self::Refused(FileRefusal::OutsideProject) => {
                format!("{noun} `{src}` is outside the project")
            }
            Self::Refused(FileRefusal::NotAllowed) => {
                format!("{noun} `{src}` is not a supported image type")
            }
        }
    }
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
    Some(place(resolver, source_path, src)?.map_err(|unplaced| unplaced.warning("image", src)))
}

/// [`image_file`], keeping why a file is refused.
pub(crate) fn place(
    resolver: &dyn FileResolver,
    source_path: Option<&str>,
    src: &str,
) -> Option<Result<String, Unplaced>> {
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

    let name = path.rsplit(SEPARATORS).next().unwrap_or(path);
    if matches!(
        &*percent_decode_str(name).decode_utf8_lossy(),
        "" | "." | ".."
    ) {
        // A directory, or nothing at all.
        return None;
    }
    let Some(path) = file_path(dir, path) else {
        return Some(Err(Unplaced::EncodedSeparator));
    };
    Some(resolver.project_file(&path).map_err(Unplaced::Refused))
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
        if path.starts_with("../") {
            return Err(FileRefusal::OutsideProject);
        }
        if !(self.allowed)(path) {
            return Err(FileRefusal::NotAllowed);
        }
        Ok(path.to_owned())
    }
}

/// Serves each project file under a url prefix, keeping the query and
/// fragment of the image `src` or link that named it.
#[derive(Clone)]
pub(crate) struct FilesBase(pub(crate) String);

impl FilesBase {
    pub(crate) fn url(&self, path: &str, src: &str) -> String {
        let (_, suffix) = split_query(src);
        format!("{}/{}{suffix}", self.0, utf8_percent_encode(path, PATH))
    }
}

impl ImageSrc for FilesBase {
    fn image_src(&self, path: &str, src: &str) -> Option<String> {
        Some(self.url(path, src))
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

    fn cfg() -> RenderConfig {
        RenderConfig {
            file_resolver: Some(Box::new(PngsInside)),
            ..RenderConfig::default()
        }
    }

    fn placed(source_path: &str, src: &str) -> Option<String> {
        let mut cfg = cfg();
        cfg.file_resolver = Some(Box::new(Echo));
        resolve(&cfg, Some(source_path), src).map(Result::unwrap)
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
        for src in [
            "a%2Fb.png",
            "x/..%2F..%2F.env",
            "/a%2fb.png",
            "a%5Cb.png",
            "a%00b.png",
        ] {
            assert_eq!(
                resolve(&cfg(), Some("i.md"), src),
                Some(Err(format!(
                    "image `{src}` has an encoded `/`, `\\` or NUL in a file name"
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
            assert_eq!(resolve(&cfg(), Some("g/i.md"), src), None, "{src:?}");
        }
        // A colon after a slash starts no scheme.
        assert_eq!(
            placed("i.md", "img/a:b.png").as_deref(),
            Some("img/a:b.png")
        );
    }

    #[test]
    fn refusals_warn() {
        let cfg = cfg();
        assert_eq!(
            resolve(&cfg, Some("i.md"), "../a.png"),
            Some(Err("image `../a.png` is outside the project".to_owned()))
        );
        assert_eq!(
            resolve(&cfg, Some("i.md"), "/a.bmp?x"),
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
        assert_eq!(resolve(&cfg(), None, "a.png"), None);
        assert_eq!(
            resolve(&cfg(), None, "/a.png"),
            Some(Ok("a.png".to_owned()))
        );
        let mut cfg = cfg();
        cfg.file_resolver = None;
        assert_eq!(resolve(&cfg, Some("i.md"), "a.png"), None);
        assert_eq!(resolve(&cfg, Some("i.md"), "/a.png"), None);
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
}
