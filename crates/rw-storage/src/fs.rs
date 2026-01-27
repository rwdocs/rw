//! Filesystem storage implementation.
//!
//! Provides [`FsStorage`] for reading documents from the local filesystem
//! with mtime-based caching for title extraction.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

use crate::storage::{Document, Storage, StorageError};

/// Cached file metadata for incremental title extraction.
#[derive(Clone, Debug)]
struct CachedFile {
    /// File modification time.
    mtime: SystemTime,
    /// Extracted title from the file.
    title: String,
}

/// Filesystem storage implementation.
///
/// Scans a source directory recursively for markdown files and extracts
/// titles from the first H1 heading. Uses mtime caching to avoid re-reading
/// unchanged files.
///
/// # Example
///
/// ```ignore
/// use std::path::PathBuf;
/// use rw_storage::{FsStorage, Storage};
///
/// let storage = FsStorage::new(PathBuf::from("docs"));
/// let docs = storage.scan()?;
/// ```
pub struct FsStorage {
    /// Root directory for document storage.
    source_dir: PathBuf,
    /// Regex for extracting first H1 heading.
    h1_regex: Regex,
    /// Mtime cache for incremental title extraction.
    mtime_cache: Mutex<HashMap<PathBuf, CachedFile>>,
}

impl FsStorage {
    /// Create a new filesystem storage.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Root directory containing markdown files
    ///
    /// # Panics
    ///
    /// Panics if the internal regex for H1 heading extraction fails to compile.
    /// This should never happen as the regex is a compile-time constant.
    #[must_use]
    pub fn new(source_dir: PathBuf) -> Self {
        Self {
            source_dir,
            h1_regex: Regex::new(r"(?m)^#\s+(.+)$").unwrap(),
            mtime_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Scan directory recursively and collect documents.
    fn scan_directory(&self, dir_path: &Path, base_path: &Path) -> Vec<Document> {
        let Ok(entries) = fs::read_dir(dir_path) else {
            return Vec::new();
        };

        let mut documents = Vec::new();

        // Collect entries with cached file_type to avoid repeated stat calls in sort.
        let mut entries: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|e| {
                let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
                let name_lower = e.file_name().to_string_lossy().to_lowercase();
                (e, is_dir, name_lower)
            })
            .collect();

        // Sort: directories first, then alphabetical by name
        entries.sort_by(|(_, a_is_dir, a_name), (_, b_is_dir, b_name)| {
            b_is_dir.cmp(a_is_dir).then_with(|| a_name.cmp(b_name))
        });

        for (entry, is_dir, name_lower) in entries {
            // Skip hidden and underscore-prefixed files/dirs
            if name_lower.starts_with('.') || name_lower.starts_with('_') {
                continue;
            }

            // Skip common non-documentation directories
            if is_dir
                && matches!(
                    name_lower.as_str(),
                    "node_modules"
                        | "target"
                        | "dist"
                        | "build"
                        | ".cache"
                        | "vendor"
                        | "__pycache__"
                )
            {
                continue;
            }

            let path = entry.path();

            if is_dir {
                // Recurse into subdirectory
                let rel_path = base_path.join(entry.file_name());
                documents.extend(self.scan_directory(&path, &rel_path));
            } else if path.extension().is_some_and(|e| e == "md") {
                // Process markdown file
                let rel_path = base_path.join(entry.file_name());
                let title = self.get_title(&path, &name_lower);
                documents.push(Document {
                    path: rel_path,
                    title,
                });
            }
        }

        documents
    }

    /// Get title for a file, using mtime cache when possible.
    fn get_title(&self, file_path: &Path, name_lower: &str) -> String {
        // Get current mtime
        let current_mtime = fs::metadata(file_path).ok().and_then(|m| m.modified().ok());

        // Check cache
        {
            let cache = self.mtime_cache.lock().unwrap();
            if let (Some(cached), Some(mtime)) = (cache.get(file_path), current_mtime)
                && cached.mtime == mtime
            {
                return cached.title.clone();
            }
        }

        // Cache miss - extract title
        let title = self
            .extract_title_from_content(file_path)
            .unwrap_or_else(|| Self::title_from_filename(name_lower));

        // Update cache
        if let Some(mtime) = current_mtime {
            let mut cache = self.mtime_cache.lock().unwrap();
            cache.insert(
                file_path.to_path_buf(),
                CachedFile {
                    mtime,
                    title: title.clone(),
                },
            );
        }

        title
    }

    /// Extract title from first H1 heading in markdown file.
    fn extract_title_from_content(&self, file_path: &Path) -> Option<String> {
        let content = fs::read_to_string(file_path).ok()?;
        self.h1_regex
            .captures(&content)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().trim().to_string())
    }

    /// Generate title from filename.
    fn title_from_filename(name_lower: &str) -> String {
        // Remove .md extension
        let name = name_lower.strip_suffix(".md").unwrap_or(name_lower);

        name.replace(['-', '_'], " ")
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Storage for FsStorage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        if !self.source_dir.exists() {
            return Ok(Vec::new());
        }

        Ok(self.scan_directory(&self.source_dir, Path::new("")))
    }

    fn read(&self, path: &Path) -> Result<String, StorageError> {
        let full_path = self.source_dir.join(path);
        fs::read_to_string(&full_path)
            .map_err(|e| StorageError::io(e, Some(full_path.clone())).with_backend("Fs"))
    }

    fn exists(&self, path: &Path) -> bool {
        self.source_dir.join(path).exists()
    }

    fn mtime(&self, path: &Path) -> Result<f64, StorageError> {
        let full_path = self.source_dir.join(path);
        let metadata = fs::metadata(&full_path)
            .map_err(|e| StorageError::io(e, Some(full_path.clone())).with_backend("Fs"))?;
        let modified = metadata
            .modified()
            .map_err(|e| StorageError::io(e, Some(full_path)).with_backend("Fs"))?;
        Ok(modified
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageErrorKind;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_fs_storage_is_send_sync() {
        assert_send_sync::<FsStorage>();
    }

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_scan_empty_dir() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert!(docs.is_empty());
    }

    #[test]
    fn test_scan_missing_dir() {
        let storage = FsStorage::new(PathBuf::from("/nonexistent"));
        let docs = storage.scan().unwrap();

        assert!(docs.is_empty());
    }

    #[test]
    fn test_scan_flat_structure() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# User Guide\n\nContent.").unwrap();
        fs::write(temp_dir.path().join("api.md"), "# API Reference\n\nDocs.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 2);
        let paths: Vec<_> = docs.iter().map(|d| d.path.to_str().unwrap()).collect();
        assert!(paths.contains(&"api.md"));
        assert!(paths.contains(&"guide.md"));
    }

    #[test]
    fn test_scan_nested_structure() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain\n\nOverview.").unwrap();
        fs::write(domain_dir.join("guide.md"), "# Domain Guide\n\nSteps.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 2);
        let paths: Vec<_> = docs.iter().map(|d| d.path.to_str().unwrap()).collect();
        assert!(paths.contains(&"domain/index.md"));
        assert!(paths.contains(&"domain/guide.md"));
    }

    #[test]
    fn test_scan_extracts_title_from_h1() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("guide.md"),
            "# My Custom Title\n\nContent.",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "My Custom Title");
    }

    #[test]
    fn test_scan_falls_back_to_filename() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("setup-guide.md"),
            "Content without heading.",
        )
        .unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "Setup Guide");
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden").unwrap();
        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, PathBuf::from("visible.md"));
    }

    #[test]
    fn test_scan_skips_underscore_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("_partial.md"), "# Partial").unwrap();
        fs::write(temp_dir.path().join("main.md"), "# Main").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, PathBuf::from("main.md"));
    }

    #[test]
    fn test_scan_skips_node_modules() {
        let temp_dir = create_test_dir();
        let node_modules = temp_dir.path().join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("package.md"), "# Package").unwrap();
        fs::write(temp_dir.path().join("main.md"), "# Main").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let docs = storage.scan().unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, PathBuf::from("main.md"));
    }

    #[test]
    fn test_read_existing_file() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide\n\nContent here.").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let content = storage.read(Path::new("guide.md")).unwrap();

        assert_eq!(content, "# Guide\n\nContent here.");
    }

    #[test]
    fn test_read_nested_file() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("guide.md"), "# Domain Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let content = storage.read(Path::new("domain/guide.md")).unwrap();

        assert_eq!(content, "# Domain Guide");
    }

    #[test]
    fn test_read_missing_file() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.read(Path::new("nonexistent.md"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.backend(), Some("Fs"));
    }

    #[test]
    fn test_exists_returns_true_for_existing_file() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        assert!(storage.exists(Path::new("guide.md")));
    }

    #[test]
    fn test_exists_returns_false_for_missing_file() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        assert!(!storage.exists(Path::new("nonexistent.md")));
    }

    #[test]
    fn test_exists_returns_true_for_directory() {
        let temp_dir = create_test_dir();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        assert!(storage.exists(Path::new("subdir")));
    }

    #[test]
    fn test_mtime_cache_reuses_titles() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // First scan
        let docs1 = storage.scan().unwrap();
        assert_eq!(docs1[0].title, "Original Title");

        // Second scan without changes - should use cache
        let docs2 = storage.scan().unwrap();
        assert_eq!(docs2[0].title, "Original Title");
    }

    #[test]
    fn test_mtime_cache_detects_changes() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Original Title").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());

        // First scan
        let docs1 = storage.scan().unwrap();
        assert_eq!(docs1[0].title, "Original Title");

        // Small delay to ensure mtime changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify file
        fs::write(temp_dir.path().join("guide.md"), "# Updated Title").unwrap();

        // Second scan should see new title
        let docs2 = storage.scan().unwrap();
        assert_eq!(docs2[0].title, "Updated Title");
    }

    #[test]
    fn test_title_from_filename() {
        assert_eq!(
            FsStorage::title_from_filename("setup-guide.md"),
            "Setup Guide"
        );
        assert_eq!(FsStorage::title_from_filename("my_page.md"), "My Page");
        assert_eq!(
            FsStorage::title_from_filename("complex-name_here.md"),
            "Complex Name Here"
        );
        assert_eq!(FsStorage::title_from_filename("simple.md"), "Simple");
    }

    #[test]
    fn test_mtime_returns_modification_time() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let mtime = storage.mtime(Path::new("guide.md")).unwrap();

        // mtime should be a recent timestamp (within last minute)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert!(mtime > now - 60.0);
        assert!(mtime <= now);
    }

    #[test]
    fn test_mtime_missing_file() {
        let temp_dir = create_test_dir();

        let storage = FsStorage::new(temp_dir.path().to_path_buf());
        let result = storage.mtime(Path::new("nonexistent.md"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::NotFound);
        assert_eq!(err.backend(), Some("Fs"));
    }
}
