//! Source file classification for document discovery.
//!
//! This module provides types for classifying files discovered during scanning.
//! Files with the same `url_path` are combined into a single `DocumentRef`.

use std::fs::DirEntry;
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
    /// Attempt to create a SourceFile from a directory entry.
    ///
    /// Returns `Some` if the entry is a recognized source file:
    /// - `.md` files become `SourceKind::Content`
    /// - Files matching `meta_filename` become `SourceKind::Metadata`
    ///
    /// Returns `None` for:
    /// - Directories
    /// - Symlinks (detected via symlink_metadata)
    /// - Hidden files (starting with `.`)
    /// - Unrecognized file types
    pub fn from_entry(entry: &DirEntry, source_dir: &Path, meta_filename: &str) -> Option<Self> {
        let path = entry.path();
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        // Skip hidden files
        if filename_str.starts_with('.') {
            return None;
        }

        // Skip symlinks - must use symlink_metadata, not file_type()
        // (DirEntry::file_type() follows symlinks on some platforms)
        let metadata = std::fs::symlink_metadata(&path).ok()?;
        if metadata.file_type().is_symlink() {
            return None;
        }

        // Skip directories
        if metadata.is_dir() {
            return None;
        }

        // Compute relative path from source_dir
        let rel_path = path.strip_prefix(source_dir).ok()?;

        // Determine kind and url_path
        if filename_str.ends_with(".md") {
            // Content file - url_path depends on whether it's index.md
            let url_path = if filename_str == "index.md" {
                // index.md -> parent directory's url_path
                rel_path
                    .parent()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default()
            } else {
                // standalone.md -> parent/stem
                let stem = rel_path.file_stem()?.to_string_lossy();
                match rel_path.parent() {
                    Some(parent) if parent.as_os_str().is_empty() => stem.into_owned(),
                    Some(parent) => {
                        format!("{}/{}", parent.to_string_lossy().replace('\\', "/"), stem)
                    }
                    None => stem.into_owned(),
                }
            };
            Some(Self {
                url_path,
                kind: SourceKind::Content,
                path,
            })
        } else if filename_str == meta_filename {
            // Metadata file -> parent directory's url_path
            let url_path = rel_path
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_hidden_files_return_none() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden").unwrap();

        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let result = SourceFile::from_entry(&entries[0], temp_dir.path(), "meta.yaml");
        assert!(result.is_none());
    }

    #[test]
    fn test_directories_return_none() {
        let temp_dir = create_test_dir();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let result = SourceFile::from_entry(&entries[0], temp_dir.path(), "meta.yaml");
        assert!(result.is_none());
    }

    #[test]
    fn test_md_files_return_content() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let result = SourceFile::from_entry(&entries[0], temp_dir.path(), "meta.yaml").unwrap();
        assert_eq!(result.kind, SourceKind::Content);
        assert_eq!(result.url_path, "guide");
    }

    #[test]
    fn test_index_md_url_path() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();

        let entries: Vec<_> = fs::read_dir(&domain_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let result = SourceFile::from_entry(&entries[0], temp_dir.path(), "meta.yaml").unwrap();
        assert_eq!(result.kind, SourceKind::Content);
        assert_eq!(result.url_path, "domain");
    }

    #[test]
    fn test_meta_files_return_metadata() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("meta.yaml"), "title: Test").unwrap();

        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let result = SourceFile::from_entry(&entries[0], temp_dir.path(), "meta.yaml").unwrap();
        assert_eq!(result.kind, SourceKind::Metadata);
        assert_eq!(result.url_path, "");
    }

    #[test]
    fn test_nested_standalone_md() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("api.md"), "# API").unwrap();

        let entries: Vec<_> = fs::read_dir(&domain_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let result = SourceFile::from_entry(&entries[0], temp_dir.path(), "meta.yaml").unwrap();
        assert_eq!(result.kind, SourceKind::Content);
        assert_eq!(result.url_path, "domain/api");
    }

    #[test]
    fn test_unrecognized_files_return_none() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("readme.txt"), "text").unwrap();

        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let result = SourceFile::from_entry(&entries[0], temp_dir.path(), "meta.yaml");
        assert!(result.is_none());
    }

    #[test]
    fn test_custom_meta_filename() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("config.yml"), "title: Test").unwrap();
        fs::write(temp_dir.path().join("meta.yaml"), "ignored").unwrap();

        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        for entry in &entries {
            let filename = entry.file_name();
            if filename.to_string_lossy() == "config.yml" {
                let result =
                    SourceFile::from_entry(entry, temp_dir.path(), "config.yml").unwrap();
                assert_eq!(result.kind, SourceKind::Metadata);
            } else if filename.to_string_lossy() == "meta.yaml" {
                // meta.yaml should return None when looking for config.yml
                let result = SourceFile::from_entry(entry, temp_dir.path(), "config.yml");
                assert!(result.is_none());
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_symlinks_return_none() {
        use std::os::unix::fs::symlink;

        let temp_dir = create_test_dir();
        let target = temp_dir.path().join("target.md");
        fs::write(&target, "# Target").unwrap();
        symlink(&target, temp_dir.path().join("link.md")).unwrap();

        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        for entry in &entries {
            let filename = entry.file_name();
            if filename.to_string_lossy() == "link.md" {
                let result = SourceFile::from_entry(entry, temp_dir.path(), "meta.yaml");
                assert!(result.is_none(), "symlink should return None");
            }
        }
    }

    #[test]
    fn test_deeply_nested_path() {
        let temp_dir = create_test_dir();
        let deep_dir = temp_dir.path().join("a").join("b").join("c");
        fs::create_dir_all(&deep_dir).unwrap();
        fs::write(deep_dir.join("doc.md"), "# Doc").unwrap();

        let entries: Vec<_> = fs::read_dir(&deep_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let result = SourceFile::from_entry(&entries[0], temp_dir.path(), "meta.yaml").unwrap();
        assert_eq!(result.url_path, "a/b/c/doc");
    }

    #[test]
    fn test_root_index_md() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();

        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        let result = SourceFile::from_entry(&entries[0], temp_dir.path(), "meta.yaml").unwrap();
        assert_eq!(result.kind, SourceKind::Content);
        assert_eq!(result.url_path, "");
    }
}
