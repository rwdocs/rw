//! The url↔file mapping for a source tree.
//!
//! Both directions live here so they cannot disagree: file→url classification
//! ([`classify_relpath`]) for the scanner and the watch path, and url→file
//! resolution ([`PathResolver`]) for every place that reads a page's sources.

use glob::Pattern;
use std::ffi::OsStr;
use std::path::{Path, PathBuf, absolute};

/// Fallback name the root page is titled from when nothing else titles it.
pub(crate) const HOMEPAGE_FALLBACK_NAME: &str = "home";

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
/// 4. `<name>.<meta_filename>` → sibling metadata, when `<name>` is a safe stem
///    (non-empty, not `index`, not `.`/`..`, contains no `..`); degenerate
///    stems that would yield a `validate_path`-rejected url path fall through
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
        && !prefix.contains("..")
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
/// Handles `.md` extension stripping, `index.md` special case,
/// and Windows path separator normalization.
///
/// # Examples
///
/// - `index.md` -> `""`
/// - `guide.md` -> `"guide"`
/// - `domain/index.md` -> `"domain"`
/// - `domain/setup.md` -> `"domain/setup"`
pub(crate) fn file_path_to_url(rel_path: &Path) -> String {
    let filename = rel_path
        .file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_default();

    if filename == "index.md" {
        // index.md -> parent directory's url_path
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

/// Get URL path from parent directory of a relative path.
fn parent_url_path(rel_path: &Path) -> String {
    rel_path
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
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
    /// `source_dir/index.md` does not exist.
    ///
    /// This is only the candidate path; whether a file is actually there is
    /// answered by [`PathResolver::existing_readme`].
    readme_path: PathBuf,
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
        let readme_path = project_dir.join("README.md");
        Self {
            source_dir,
            meta_filename: meta_filename.to_owned(),
            readme_path,
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
    /// For root path (`""`):
    /// 1. `source_dir/index.md`
    /// 2. `readme_path` (`README.md` in the project directory)
    ///
    /// For other paths:
    /// 1. `{path}/index.md` (directory structure preferred)
    /// 2. `{path}.md` (standalone file fallback)
    ///
    /// Returns `None` if no content file exists.
    pub(crate) fn resolve_content(&self, url_path: &str) -> Option<PathBuf> {
        self.resolve_content_with(url_path, Path::exists)
    }

    /// [`Self::resolve_content`], asking `exists` instead of the filesystem.
    pub(crate) fn resolve_content_with(
        &self,
        url_path: &str,
        exists: impl Fn(&Path) -> bool,
    ) -> Option<PathBuf> {
        if url_path.is_empty() {
            let index = self.source_dir.join("index.md");
            if exists(&index) {
                return Some(index);
            }
            if exists(&self.readme_path) {
                return Some(self.readme_path.clone());
            }
            return None;
        }

        let index_path = self.source_dir.join(format!("{url_path}/index.md"));
        if exists(&index_path) {
            return Some(index_path);
        }

        let file_path = self.source_dir.join(format!("{url_path}.md"));
        exists(&file_path).then_some(file_path)
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

    /// Whether `path` is the README homepage, as [`Self::resolve_content`]
    /// returns it.
    pub(crate) fn is_readme(&self, path: &Path) -> bool {
        // Plain equality is sound only because `resolve_content` returns a
        // clone of the field. Do NOT make that method canonicalize or rebuild
        // the path without making this a canonicalized comparison too.
        self.readme_path == path
    }

    /// The name `Meta::resolve` falls back to when titling `url_path`.
    ///
    /// A standalone page's own file name. A directory page — `index.md`, the
    /// README homepage, or no content file at all — is named after the url's
    /// last segment, or `"home"` for the root.
    pub(crate) fn content_fallback_name(&self, url_path: &str, resolved: Option<&Path>) -> String {
        if let Some(path) = resolved
            && !self.is_readme(path)
            && let Some(name) = path.file_name()
            && name != "index.md"
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

    /// Every url path a source file maps to.
    ///
    /// See [`crate::FsStorage::url_paths_for_source`] for the accepted input
    /// forms and the result contract.
    pub(crate) fn url_paths_for_source(&self, file_path: &Path) -> Vec<String> {
        let mut urls: Vec<String> = Vec::new();
        let mut push = |u: String| {
            if !urls.contains(&u) {
                urls.push(u);
            }
        };

        // README homepage maps to the root url — but only when it is actually
        // the served homepage: a real `docs/index.md` shadows it.
        let readme = &self.readme_path;
        if (file_path == Path::new("README.md")
            || absolute(file_path).ok() == absolute(readme).ok())
            && self.resolve_content("").as_deref() == Some(readme.as_path())
        {
            push(String::new());
        }

        let mut rels: Vec<PathBuf> = Vec::new();
        if file_path.is_absolute() {
            if let Ok(rel) = file_path.strip_prefix(&self.source_dir) {
                rels.push(rel.to_path_buf());
            }
        } else {
            rels.push(file_path.to_path_buf());
            if let Some(name) = self.source_dir.file_name()
                && let Ok(stripped) = file_path.strip_prefix(name)
            {
                rels.push(stripped.to_path_buf());
            }
        }

        for rel in rels {
            if !self.source_dir.join(&rel).is_file() {
                continue;
            }
            let Some(name) = rel.file_name().map(OsStr::to_string_lossy) else {
                continue;
            };
            if let Some(Classification::Content { url_path }) = self.classify_relpath(&rel, &name) {
                push(url_path);
            }
        }

        urls
    }

    /// The README homepage *candidate*, when a README exists on disk.
    ///
    /// Existence alone — a project with `docs/index.md` has an existing README
    /// that is never served, and this still returns it. Callers that need the
    /// actually-served homepage go through [`Self::resolve_content`] with `""`.
    pub(crate) fn existing_readme(&self) -> Option<&Path> {
        self.readme_path.exists().then_some(&*self.readme_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn classify_relpath_embedded_dotdot_prefix_is_none() {
        // `a..b.meta.yaml` strips to prefix "a..b"; url path "a..b" would be
        // rejected by validate_path, so it must not classify as a sibling page.
        assert!(
            classify_relpath(Path::new("a..b.meta.yaml"), "a..b.meta.yaml", "meta.yaml").is_none()
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
            resolver.content_fallback_name("", Some(&homepage)),
            HOMEPAGE_FALLBACK_NAME
        );
    }

    #[test]
    fn fallback_name_for_resolved_file_is_its_lowercased_filename() {
        // No file is created: the name comes from the path, not from disk.
        let resolver = PathResolver::new(Path::new("/"), PathBuf::from("/docs"), "meta.yaml");

        assert_eq!(
            resolver.content_fallback_name("guide", Some(Path::new("/docs/Guide.md"))),
            "guide.md"
        );
    }

    #[test]
    fn fallback_name_for_index_md_is_its_directorys() {
        let resolver = PathResolver::new(Path::new("/"), PathBuf::from("/docs"), "meta.yaml");

        assert_eq!(
            resolver.content_fallback_name(
                "a/setup-guide",
                Some(Path::new("/docs/a/setup-guide/index.md"))
            ),
            "setup-guide"
        );
        assert_eq!(
            resolver.content_fallback_name("", Some(Path::new("/docs/index.md"))),
            HOMEPAGE_FALLBACK_NAME
        );
    }

    #[test]
    fn fallback_name_without_resolution_is_the_urls_last_segment() {
        let resolver = PathResolver::new(Path::new("/"), PathBuf::from("/docs"), "meta.yaml");

        assert_eq!(resolver.content_fallback_name("a/b/setup", None), "setup");
        assert_eq!(resolver.content_fallback_name("guide", None), "guide");
    }

    #[test]
    fn fallback_name_without_resolution_at_root_is_the_shared_const() {
        let resolver = PathResolver::new(Path::new("/"), PathBuf::from("/docs"), "meta.yaml");

        assert_eq!(
            resolver.content_fallback_name("", None),
            HOMEPAGE_FALLBACK_NAME
        );
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
