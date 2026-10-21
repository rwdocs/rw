//! The url↔file mapping for a source tree.
//!
//! Both directions live here so they cannot disagree: file→url classification
//! ([`classify_relpath`]) for the scanner and the watch path, and url→file
//! resolution ([`PathResolver`]) for every place that reads a page's sources.

use glob::Pattern;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf, absolute};

/// Fallback name the root page is titled from when nothing else titles it.
pub(crate) const HOMEPAGE_FALLBACK_NAME: &str = "home";

/// File names that serve their directory's page, in precedence order.
const DIR_PAGE_FILENAMES: [&str; 2] = ["index.md", README_FILENAME];

/// A directory's page when it has no `index.md`; also the homepage fallback in
/// the project directory.
const README_FILENAME: &str = "README.md";

/// A directory's metadata file.
pub(crate) const META_FILENAME: &str = "meta.yaml";

/// Suffix of a `<name>.meta.yaml` metadata file.
const META_SUFFIX: &str = ".meta.yaml";

/// Variant of [`META_FILENAME`], accepted with a warning.
pub(crate) const INDEX_META_FILENAME: &str = "index.meta.yaml";

/// The classification decision for a single relative path.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Classification {
    /// Content file (`.md`).
    Content { url_path: String },
    /// Metadata file, in any of its forms.
    Metadata { url_path: String },
}

impl Classification {
    /// Consume into the url path this file contributes to.
    pub(crate) fn into_url_path(self) -> String {
        match self {
            Self::Content { url_path } | Self::Metadata { url_path } => url_path,
        }
    }
}

/// Whether `rel_path`, relative to the source directory, is left out of the
/// site: it is [hidden](is_hidden), or a component starting with `_` marks it
/// as meant to be included in pages rather than be one.
pub(crate) fn is_excluded(rel_path: &Path) -> bool {
    has_component_starting_with(rel_path, &['.', '_'])
}

/// Whether some component of `rel_path` starts with `.`.
pub(crate) fn is_hidden(rel_path: &Path) -> bool {
    has_component_starting_with(rel_path, &['.'])
}

fn has_component_starting_with(rel_path: &Path, prefixes: &[char]) -> bool {
    rel_path.components().any(|c| {
        matches!(c, Component::Normal(name)
            if name.to_string_lossy().starts_with(prefixes))
    })
}

/// Classify a relative path into content/metadata + url path.
///
/// Precedence (must stay in sync — that's why it lives in one place):
/// 1. [`is_excluded`] → `None`
/// 2. `*.md` → content (url path via `file_path_to_url`)
/// 3. exact `meta.yaml` → canonical directory metadata (parent url path)
/// 4. `index.meta.yaml` → directory metadata (parent url path) + warning
/// 5. `<name>.meta.yaml` → sibling metadata
/// 6. otherwise → `None`
pub(crate) fn classify_relpath(rel_path: &Path, filename: &str) -> Option<Classification> {
    if is_excluded(rel_path) {
        return None;
    }

    if Path::new(filename)
        .extension()
        .is_some_and(|ext| ext == "md")
    {
        return Some(Classification::Content {
            url_path: file_path_to_url(rel_path),
        });
    }

    if filename == META_FILENAME {
        return Some(Classification::Metadata {
            url_path: parent_url_path(rel_path),
        });
    }

    if let Some(prefix) = filename.strip_suffix(META_SUFFIX) {
        if prefix == "index" {
            return Some(Classification::Metadata {
                url_path: parent_url_path(rel_path),
            });
        }
        return Some(Classification::Metadata {
            url_path: named_meta_url(rel_path),
        });
    }

    None
}

/// Every file whose change can affect a document: markdown, plus both
/// metadata forms.
pub(crate) fn watch_patterns() -> Vec<Pattern> {
    ["**/*.md", "**/meta.yaml", "**/*.meta.yaml"]
        .into_iter()
        .map(|pattern| Pattern::new(pattern).expect("invalid glob pattern"))
        .collect()
}

/// Compute the sibling-file url path for a `<name>.meta.yaml` file.
///
/// Strips the full `.meta.yaml` suffix from the file name (NOT via
/// `file_stem`, which would leave `payments.meta`) and re-attaches the parent
/// directory — mirroring the standalone-`.md` arm of `file_path_to_url`.
fn named_meta_url(rel_path: &Path) -> String {
    let filename = rel_path
        .file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_default();
    let stem = filename.strip_suffix(META_SUFFIX).unwrap_or(&filename);

    match rel_path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => stem.to_owned(),
        Some(parent) => format!("{}/{}", dir_url_path(parent), stem),
        None => stem.to_owned(),
    }
}

/// Convert a relative file path to a URL path.
///
/// Handles `.md` extension stripping and directory page files (`index.md`,
/// `README.md`).
///
/// # Examples
///
/// - `index.md` -> `""`
/// - `guide.md` -> `"guide"`
/// - `domain/index.md` -> `"domain"`
/// - `domain/README.md` -> `"domain"`
/// - `domain/setup.md` -> `"domain/setup"`
fn file_path_to_url(rel_path: &Path) -> String {
    if is_dir_page(rel_path) {
        parent_url_path(rel_path)
    } else {
        // standalone.md -> parent/stem
        let stem = rel_path.file_stem().map(|s| s.to_string_lossy());
        let Some(stem) = stem else {
            return String::new();
        };

        match rel_path.parent() {
            Some(parent) if parent.as_os_str().is_empty() => stem.into_owned(),
            Some(parent) => format!("{}/{}", dir_url_path(parent), stem),
            None => stem.into_owned(),
        }
    }
}

/// Whether `path` names a directory page file rather than a leaf page.
pub(crate) fn is_dir_page(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| DIR_PAGE_FILENAMES.iter().any(|dir_page| name == *dir_page))
}

/// Whether `path` exists on disk, a directory page only under its exact name.
///
/// A case-insensitive filesystem (APFS, NTFS) also finds `readme.md` when asked
/// for `README.md`, which would then serve a page the scan never gave it.
pub(crate) fn exists_on_disk(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    if !is_dir_page(path) {
        return true;
    }
    path.parent()
        .and_then(|dir| fs::read_dir(dir).ok())
        .is_some_and(|mut entries| {
            entries.any(|entry| {
                entry.is_ok_and(|entry| Some(entry.file_name().as_os_str()) == path.file_name())
            })
        })
}

/// The name `Meta::resolve` falls back to when titling `url_path`.
///
/// A standalone page's own file name. A directory page — `index.md`,
/// `README.md`, or no content file at all — is named after the url's last
/// segment, or `"home"` for the root.
pub(crate) fn content_fallback_name(url_path: &str, resolved: Option<&Path>) -> String {
    if let Some(path) = resolved
        && !is_dir_page(path)
        && let Some(name) = path.file_name()
    {
        return name.to_string_lossy().to_lowercase();
    }

    let slug = url_path.rsplit('/').next().unwrap_or(url_path);
    if slug.is_empty() {
        HOMEPAGE_FALLBACK_NAME.to_owned()
    } else {
        slug.to_owned()
    }
}

/// Get URL path from parent directory of a relative path.
fn parent_url_path(rel_path: &Path) -> String {
    rel_path.parent().map(dir_url_path).unwrap_or_default()
}

/// URL path of a directory: its whole relative path, where a file's loses
/// its extension (`v1.2` stays `v1.2`).
///
/// Joins components because `\` is a legal name character on Unix.
pub(crate) fn dir_url_path(rel_path: &Path) -> String {
    let segments: Vec<_> = rel_path.iter().map(|part| part.to_string_lossy()).collect();
    segments.join("/")
}

/// Where [`FsStorage::watch`](crate::FsStorage::watch) hears about the README.
pub(crate) struct ReadmeWatch {
    /// Directories outside `source_dir` to watch non-recursively, each with
    /// the README's file name in it.
    pub(crate) dirs: Vec<(PathBuf, OsString)>,
    /// A symlinked README's target inside `source_dir`, relative to it. The
    /// `source_dir` watch reports it, under its own url.
    pub(crate) target_in_source: Option<PathBuf>,
}

/// Owner of the url→file probe order for a source tree, plus the file-level
/// config (`source_dir`, README fallback) that determines it.
///
/// The only answer to "which files serve this url": scan, the request path
/// (`read`/`mtime`), and the watch path all ask it, so they cannot disagree.
///
/// Cloneable because the watch drain thread has no `&FsStorage` — `Storage::watch`
/// hands out no `Arc`. Clone it into that thread; do not re-derive the config
/// there.
#[derive(Debug, Clone)]
pub(crate) struct PathResolver {
    /// Root directory for document storage.
    source_dir: PathBuf,
    /// `README.md` in the project directory, used as the homepage fallback when
    /// `source_dir` has no directory page of its own. Only the candidate path:
    /// the file need not exist.
    readme_path: PathBuf,
    /// `readme_path` lies inside `source_dir` (as with `source_dir = "."`), so
    /// it is an ordinary directory page there, never the fallback.
    readme_in_source: bool,
    /// What [`Self::source_in_project`] returns, computed once.
    source_in_project: String,
    /// The canonical project directory, `None` if it did not exist.
    project: Option<PathBuf>,
}

impl PathResolver {
    /// Build a resolver.
    ///
    /// `project_dir` is the project root — `rw_config::Config::project_dir`,
    /// the directory containing `rw.toml`. The `README.md` homepage fallback
    /// lives there.
    ///
    /// Do **not** derive it from `source_dir`: a nested (`docs/site`) or
    /// absolute `docs.source_dir` puts `parent()` somewhere that isn't the
    /// project root. See `rw_config::Config::project_dir`.
    pub(crate) fn new(project_dir: &Path, source_dir: PathBuf) -> Self {
        let readme_path = project_dir.join(README_FILENAME);
        let project = fs::canonicalize(project_dir).ok();
        let source = absolute(&source_dir).ok();
        let readme_lexically_in_source = absolute(&readme_path)
            .ok()
            .zip(source.as_deref())
            .is_some_and(|(readme, source)| readme.starts_with(source));
        // `absolute` keeps `..`, so a prefix match can still lead out.
        let relative = |(source, project): (PathBuf, PathBuf)| {
            source
                .strip_prefix(project)
                .ok()
                .map(dir_url_path)
                .filter(|rel| !rel.split('/').any(|segment| segment == ".."))
        };
        // Only canonical paths match when one side is reached through a
        // symlink, e.g. `/tmp` against `/private/tmp` on macOS. Config
        // refuses a source directory outside the project.
        let source_in_project = source
            .zip(absolute(project_dir).ok())
            .and_then(relative)
            .or_else(|| {
                canonicalize_existing(&source_dir)
                    .zip(project.clone())
                    .and_then(relative)
            })
            .unwrap_or_default();
        let readme_in_source = readme_lexically_in_source || source_in_project.is_empty();
        Self {
            source_dir,
            readme_path,
            readme_in_source,
            source_in_project,
            project,
        }
    }

    /// Whether the canonical `path` is inside the project.
    pub(crate) fn in_project(&self, path: &Path) -> bool {
        self.project
            .as_ref()
            .is_some_and(|project| path.starts_with(project))
    }

    /// Root directory for document storage.
    pub(crate) fn source_dir(&self) -> &Path {
        &self.source_dir
    }

    /// Resolve URL path to content file path.
    ///
    /// 1. `{path}/index.md`, then `{path}/README.md` (directory structure
    ///    preferred)
    /// 2. For the root, `readme_path` (`README.md` in the project directory);
    ///    otherwise `{path}.md` (standalone file fallback), never named
    ///    `index.md` or `README.md`
    ///
    /// Returns `None` if no content file exists.
    pub(crate) fn resolve_content(&self, url_path: &str) -> Option<PathBuf> {
        self.resolve_content_with(url_path, exists_on_disk)
    }

    /// [`Self::resolve_content`], asking `exists` instead of the filesystem.
    pub(crate) fn resolve_content_with(
        &self,
        url_path: &str,
        exists: impl Fn(&Path) -> bool,
    ) -> Option<PathBuf> {
        let dir = self.source_dir.join(url_path);
        if let Some(dir_page) = DIR_PAGE_FILENAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|path| exists(path))
        {
            return Some(dir_page);
        }

        if url_path.is_empty() {
            // A symlink can lead anywhere, such as to `/etc/hosts`.
            return (!self.readme_in_source
                && exists(&self.readme_path)
                && fs::canonicalize(&self.readme_path).is_ok_and(|path| self.in_project(&path)))
            .then(|| self.readme_path.clone());
        }

        let file_path = self.source_dir.join(format!("{url_path}.md"));
        (exists(&file_path) && !is_dir_page(&file_path)).then_some(file_path)
    }

    /// Resolve a page's own metadata file.
    ///
    /// Candidates in precedence order: the canonical `<dir>/meta.yaml`, the
    /// `<dir>/index.meta.yaml` variant, then the sibling `<url_path>.meta.yaml`.
    /// The root (`""`) has no sibling form.
    pub(crate) fn resolve_meta(&self, url_path: &str) -> Option<PathBuf> {
        self.resolve_meta_with(url_path, Path::exists)
    }

    /// [`Self::resolve_meta`], asking `exists` instead of the filesystem.
    pub(crate) fn resolve_meta_with(
        &self,
        url_path: &str,
        exists: impl Fn(&Path) -> bool,
    ) -> Option<PathBuf> {
        let dir = if url_path.is_empty() {
            self.source_dir.clone()
        } else {
            self.source_dir.join(url_path)
        };

        let canonical = dir.join(META_FILENAME);
        if exists(&canonical) {
            return Some(canonical);
        }

        let index_variant = dir.join(INDEX_META_FILENAME);
        if exists(&index_variant) {
            return Some(index_variant);
        }

        if url_path.is_empty() {
            return None;
        }
        let sibling = self.source_dir.join(format!("{url_path}{META_SUFFIX}"));
        exists(&sibling).then_some(sibling)
    }

    /// `path` as a warning names it: like [`Self::source`] where it can be.
    pub(crate) fn display_path(&self, path: &Path) -> String {
        self.source(path)
            .unwrap_or_else(|| path.display().to_string())
    }

    /// The [`Document::source`](rw_storage::Document::source) of `path`, a
    /// file [`Self::resolve_content`] returned, or that form of any file in
    /// the source directory.
    pub(crate) fn source(&self, path: &Path) -> Option<String> {
        // Plain equality is sound only because `resolve_content` returns a
        // clone of the field. Do NOT make that method canonicalize or rebuild
        // the path without making this a canonicalized comparison too.
        if !self.readme_in_source && self.readme_path == path {
            return Some(README_FILENAME.to_owned());
        }
        let in_source = dir_url_path(path.strip_prefix(&self.source_dir).ok()?);
        Some(match self.source_in_project.as_str() {
            "" => in_source,
            dir => format!("{dir}/{in_source}"),
        })
    }

    /// `source_dir` relative to the project directory, `/`-separated.
    pub(crate) fn source_in_project(&self) -> &str {
        &self.source_in_project
    }

    /// The README homepage candidate's path.
    pub(crate) fn readme_path(&self) -> &Path {
        &self.readme_path
    }

    /// Where watch events for the README come from, besides its own url.
    ///
    /// Its own directory when outside `source_dir`, whether or not a README
    /// exists yet. For a symlink, also its target, because editing the target
    /// changes nothing in the link's directory.
    pub(crate) fn readme_watch(&self) -> ReadmeWatch {
        let mut watch = ReadmeWatch {
            dirs: Vec::new(),
            target_in_source: None,
        };
        if !self.readme_in_source
            && let Some(dir) = self.readme_path.parent()
        {
            watch
                .dirs
                .push((dir.to_path_buf(), OsString::from(README_FILENAME)));
        }
        if fs::symlink_metadata(&self.readme_path).is_ok_and(|meta| meta.is_symlink())
            && let Ok(target) = fs::canonicalize(&self.readme_path)
        {
            let source_dir = self.source_dir.canonicalize().ok();
            match source_dir
                .as_deref()
                .and_then(|src| target.strip_prefix(src).ok())
            {
                // Already reported at the root.
                Some(rel) if rel == Path::new("index.md") => {}
                Some(rel) => watch.target_in_source = Some(rel.to_path_buf()),
                None => {
                    if let (Some(dir), Some(name)) = (target.parent(), target.file_name()) {
                        watch.dirs.push((dir.to_path_buf(), name.to_owned()));
                    }
                }
            }
        }
        watch
    }
}

/// `path` with symlinks resolved as far as it exists.
fn canonicalize_existing(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path)
        .ok()
        .or_else(|| Some(canonicalize_existing(path.parent()?)?.join(path.file_name()?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_in_project_accepts_relative_dirs() {
        for (project_dir, source_dir) in [(".", "docs"), (".", "./docs")] {
            let resolver = PathResolver::new(Path::new(project_dir), PathBuf::from(source_dir));

            assert_eq!(
                resolver.source_in_project(),
                "docs",
                "{project_dir:?} {source_dir:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn source_in_project_sees_through_a_symlinked_path() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("real/proj");
        std::fs::create_dir_all(project.join("site/docs")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
        let linked = dir.path().join("link/proj");

        for (project_dir, source_dir) in [
            (project.clone(), linked.join("site/docs")),
            (linked.clone(), project.join("site/docs")),
        ] {
            let resolver = PathResolver::new(&project_dir, source_dir);

            assert_eq!(resolver.source_in_project(), "site/docs");
        }
    }

    #[cfg(unix)]
    #[test]
    fn readme_watch_adds_no_dir_for_a_project_source_dir_reached_through_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("README.md"), "# Home").unwrap();
        std::os::unix::fs::symlink(&project, dir.path().join("link")).unwrap();

        let resolver = PathResolver::new(&project, dir.path().join("link"));

        assert_eq!(resolver.readme_watch().dirs, []);
    }

    #[test]
    fn test_file_path_to_url() {
        assert_eq!(file_path_to_url(Path::new("index.md")), "");
        assert_eq!(file_path_to_url(Path::new("guide.md")), "guide");
        assert_eq!(file_path_to_url(Path::new("domain/index.md")), "domain");
        assert_eq!(
            file_path_to_url(Path::new("domain/setup.md")),
            "domain/setup"
        );
        assert_eq!(file_path_to_url(Path::new("a/b/c.md")), "a/b/c");
        assert_eq!(file_path_to_url(Path::new("index/index.md")), "index");
    }

    #[test]
    fn is_excluded_by_a_dot_or_underscore_component() {
        for path in [
            ".meta.yaml",
            "dir/.hidden.md",
            ".rw/cache/x",
            "_note.md",
            "guide/_note.md",
            "_partials/warning.md",
            "a/_b/c.md",
            "_partials/meta.yaml",
        ] {
            assert!(is_excluded(Path::new(path)), "{path}");
        }
        for path in [
            "dir/visible.md",
            "payments.meta.yaml",
            "snake_case.md",
            "a_b/c.md",
            "",
        ] {
            assert!(!is_excluded(Path::new(path)), "{path}");
        }
        assert!(is_hidden(Path::new("dir/.hidden.md")));
        assert!(!is_hidden(Path::new("_partials/warning.md")));
    }

    // --- classify_relpath (shared classifier) ---

    #[test]
    fn classify_relpath_underscore_paths_are_none() {
        for path in [
            "_note.md",
            "guide/_note.md",
            "_partials/index.md",
            "_partials/README.md",
            "a/_b/c.md",
            "_partials/meta.yaml",
            "_partials/index.meta.yaml",
            "_draft.meta.yaml",
            "_partials/x.meta.yaml",
        ] {
            let path = Path::new(path);
            let filename = path.file_name().unwrap().to_string_lossy();
            assert_eq!(classify_relpath(path, &filename), None, "{path:?}");
        }
    }

    #[test]
    fn classify_relpath_content_md() {
        let c = classify_relpath(Path::new("domain/guide.md"), "guide.md").unwrap();
        assert_eq!(
            c,
            Classification::Content {
                url_path: "domain/guide".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_index_md_collapses_to_parent() {
        let c = classify_relpath(Path::new("domain/index.md"), "index.md").unwrap();
        assert_eq!(
            c,
            Classification::Content {
                url_path: "domain".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_bare_meta_is_canonical_dir() {
        let c = classify_relpath(Path::new("domain/meta.yaml"), "meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: "domain".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_root_bare_meta() {
        let c = classify_relpath(Path::new("meta.yaml"), "meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: String::new()
            }
        );
    }

    #[test]
    fn classify_relpath_index_meta_is_index_dir_at_parent() {
        let c = classify_relpath(Path::new("dir/index.meta.yaml"), "index.meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: "dir".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_root_index_meta_maps_to_empty() {
        let c = classify_relpath(Path::new("index.meta.yaml"), "index.meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: String::new()
            }
        );
    }

    #[test]
    fn classify_relpath_named_sibling() {
        let c = classify_relpath(
            Path::new("systems/payments.meta.yaml"),
            "payments.meta.yaml",
        )
        .unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: "systems/payments".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_root_named_sibling() {
        let c = classify_relpath(Path::new("payments.meta.yaml"), "payments.meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: "payments".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_hidden_meta_is_none() {
        // ".meta.yaml" has an empty prefix -> not a named file, not bare -> None
        assert!(classify_relpath(Path::new(".meta.yaml"), ".meta.yaml").is_none());
    }

    #[test]
    fn classify_relpath_unrecognized_is_none() {
        assert!(classify_relpath(Path::new("notes.txt"), "notes.txt").is_none());
    }

    #[test]
    fn classify_relpath_dotdot_prefix_is_none() {
        // `...meta.yaml` strips to prefix ".." — must not become a sibling page.
        assert!(classify_relpath(Path::new("...meta.yaml"), "...meta.yaml").is_none());
    }

    #[test]
    fn classify_relpath_dot_prefix_is_none() {
        // `..meta.yaml` strips to prefix "." — must not become a sibling page.
        assert!(classify_relpath(Path::new("..meta.yaml"), "..meta.yaml").is_none());
    }

    #[test]
    fn classify_relpath_embedded_dotdot_prefix_is_sibling() {
        let c = classify_relpath(Path::new("a..b.meta.yaml"), "a..b.meta.yaml");
        assert_eq!(
            c,
            Some(Classification::Metadata {
                url_path: "a..b".to_owned()
            })
        );
    }

    #[test]
    fn classify_relpath_into_url_path() {
        let c = classify_relpath(Path::new("dir/index.meta.yaml"), "index.meta.yaml").unwrap();
        assert_eq!(c.into_url_path(), "dir");
    }

    // --- PathResolver ---

    #[test]
    fn resolver_resolve_meta_prefers_canonical_dir_over_index_variant() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("guide")).unwrap();
        std::fs::write(root.join("guide/meta.yaml"), "title: Canonical").unwrap();
        std::fs::write(root.join("guide/index.meta.yaml"), "title: Variant").unwrap();

        let resolver = PathResolver::new(root, root.to_path_buf());

        assert_eq!(
            resolver.resolve_meta("guide"),
            Some(root.join("guide/meta.yaml"))
        );
    }

    #[test]
    fn resolver_resolve_meta_falls_back_to_named_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("payments.meta.yaml"), "title: Payments").unwrap();

        let resolver = PathResolver::new(root, root.to_path_buf());

        assert_eq!(
            resolver.resolve_meta("payments"),
            Some(root.join("payments.meta.yaml"))
        );
    }

    #[test]
    fn resolver_resolve_content_prefers_directory_index() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("domain")).unwrap();
        std::fs::write(root.join("domain/index.md"), "# Domain").unwrap();
        std::fs::write(root.join("domain.md"), "# Standalone").unwrap();

        let resolver = PathResolver::new(root, root.to_path_buf());

        assert_eq!(
            resolver.resolve_content("domain"),
            Some(root.join("domain/index.md"))
        );
    }

    #[test]
    fn fallback_name_for_readme_homepage_is_the_shared_const() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(root.join("README.md"), "Body.").unwrap();

        let resolver = PathResolver::new(root, docs);
        let homepage = resolver.resolve_content("").unwrap();

        // Not "readme.md" — the README homepage is titled like `scan` titles it.
        assert_eq!(
            content_fallback_name("", Some(&homepage)),
            HOMEPAGE_FALLBACK_NAME
        );
    }

    #[test]
    fn fallback_name_for_resolved_file_is_its_lowercased_filename() {
        // No file is created: the name comes from the path, not from disk.
        assert_eq!(
            content_fallback_name("guide", Some(Path::new("/docs/Guide.md"))),
            "guide.md"
        );
    }

    #[test]
    fn fallback_name_for_index_md_is_its_directorys() {
        assert_eq!(
            content_fallback_name(
                "a/setup-guide",
                Some(Path::new("/docs/a/setup-guide/index.md"))
            ),
            "setup-guide"
        );
        assert_eq!(
            content_fallback_name("", Some(Path::new("/docs/index.md"))),
            HOMEPAGE_FALLBACK_NAME
        );
    }

    #[test]
    fn fallback_name_without_resolution_is_the_urls_last_segment() {
        assert_eq!(content_fallback_name("a/b/setup", None), "setup");
        assert_eq!(content_fallback_name("guide", None), "guide");
    }

    #[test]
    fn fallback_name_without_resolution_at_root_is_the_shared_const() {
        assert_eq!(content_fallback_name("", None), HOMEPAGE_FALLBACK_NAME);
    }

    #[cfg(unix)]
    #[test]
    fn readme_watch_tracks_a_target_in_source_dir_unless_it_is_the_root_index() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("index.md"), "# Index").unwrap();
        std::fs::write(docs.join("overview.md"), "# Overview").unwrap();
        let resolver = PathResolver::new(root, docs.clone());
        let readme = root.join("README.md");

        std::os::unix::fs::symlink(docs.join("overview.md"), &readme).unwrap();
        let watch = resolver.readme_watch();
        assert_eq!(
            watch.dirs,
            [(root.to_path_buf(), OsString::from("README.md"))]
        );
        assert_eq!(watch.target_in_source, Some(PathBuf::from("overview.md")));

        std::fs::remove_file(&readme).unwrap();
        std::os::unix::fs::symlink(docs.join("index.md"), &readme).unwrap();
        assert_eq!(resolver.readme_watch().target_in_source, None);
    }

    #[test]
    fn resolver_resolve_content_root_falls_back_to_readme() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(root.join("README.md"), "# Readme Home").unwrap();

        let resolver = PathResolver::new(root, docs);

        assert_eq!(resolver.resolve_content(""), Some(root.join("README.md")));
    }
}
