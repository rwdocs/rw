//! Source file classification for document discovery.
//!
//! This module provides types for classifying files discovered during scanning.
//! Files with the same `url_path` are combined into a single `DocumentRef`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// The role a source file plays in document construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceKind {
    /// Content file (.md) - provides page body
    Content,
    /// Metadata file (e.g., meta.yaml) - provides page configuration
    Metadata,
}

/// A source file discovered during scanning.
///
/// Represents a single file that contributes to a document.
/// Files with the same `url_path` are combined into one `DocumentRef`.
#[derive(Debug, Clone)]
pub(crate) struct SourceFile {
    /// URL path this file contributes to (e.g., "domain", "domain/guide")
    pub url_path: String,
    /// What kind of source this is
    pub kind: SourceKind,
    /// Absolute path to the file
    pub path: PathBuf,
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
        let filename_str = filename.to_string_lossy();

        // Compute relative path from source_dir
        let rel_path = path.strip_prefix(source_dir).ok()?;

        // Determine kind and url_path based on filename
        if filename_str.ends_with(".md") {
            let url_path = compute_url_path(rel_path, &filename_str);
            Some(Self {
                url_path,
                kind: SourceKind::Content,
                path,
            })
        } else if filename_str == meta_filename {
            // Metadata file -> parent directory's url_path
            let url_path = parent_url_path(rel_path);
            Some(Self {
                url_path,
                kind: SourceKind::Metadata,
                path,
            })
        } else {
            None
        }
    }
}

/// Compute URL path for a content file (.md).
fn compute_url_path(rel_path: &Path, filename: &str) -> String {
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
}
