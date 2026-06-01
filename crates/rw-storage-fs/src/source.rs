//! Source file classification for document discovery.
//!
//! This module provides types for classifying files discovered during scanning.
//! Files with the same `url_path` are combined into a single `DocumentRef`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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
    if filename.ends_with(".md") {
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
}
