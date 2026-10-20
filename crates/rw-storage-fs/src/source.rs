//! The url↔file mapping for a source tree.
//!
//! Both directions live here so they cannot disagree: file→url classification
//! ([`classify_relpath`]) for the scanner and the watch path, and url→file
//! resolution ([`PathResolver`]) for every place that reads a page's sources.

use glob::Pattern;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf, absolute};

/// Fallback name the root page is titled from when nothing else titles it.
pub(crate) const HOMEPAGE_FALLBACK_NAME: &str = "home";

/// File names that serve their directory's page, in precedence order.
const DIR_PAGE_FILENAMES: [&str; 2] = ["index.md", README_FILENAME];

/// A directory's page when it has no `index.md`; also the homepage fallback in
/// the project directory.
const README_FILENAME: &str = "README.md";

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

/// Classify a relative path into content/metadata + url path.
///
/// Precedence (must stay in sync — that's why it lives in one place):
/// 1. `*.md` → content (url path via `file_path_to_url`)
/// 2. exact `<meta_filename>` → canonical directory metadata (parent url path)
/// 3. `index.<meta_filename>` → directory metadata (parent url path) + warning
/// 4. `<name>.<meta_filename>` → sibling metadata, when `<name>` is non-empty
///    and not `.`/`..`, which would name the directory itself or its parent
/// 5. otherwise → `None`
pub(crate) fn classify_relpath(
    rel_path: &Path,
    filename: &str,
    meta_filename: &str,
) -> Option<Classification> {
    if Path::new(filename)
        .extension()
        .is_some_and(|ext| ext == "md")
    {
        return Some(Classification::Content {
            url_path: file_path_to_url(rel_path),
        });
    }

    if filename == meta_filename {
        return Some(Classification::Metadata {
            url_path: parent_url_path(rel_path),
        });
    }

    let suffix = format!(".{meta_filename}");
    if let Some(prefix) = filename.strip_suffix(&suffix)
        && !prefix.is_empty()
        && prefix != "."
        && prefix != ".."
    {
        if prefix == "index" {
            tracing::warn!(
                file = %rel_path.display(),
                canonical = %meta_filename,
                "metadata file `index.{meta_filename}` is treated as directory metadata; \
                 rename it to `{meta_filename}`",
            );
            return Some(Classification::Metadata {
                url_path: parent_url_path(rel_path),
            });
        }
        return Some(Classification::Metadata {
            url_path: named_meta_url(rel_path, &suffix),
        });
    }

    None
}

/// Compute the sibling-file url path for a `<name>.<meta_filename>` file.
///
/// Strips the full `.<meta_filename>` suffix from the file name (NOT via
/// `file_stem`, which would leave `payments.meta`) and re-attaches the parent
/// directory — mirroring the standalone-`.md` arm of `file_path_to_url`.
fn named_meta_url(rel_path: &Path, suffix: &str) -> String {
    let filename = rel_path
        .file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_default();
    let stem = filename.strip_suffix(suffix).unwrap_or(&filename);

    match rel_path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => stem.to_owned(),
        Some(parent) => format!("{}/{}", parent.to_string_lossy().replace('\\', "/"), stem),
        None => stem.to_owned(),
    }
}

/// Convert a relative file path to a URL path.
///
/// Handles `.md` extension stripping, directory page files (`index.md`,
/// `README.md`), and Windows path separator normalization.
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
            Some(parent) => {
                format!("{}/{}", parent.to_string_lossy().replace('\\', "/"), stem)
            }
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
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::read_dir(dir).is_ok_and(|mut entries| {
        entries.any(|entry| {
            entry.is_ok_and(|entry| Some(entry.file_name().as_os_str()) == path.file_name())
        })
    })
}

/// Whether `path` names a `README.md`.
pub(crate) fn is_readme(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == README_FILENAME)
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
pub(crate) fn dir_url_path(rel_path: &Path) -> String {
    rel_path.to_string_lossy().replace('\\', "/")
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
/// config (`source_dir`, `meta_filename`, README fallback) that determines it.
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
    /// Metadata file name (e.g. "meta.yaml").
    meta_filename: String,
    /// `README.md` in the project directory, used as the homepage fallback when
    /// `source_dir` has no directory page of its own. Only the candidate path:
    /// the file need not exist.
    readme_path: PathBuf,
    /// `readme_path` lies inside `source_dir` (as with `source_dir = "."`), so
    /// it is an ordinary directory page there, never the fallback.
    readme_in_source: bool,
    /// What [`Self::source_in_project`] returns, computed once.
    source_in_project: Option<String>,
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
    pub(crate) fn new(project_dir: &Path, source_dir: PathBuf, meta_filename: &str) -> Self {
        let readme_path = project_dir.join(README_FILENAME);
        let source = absolute(&source_dir).ok();
        let readme_in_source = absolute(&readme_path)
            .ok()
            .zip(source.as_deref())
            .is_some_and(|(readme, source)| readme.starts_with(source));
        // `absolute` rejects the empty parent of a bare `rw.toml`, and keeps
        // `..`, so a prefix match can still lead out.
        let project = Some(project_dir)
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let relative = |(source, project): (PathBuf, PathBuf)| {
            source
                .strip_prefix(project)
                .ok()
                .map(dir_url_path)
                .filter(|rel| !rel.split('/').any(|segment| segment == ".."))
        };
        // Only canonical paths match when one side is reached through a
        // symlink, e.g. `/tmp` against `/private/tmp` on macOS.
        let source_in_project = source
            .zip(absolute(project).ok())
            .and_then(relative)
            .or_else(|| {
                fs::canonicalize(&source_dir)
                    .ok()
                    .zip(fs::canonicalize(project).ok())
                    .and_then(relative)
            });
        Self {
            source_dir,
            meta_filename: meta_filename.to_owned(),
            readme_path,
            readme_in_source,
            source_in_project,
        }
    }

    /// Root directory for document storage.
    pub(crate) fn source_dir(&self) -> &Path {
        &self.source_dir
    }

    /// Every file whose change can affect a document: markdown, plus both
    /// metadata forms.
    pub(crate) fn watch_patterns(&self) -> Vec<Pattern> {
        let meta_filename = &self.meta_filename;
        vec![
            Pattern::new("**/*.md").expect("invalid glob pattern"),
            Pattern::new(&format!("**/{meta_filename}")).expect("invalid glob pattern"),
            Pattern::new(&format!("**/*.{meta_filename}")).expect("invalid glob pattern"),
        ]
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
            return (!self.readme_in_source && exists(&self.readme_path))
                .then(|| self.readme_path.clone());
        }

        let file_path = self.source_dir.join(format!("{url_path}.md"));
        (exists(&file_path) && !is_dir_page(&file_path)).then_some(file_path)
    }

    /// Resolve a page's own metadata file.
    ///
    /// Candidates in precedence order: the canonical `<dir>/<meta_filename>`,
    /// the `<dir>/index.<meta_filename>` variant, then the sibling
    /// `<url_path>.<meta_filename>`. The root (`""`) has no sibling form.
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

        let canonical = dir.join(&self.meta_filename);
        if exists(&canonical) {
            return Some(canonical);
        }

        let index_variant = dir.join(format!("index.{}", self.meta_filename));
        if exists(&index_variant) {
            return Some(index_variant);
        }

        if url_path.is_empty() {
            return None;
        }
        let sibling = self
            .source_dir
            .join(format!("{url_path}.{}", self.meta_filename));
        exists(&sibling).then_some(sibling)
    }

    /// Classify a path relative to `source_dir`, using this resolver's config.
    pub(crate) fn classify_relpath(
        &self,
        rel_path: &Path,
        filename: &str,
    ) -> Option<Classification> {
        classify_relpath(rel_path, filename, &self.meta_filename)
    }

    /// Whether `path` is the project `README.md` serving as the homepage
    /// fallback, as [`Self::resolve_content`] returns it.
    pub(crate) fn is_fallback_readme(&self, path: &Path) -> bool {
        // Plain equality is sound only because `resolve_content` returns a
        // clone of the field. Do NOT make that method canonicalize or rebuild
        // the path without making this a canonicalized comparison too.
        !self.readme_in_source && self.readme_path == path
    }

    /// `source_dir` relative to the project directory, `/`-separated, or
    /// `None` when it lies outside.
    pub(crate) fn source_in_project(&self) -> Option<&str> {
        self.source_in_project.as_deref()
    }

    /// How a project-relative path names `source_dir`: its path in the
    /// project, or just its name when it lies outside.
    pub(crate) fn source_label(&self) -> Option<&str> {
        self.source_in_project()
            .or_else(|| self.source_dir.file_name()?.to_str())
    }

    /// Every url path a source file maps to.
    ///
    /// See [`crate::FsStorage::url_paths_for_source`] for the accepted input
    /// forms and the result contract.
    pub(crate) fn url_paths_for_source(&self, file_path: &Path) -> Vec<String> {
        // `components` drops every `.` but a leading one.
        let file_path: PathBuf = file_path
            .components()
            .filter(|component| *component != Component::CurDir)
            .collect();
        let file_path = file_path.as_path();
        let mut urls: Vec<String> = Vec::new();
        let mut push = |u: String| {
            if !urls.contains(&u) {
                urls.push(u);
            }
        };

        // The project README maps to the root url, but only while it is the
        // served homepage.
        let readme = &self.readme_path;
        if (file_path == Path::new("README.md")
            || absolute(file_path).ok() == absolute(readme).ok())
            && self.resolve_content("").as_deref() == Some(readme.as_path())
        {
            push(String::new());
        }

        let mut rels: Vec<PathBuf> = Vec::new();
        if file_path.is_absolute() {
            // The input and `source_dir` may name the tree through different
            // symlinks. Only the parent is canonicalized, so a symlinked page
            // still resolves from where it sits.
            let rel = file_path
                .strip_prefix(&self.source_dir)
                .map(Path::to_path_buf)
                .ok()
                .or_else(|| {
                    let dir = fs::canonicalize(file_path.parent()?).ok()?;
                    let source = fs::canonicalize(&self.source_dir).ok()?;
                    let path = dir.join(file_path.file_name()?);
                    Some(path.strip_prefix(source).ok()?.to_path_buf())
                });
            rels.extend(rel);
        } else {
            rels.push(file_path.to_path_buf());
            if let Some(prefix) = self.source_label()
                && let Ok(stripped) = file_path.strip_prefix(prefix)
            {
                rels.push(stripped.to_path_buf());
            }
        }

        // A `..` survives into the url and keys a page that does not exist.
        rels.retain(|rel| !rel.components().any(|c| c == Component::ParentDir));
        for rel in rels {
            let path = self.source_dir.join(&rel);
            if !path.is_file() {
                continue;
            }
            let Some(name) = rel.file_name().map(OsStr::to_string_lossy) else {
                continue;
            };
            // Only the file that serves the url: another one there is excluded.
            if let Some(Classification::Content { url_path }) = self.classify_relpath(&rel, &name)
                && self.resolve_content(&url_path) == Some(path)
            {
                push(url_path);
            }
        }

        urls
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
        if !self.readme_in_source {
            // A bare `README.md` (project dir `""`) has an empty parent.
            let dir = self
                .readme_path
                .parent()
                .filter(|dir| !dir.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_in_project_accepts_relative_dirs() {
        for (project_dir, source_dir) in [("", "docs"), (".", "docs"), (".", "./docs")] {
            let resolver = PathResolver::new(
                Path::new(project_dir),
                PathBuf::from(source_dir),
                "meta.yaml",
            );

            assert_eq!(
                resolver.source_in_project(),
                Some("docs"),
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
        std::fs::write(project.join("site/docs/guide.md"), "# Guide").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
        let linked = dir.path().join("link/proj");

        for (project_dir, source_dir) in [
            (project.clone(), linked.join("site/docs")),
            (linked.clone(), project.join("site/docs")),
        ] {
            let resolver = PathResolver::new(&project_dir, source_dir, "meta.yaml");

            assert_eq!(resolver.source_in_project(), Some("site/docs"));
            assert_eq!(resolver.source_label(), Some("site/docs"));
            assert_eq!(
                resolver.url_paths_for_source(Path::new("site/docs/guide.md")),
                ["guide"]
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn url_paths_for_source_sees_an_absolute_path_through_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("real/proj/docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("guide.md"), "# Guide").unwrap();
        std::fs::write(dir.path().join("real/proj/shared.md"), "# Shared").unwrap();
        std::os::unix::fs::symlink("../shared.md", docs.join("alias.md")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
        let linked = dir.path().join("link/proj/docs");

        for (source_dir, given) in [(&linked, &docs), (&docs, &linked)] {
            let project = source_dir.parent().unwrap();
            let resolver = PathResolver::new(project, source_dir.clone(), "meta.yaml");

            for (file, url) in [("guide.md", "guide"), ("alias.md", "alias")] {
                assert_eq!(
                    resolver.url_paths_for_source(&given.join(file)),
                    [url],
                    "{}",
                    given.display()
                );
            }
        }
    }

    #[test]
    fn url_paths_for_source_rejects_parent_dir_paths() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        let docs = project.join("docs");
        let content = dir.path().join("content");
        for source in [&docs, &content] {
            std::fs::create_dir_all(source).unwrap();
            std::fs::write(source.join("guide.md"), "# Guide").unwrap();
        }

        for (source_dir, file) in [
            (&docs, "docs/../docs/guide.md"),
            (&docs, "../docs/guide.md"),
            (&content, "../content/guide.md"),
        ] {
            let resolver = PathResolver::new(&project, source_dir.clone(), "meta.yaml");

            assert_eq!(
                resolver.url_paths_for_source(Path::new(file)),
                [] as [String; 0],
                "{file}"
            );
        }
        let resolver = PathResolver::new(&project, docs.clone(), "meta.yaml");
        assert_eq!(
            resolver.url_paths_for_source(&docs.join("../docs/guide.md")),
            [] as [String; 0]
        );
    }

    #[test]
    fn url_paths_for_source_accepts_a_leading_cur_dir() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("guide.md"), "# Guide").unwrap();
        let resolver = PathResolver::new(dir.path(), docs, "meta.yaml");

        for file in ["./docs/guide.md", "./guide.md"] {
            assert_eq!(
                resolver.url_paths_for_source(Path::new(file)),
                ["guide"],
                "{file}"
            );
        }
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

    // --- classify_relpath (shared classifier) ---

    #[test]
    fn classify_relpath_content_md() {
        let c = classify_relpath(Path::new("domain/guide.md"), "guide.md", "meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Content {
                url_path: "domain/guide".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_index_md_collapses_to_parent() {
        let c = classify_relpath(Path::new("domain/index.md"), "index.md", "meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Content {
                url_path: "domain".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_bare_meta_is_canonical_dir() {
        let c = classify_relpath(Path::new("domain/meta.yaml"), "meta.yaml", "meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: "domain".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_root_bare_meta() {
        let c = classify_relpath(Path::new("meta.yaml"), "meta.yaml", "meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: String::new()
            }
        );
    }

    #[test]
    fn classify_relpath_index_meta_is_index_dir_at_parent() {
        let c = classify_relpath(
            Path::new("dir/index.meta.yaml"),
            "index.meta.yaml",
            "meta.yaml",
        )
        .unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: "dir".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_root_index_meta_maps_to_empty() {
        let c =
            classify_relpath(Path::new("index.meta.yaml"), "index.meta.yaml", "meta.yaml").unwrap();
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
            "meta.yaml",
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
        let c = classify_relpath(
            Path::new("payments.meta.yaml"),
            "payments.meta.yaml",
            "meta.yaml",
        )
        .unwrap();
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
        assert!(classify_relpath(Path::new(".meta.yaml"), ".meta.yaml", "meta.yaml").is_none());
    }

    #[test]
    fn classify_relpath_custom_dotted_filename() {
        let sibling = classify_relpath(
            Path::new("dir/app.config.yml"),
            "app.config.yml",
            "config.yml",
        )
        .unwrap();
        assert_eq!(
            sibling,
            Classification::Metadata {
                url_path: "dir/app".to_owned()
            }
        );
        let bare =
            classify_relpath(Path::new("dir/config.yml"), "config.yml", "config.yml").unwrap();
        assert_eq!(
            bare,
            Classification::Metadata {
                url_path: "dir".to_owned()
            }
        );
    }

    #[test]
    fn classify_relpath_unrecognized_is_none() {
        assert!(classify_relpath(Path::new("notes.txt"), "notes.txt", "meta.yaml").is_none());
    }

    #[test]
    fn classify_relpath_dotdot_prefix_is_none() {
        // `...meta.yaml` strips to prefix ".." — must not become a sibling page.
        assert!(classify_relpath(Path::new("...meta.yaml"), "...meta.yaml", "meta.yaml").is_none());
    }

    #[test]
    fn classify_relpath_dot_prefix_is_none() {
        // `..meta.yaml` strips to prefix "." — must not become a sibling page.
        assert!(classify_relpath(Path::new("..meta.yaml"), "..meta.yaml", "meta.yaml").is_none());
    }

    #[test]
    fn classify_relpath_embedded_dotdot_prefix_is_sibling() {
        let c = classify_relpath(Path::new("a..b.meta.yaml"), "a..b.meta.yaml", "meta.yaml");
        assert_eq!(
            c,
            Some(Classification::Metadata {
                url_path: "a..b".to_owned()
            })
        );
    }

    #[test]
    fn classify_relpath_into_url_path() {
        let c = classify_relpath(
            Path::new("dir/index.meta.yaml"),
            "index.meta.yaml",
            "meta.yaml",
        )
        .unwrap();
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

        let resolver = PathResolver::new(root, root.to_path_buf(), "meta.yaml");

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

        let resolver = PathResolver::new(root, root.to_path_buf(), "meta.yaml");

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

        let resolver = PathResolver::new(root, root.to_path_buf(), "meta.yaml");

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

        let resolver = PathResolver::new(root, docs, "meta.yaml");
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
        let resolver = PathResolver::new(root, docs.clone(), "meta.yaml");
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

        let resolver = PathResolver::new(root, docs, "meta.yaml");

        assert_eq!(resolver.resolve_content(""), Some(root.join("README.md")));
    }
}
