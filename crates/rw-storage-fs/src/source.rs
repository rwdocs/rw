//! The url↔file mapping for a source tree.
//!
//! Both directions live here so they cannot disagree: file→url classification
//! ([`SourceFile`], [`classify_relpath`]) for the scanner and the watch path,
//! and url→file resolution ([`PathResolver`]) for `read`/`exists`/`meta` and
//! the watch drain thread. Files with the same `url_path` are combined into a
//! single `DocumentRef` by `scanner`.

use glob::Pattern;
use rw_meta::Meta;
use std::ffi::OsStr;
use std::fs::read_to_string;
use std::path::{Path, PathBuf, absolute};

/// Fallback name the README homepage is titled from when it has no H1.
///
/// Read by both the scan path's README injection and the watch path, which
/// must agree or the same page gets two different titles.
pub(crate) const HOMEPAGE_FALLBACK_NAME: &str = "home";

/// The role a source file plays in document construction.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SourceKind {
    /// Content file (.md) - provides page body
    Content,
    /// Metadata file (e.g., meta.yaml) - provides page configuration
    Metadata,
}

/// Resolution precedence of a metadata file that maps to a url path.
///
/// Lower wins. This mirrors the lookup order in `FsStorage::resolve_meta`
/// (canonical directory form, then the `index.` directory variant, then the
/// sibling form), so the scanner's collision tie-break agrees with `meta()`.
/// Each form is unique per url path, so equal-rank ties never occur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MetaRank {
    /// Exact bare `<meta_filename>` (e.g. `dir/meta.yaml`).
    CanonicalDir,
    /// `index.<meta_filename>` directory variant (e.g. `dir/index.meta.yaml`).
    IndexDir,
    /// Sibling `<name>.<meta_filename>` (e.g. `dir/foo.meta.yaml`).
    Sibling,
}

/// The classification decision for a single relative path.
///
/// Shared by `SourceFile::classify` (scan) and `to_storage_event` (watch) so
/// the two never drift apart.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Classification {
    /// Content file (`.md`).
    Content { url_path: String },
    /// Metadata file, with its resolution rank.
    Metadata { url_path: String, rank: MetaRank },
}

impl Classification {
    /// Consume into the url path this file contributes to.
    pub(crate) fn into_url_path(self) -> String {
        match self {
            Self::Content { url_path } | Self::Metadata { url_path, .. } => url_path,
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
            rank: MetaRank::CanonicalDir,
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
                rank: MetaRank::IndexDir,
            });
        }
        return Some(Classification::Metadata {
            url_path: named_meta_url(rel_path, &suffix),
            rank: MetaRank::Sibling,
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

/// A source file discovered during scanning.
///
/// Represents a single file that contributes to a document.
/// Files with the same `url_path` are combined into one `DocumentRef`.
#[derive(Debug)]
pub(crate) struct SourceFile {
    /// URL path this file contributes to (e.g., "domain", "domain/guide")
    pub url_path: String,
    /// What kind of source this is
    pub kind: SourceKind,
    /// Absolute path to the file
    pub path: PathBuf,
    /// Resolution rank for metadata files; `None` for content.
    pub meta_rank: Option<MetaRank>,
}

impl SourceFile {
    /// Classify a file path as a source file.
    ///
    /// Returns `Some` if the file is a recognized source type:
    /// - `.md` files become `SourceKind::Content`
    /// - Files matching `meta_filename` become `SourceKind::Metadata`
    ///
    /// Returns `None` for unrecognized file types.
    ///
    /// Note: This method assumes the caller has already filtered out
    /// hidden files, symlinks, and directories.
    pub fn classify(
        path: PathBuf,
        filename: &OsStr,
        source_dir: &Path,
        meta_filename: &str,
    ) -> Option<Self> {
        let rel_path = path.strip_prefix(source_dir).ok()?;
        let classification =
            classify_relpath(rel_path, &filename.to_string_lossy(), meta_filename)?;

        Some(match classification {
            Classification::Content { url_path } => Self {
                url_path,
                kind: SourceKind::Content,
                path,
                meta_rank: None,
            },
            Classification::Metadata { url_path, rank } => Self {
                url_path,
                kind: SourceKind::Metadata,
                path,
                meta_rank: Some(rank),
            },
        })
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
/// `source.rs` already held the file→url direction (`classify_relpath`,
/// `file_path_to_url`); this type adds the reverse, so the request path
/// (`read`/`exists`/`meta`) and the watch path resolve identically instead of
/// each re-deriving the rules.
///
/// The *scan* path is a third encoding of the same precedence — [`MetaRank`]'s
/// ordinal and `Scanner::group_into_documents`' tie-break. Changing precedence
/// here means changing those too; only a test
/// (`scan_and_resolver_agree_across_the_precedence_matrix`) links them.
///
/// Cloneable because the watch drain thread has no `&FsStorage` — `Storage::watch`
/// hands out no `Arc`. Do NOT re-derive the config in that thread instead:
/// a second derivation is how the watch and request paths desynced before.
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
    ///
    /// The two directories differ in reference style because they differ in
    /// use: `source_dir` is stored on the resolver, so it is taken owned, while
    /// `project_dir` is only joined once to form `readme_path`.
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
        if url_path.is_empty() {
            let index = self.source_dir.join("index.md");
            if index.exists() {
                return Some(index);
            }
            if self.readme_path.exists() {
                return Some(self.readme_path.clone());
            }
            return None;
        }

        // Prefer directory/index.md
        let index_path = self.source_dir.join(format!("{url_path}/index.md"));
        if index_path.exists() {
            return Some(index_path);
        }

        // Fall back to standalone file
        let file_path = self.source_dir.join(format!("{url_path}.md"));
        file_path.exists().then_some(file_path)
    }

    /// Resolve a directory's metadata file (directory form).
    ///
    /// Two candidates in precedence order: the canonical
    /// `<dir>/<meta_filename>`, then the `<dir>/index.<meta_filename>` variant.
    /// Returns `None` if neither exists.
    pub(crate) fn resolve_dir_meta(&self, url_path: &str) -> Option<PathBuf> {
        let dir = if url_path.is_empty() {
            self.source_dir.clone()
        } else {
            self.source_dir.join(url_path)
        };

        let canonical = dir.join(&self.meta_filename);
        if canonical.exists() {
            return Some(canonical);
        }

        let index_variant = dir.join(format!("index.{}", self.meta_filename));
        index_variant.exists().then_some(index_variant)
    }

    /// Resolve a page's own metadata file (leaf query).
    ///
    /// Directory form first (see [`Self::resolve_dir_meta`]), then the sibling
    /// `<url_path>.<meta_filename>`. The canonical directory form wins when
    /// multiple exist. The root (`""`) has no sibling form.
    ///
    /// The candidate order here is the same rule [`MetaRank`] encodes as an
    /// ordinal for the scanner's tie-break;
    /// `scan_and_resolver_agree_across_the_precedence_matrix` pins them together.
    pub(crate) fn resolve_meta(&self, url_path: &str) -> Option<PathBuf> {
        if let Some(dir_meta) = self.resolve_dir_meta(url_path) {
            return Some(dir_meta);
        }
        if url_path.is_empty() {
            return None;
        }
        let sibling = self
            .source_dir
            .join(format!("{url_path}.{}", self.meta_filename));
        sibling.exists().then_some(sibling)
    }

    /// Classify a file path as a source file, using this resolver's config.
    ///
    /// Thin wrapper over [`SourceFile::classify`] so callers holding a resolver
    /// do not re-thread `(source_dir, meta_filename)`.
    pub(crate) fn classify(&self, path: PathBuf, filename: &OsStr) -> Option<SourceFile> {
        SourceFile::classify(path, filename, &self.source_dir, &self.meta_filename)
    }

    /// Classify a path relative to `source_dir`, using this resolver's config.
    pub(crate) fn classify_relpath(
        &self,
        rel_path: &Path,
        filename: &str,
    ) -> Option<Classification> {
        classify_relpath(rel_path, filename, &self.meta_filename)
    }

    /// The name `Meta::resolve` falls back to when titling `url_path`.
    ///
    /// Mirrors what the scan path passes: the resolved file's own name, except
    /// for the README homepage, which `FsStorage::scan` injects with a fixed
    /// `"home"`. Passing the README's real filename here instead would title an
    /// H1-less README "Readme" on live reload and "Home" on scan — trading one
    /// disagreement for another.
    ///
    /// Falls back to the url's last segment, then `"home"`, when nothing resolves.
    pub(crate) fn content_fallback_name(&self, url_path: &str, resolved: Option<&Path>) -> String {
        if let Some(path) = resolved {
            // Plain equality against the field is sound only because
            // `resolve_content` returns a clone of it. Do NOT make that method
            // canonicalize or rebuild the path without making this a
            // canonicalized comparison too.
            if self.readme_path == path {
                return HOMEPAGE_FALLBACK_NAME.to_owned();
            }
            if let Some(name) = path.file_name() {
                return name.to_string_lossy().to_lowercase();
            }
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
        // the served homepage. `resolve_content("")` applies the same
        // index.md-then-README precedence the scanner uses, so a project with a
        // real `docs/index.md` (which shadows the README) does not map README.md
        // to the root here.
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
            let file = self.source_dir.join(&rel);
            if !file.is_file() {
                continue;
            }
            let Some(name) = file.file_name().map(OsStr::to_os_string) else {
                continue;
            };
            if let Some(sf) = self.classify(file, &name)
                && sf.kind == SourceKind::Content
            {
                push(sf.url_path);
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

    /// Metadata for the injected README homepage document, when a README exists.
    ///
    /// `scan` calls this only after finding no root document of its own, so the
    /// README's own H1 (or [`HOMEPAGE_FALLBACK_NAME`] without one) titles the
    /// root page. Resolving the title here rather than at the call site is what
    /// keeps scan and the watch path — which titles the same page through
    /// [`Self::content_fallback_name`] — from drifting onto two literals.
    ///
    /// Gated on the README existing, not on it being readable: an unreadable one
    /// still yields metadata, titled from the fallback.
    pub(crate) fn homepage_fallback_meta(&self) -> Option<Meta> {
        let readme = self.existing_readme()?;
        let markdown = read_to_string(readme).ok();
        Some(Meta::resolve(
            markdown.as_deref(),
            None,
            HOMEPAGE_FALLBACK_NAME,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    /// Helper to classify a file path with default meta filename.
    fn classify(source_dir: &str, file_path: &str) -> Option<SourceFile> {
        let source = Path::new(source_dir);
        let path = source.join(file_path);
        let filename = OsString::from(Path::new(file_path).file_name().unwrap_or_default());
        SourceFile::classify(path, &filename, source, "meta.yaml")
    }

    #[test]
    fn test_md_files_return_content() {
        let result = classify("/docs", "guide.md").unwrap();
        assert_eq!(result.kind, SourceKind::Content);
        assert_eq!(result.url_path, "guide");
    }

    #[test]
    fn test_index_md_url_path() {
        let result = classify("/docs", "domain/index.md").unwrap();
        assert_eq!(result.kind, SourceKind::Content);
        assert_eq!(result.url_path, "domain");
    }

    #[test]
    fn test_root_index_md() {
        let result = classify("/docs", "index.md").unwrap();
        assert_eq!(result.kind, SourceKind::Content);
        assert_eq!(result.url_path, "");
    }

    #[test]
    fn test_meta_files_return_metadata() {
        let result = classify("/docs", "meta.yaml").unwrap();
        assert_eq!(result.kind, SourceKind::Metadata);
        assert_eq!(result.url_path, "");
    }

    #[test]
    fn test_nested_meta_file() {
        let result = classify("/docs", "domain/meta.yaml").unwrap();
        assert_eq!(result.kind, SourceKind::Metadata);
        assert_eq!(result.url_path, "domain");
    }

    #[test]
    fn test_nested_standalone_md() {
        let result = classify("/docs", "domain/api.md").unwrap();
        assert_eq!(result.kind, SourceKind::Content);
        assert_eq!(result.url_path, "domain/api");
    }

    #[test]
    fn test_deeply_nested_path() {
        let result = classify("/docs", "a/b/c/doc.md").unwrap();
        assert_eq!(result.url_path, "a/b/c/doc");
    }

    #[test]
    fn test_unrecognized_files_return_none() {
        let result = classify("/docs", "readme.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_custom_meta_filename() {
        let source = Path::new("/docs");
        let path = source.join("config.yml");
        let filename = OsString::from("config.yml");

        // Should match custom meta filename
        let result = SourceFile::classify(path.clone(), &filename, source, "config.yml").unwrap();
        assert_eq!(result.kind, SourceKind::Metadata);

        // Should not match default meta filename
        let result = SourceFile::classify(path, &filename, source, "meta.yaml");
        assert!(result.is_none());
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
    fn classify_named_sibling_meta() {
        let result = classify("/docs", "systems/payments.meta.yaml").unwrap();
        assert_eq!(result.kind, SourceKind::Metadata);
        assert_eq!(result.url_path, "systems/payments");
        assert_eq!(result.meta_rank, Some(MetaRank::Sibling));
    }

    #[test]
    fn classify_index_meta_is_directory() {
        let result = classify("/docs", "dir/index.meta.yaml").unwrap();
        assert_eq!(result.kind, SourceKind::Metadata);
        assert_eq!(result.url_path, "dir");
        assert_eq!(result.meta_rank, Some(MetaRank::IndexDir));
    }

    #[test]
    fn classify_bare_meta_rank() {
        let result = classify("/docs", "dir/meta.yaml").unwrap();
        assert_eq!(result.meta_rank, Some(MetaRank::CanonicalDir));
    }

    #[test]
    fn classify_content_has_no_rank() {
        let result = classify("/docs", "guide.md").unwrap();
        assert_eq!(result.kind, SourceKind::Content);
        assert_eq!(result.meta_rank, None);
    }

    #[test]
    fn classify_hidden_meta_is_none() {
        assert!(classify("/docs", ".meta.yaml").is_none());
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
                url_path: "domain".to_owned(),
                rank: MetaRank::CanonicalDir
            }
        );
    }

    #[test]
    fn classify_relpath_root_bare_meta() {
        let c = classify_relpath(Path::new("meta.yaml"), "meta.yaml", "meta.yaml").unwrap();
        assert_eq!(
            c,
            Classification::Metadata {
                url_path: String::new(),
                rank: MetaRank::CanonicalDir
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
                url_path: "dir".to_owned(),
                rank: MetaRank::IndexDir
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
                url_path: String::new(),
                rank: MetaRank::IndexDir
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
                url_path: "systems/payments".to_owned(),
                rank: MetaRank::Sibling
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
                url_path: "payments".to_owned(),
                rank: MetaRank::Sibling
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
                url_path: "dir/app".to_owned(),
                rank: MetaRank::Sibling
            }
        );
        let bare =
            classify_relpath(Path::new("dir/config.yml"), "config.yml", "config.yml").unwrap();
        assert_eq!(
            bare,
            Classification::Metadata {
                url_path: "dir".to_owned(),
                rank: MetaRank::CanonicalDir
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
