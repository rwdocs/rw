//! Document discovery by filesystem walking.
//!
//! This module separates the discovery phase (finding files) from the building
//! phase (creating Documents). The Scanner only identifies files that could
//! form documents, returning lightweight references for `FsStorage` to process.

use std::fs;
use std::path::{Path, PathBuf};

/// Reference to a document's source files.
///
/// Contains only file locations - no content is read at this stage.
/// `FsStorage`'s `build_document` method converts these to full `Document` structs.
#[derive(Debug, Clone)]
pub(crate) struct DocumentRef {
    /// URL path (e.g., "", "domain", "domain/guide")
    pub url_path: String,
    /// Path to content file (.md), if present
    pub content_path: Option<PathBuf>,
    /// Path to metadata file (e.g., meta.yaml), if present
    pub meta_path: Option<PathBuf>,
}

/// Discovers document references by walking the filesystem.
///
/// The Scanner performs Phase 1 of document loading:
/// 1. Walk directory tree
/// 2. Identify .md files and metadata files
/// 3. Return `DocumentRef` for each potential document
///
/// Phase 2 (building Documents) is handled by `FsStorage`.
pub(crate) struct Scanner {
    source_dir: PathBuf,
    meta_filename: String,
}

impl Scanner {
    /// Create a new Scanner.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Root directory to scan
    /// * `meta_filename` - Name of metadata files (e.g., "meta.yaml")
    pub fn new(source_dir: PathBuf, meta_filename: String) -> Self {
        Self {
            source_dir,
            meta_filename,
        }
    }

    /// Scan filesystem and return document references.
    ///
    /// Returns an empty Vec if the source directory doesn't exist.
    pub fn scan(&self) -> Vec<DocumentRef> {
        let mut refs = Vec::new();
        if self.source_dir.exists() {
            self.scan_directory(&self.source_dir, "", &mut refs);
        }
        refs
    }

    /// Scan a directory and collect document references.
    ///
    /// This method:
    /// 1. Collects all .md files (except index.md) as standalone documents
    /// 2. Handles index.md + meta.yaml combinations
    /// 3. Creates virtual page refs for meta.yaml without index.md
    /// 4. Recurses into subdirectories
    fn scan_directory(&self, dir_path: &Path, url_prefix: &str, refs: &mut Vec<DocumentRef>) {
        let Ok(entries) = fs::read_dir(dir_path) else {
            return;
        };

        // Collect entries with cached file_type to avoid repeated stat calls
        let entries: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|e| {
                let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
                let name_lower = e.file_name().to_string_lossy().to_lowercase();
                (e, is_dir, name_lower)
            })
            .collect();

        // Track special files in this directory
        let mut index_md_path: Option<PathBuf> = None;
        let mut meta_path: Option<PathBuf> = None;

        for (entry, is_dir, name_lower) in &entries {
            // Skip hidden files/dirs
            if name_lower.starts_with('.') {
                continue;
            }

            let path = entry.path();

            if *is_dir {
                // Recurse into subdirectory
                let child_name = entry.file_name().to_string_lossy().into_owned();
                let child_url = if url_prefix.is_empty() {
                    child_name
                } else {
                    format!("{url_prefix}/{child_name}")
                };
                self.scan_directory(&path, &child_url, refs);
            } else if path.extension().is_some_and(|e| e == "md") {
                if name_lower == "index.md" {
                    index_md_path = Some(path);
                } else {
                    // Standalone .md file - create DocumentRef immediately
                    let url_path = file_path_to_url(Path::new(&entry.file_name()), url_prefix);
                    refs.push(DocumentRef {
                        url_path,
                        content_path: Some(path),
                        meta_path: None,
                    });
                }
            } else if entry.file_name().to_string_lossy() == self.meta_filename {
                meta_path = Some(path);
            }
        }

        // Handle index.md and/or meta.yaml at this directory level
        match (&index_md_path, &meta_path) {
            (Some(_), _) | (None, Some(_)) => {
                refs.push(DocumentRef {
                    url_path: url_prefix.to_string(),
                    content_path: index_md_path,
                    meta_path,
                });
            }
            (None, None) => {
                // No index.md or meta.yaml - no document at this level
            }
        }
    }
}

/// Convert file path to URL path with optional base prefix.
///
/// Examples (with empty base):
/// - `index.md` -> `""`
/// - `guide.md` -> `"guide"`
/// - `domain/index.md` -> `"domain"`
/// - `domain/setup.md` -> `"domain/setup"`
///
/// Examples (with base):
/// - `index.md`, base `"domain"` -> `"domain"`
/// - `guide.md`, base `"domain"` -> `"domain/guide"`
pub(crate) fn file_path_to_url(rel_path: &Path, base: &str) -> String {
    let path_str = rel_path.to_string_lossy();

    // Remove .md extension
    let without_ext = path_str.strip_suffix(".md").unwrap_or(&path_str);

    // Handle index files
    let path_part = if without_ext == "index" {
        ""
    } else if let Some(without_index) = without_ext.strip_suffix("/index") {
        without_index
    } else {
        without_ext
    };

    // Combine with base
    match (base.is_empty(), path_part.is_empty()) {
        (true, _) => path_part.to_string(),
        (false, true) => base.to_string(),
        (false, false) => format!("{base}/{path_part}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_file_path_to_url() {
        // Without base
        assert_eq!(file_path_to_url(Path::new("index.md"), ""), "");
        assert_eq!(file_path_to_url(Path::new("guide.md"), ""), "guide");
        assert_eq!(file_path_to_url(Path::new("domain/index.md"), ""), "domain");
        assert_eq!(
            file_path_to_url(Path::new("domain/setup.md"), ""),
            "domain/setup"
        );
        assert_eq!(file_path_to_url(Path::new("a/b/c.md"), ""), "a/b/c");
        assert_eq!(file_path_to_url(Path::new("index/index.md"), ""), "index");

        // With base
        assert_eq!(file_path_to_url(Path::new("index.md"), "domain"), "domain");
        assert_eq!(
            file_path_to_url(Path::new("guide.md"), "domain"),
            "domain/guide"
        );
        assert_eq!(file_path_to_url(Path::new("setup.md"), "a/b"), "a/b/setup");
    }

    #[test]
    fn test_scan_finds_md_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();

        let scanner = Scanner::new(temp_dir.path().to_path_buf(), "meta.yaml".to_string());
        let refs = scanner.scan();

        assert_eq!(refs.len(), 2);

        let guide_ref = refs.iter().find(|r| r.url_path == "guide").unwrap();
        assert!(guide_ref
            .content_path
            .as_ref()
            .unwrap()
            .ends_with("guide.md"));
        assert!(guide_ref.meta_path.is_none());

        let domain_ref = refs.iter().find(|r| r.url_path == "domain").unwrap();
        assert!(domain_ref
            .content_path
            .as_ref()
            .unwrap()
            .ends_with("index.md"));
        assert!(domain_ref.meta_path.is_none());
    }

    #[test]
    fn test_scan_finds_virtual_pages() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        // No index.md, only meta.yaml
        fs::write(domain_dir.join("meta.yaml"), "title: Domain").unwrap();

        let scanner = Scanner::new(temp_dir.path().to_path_buf(), "meta.yaml".to_string());
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].url_path, "domain");
        assert!(refs[0].content_path.is_none());
        assert!(refs[0].meta_path.as_ref().unwrap().ends_with("meta.yaml"));
    }

    #[test]
    fn test_scan_combines_md_and_meta() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("meta.yaml"), "type: section").unwrap();

        let scanner = Scanner::new(temp_dir.path().to_path_buf(), "meta.yaml".to_string());
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].url_path, "domain");
        assert!(refs[0]
            .content_path
            .as_ref()
            .unwrap()
            .ends_with("index.md"));
        assert!(refs[0].meta_path.as_ref().unwrap().ends_with("meta.yaml"));
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden").unwrap();
        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        let scanner = Scanner::new(temp_dir.path().to_path_buf(), "meta.yaml".to_string());
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].url_path, "visible");
    }

    #[test]
    fn test_scan_empty_dir() {
        let temp_dir = create_test_dir();

        let scanner = Scanner::new(temp_dir.path().to_path_buf(), "meta.yaml".to_string());
        let refs = scanner.scan();

        assert!(refs.is_empty());
    }

    #[test]
    fn test_scan_missing_dir() {
        let scanner = Scanner::new(PathBuf::from("/nonexistent"), "meta.yaml".to_string());
        let refs = scanner.scan();

        assert!(refs.is_empty());
    }

    #[test]
    fn test_scan_nested_structure() {
        let temp_dir = create_test_dir();

        // Root index
        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();

        // Level 1
        let l1 = temp_dir.path().join("level1");
        fs::create_dir(&l1).unwrap();
        fs::write(l1.join("index.md"), "# L1").unwrap();
        fs::write(l1.join("guide.md"), "# L1 Guide").unwrap();

        // Level 2
        let l2 = l1.join("level2");
        fs::create_dir(&l2).unwrap();
        fs::write(l2.join("index.md"), "# L2").unwrap();

        let scanner = Scanner::new(temp_dir.path().to_path_buf(), "meta.yaml".to_string());
        let refs = scanner.scan();

        assert_eq!(refs.len(), 4);

        let paths: Vec<_> = refs.iter().map(|r| r.url_path.as_str()).collect();
        assert!(paths.contains(&""));
        assert!(paths.contains(&"level1"));
        assert!(paths.contains(&"level1/guide"));
        assert!(paths.contains(&"level1/level2"));
    }

    #[test]
    fn test_scan_with_custom_meta_filename() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("config.yml"), "type: section").unwrap();
        fs::write(domain_dir.join("meta.yaml"), "ignored").unwrap();

        let scanner = Scanner::new(temp_dir.path().to_path_buf(), "config.yml".to_string());
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        // Should include config.yml, not meta.yaml
        assert!(refs[0]
            .content_path
            .as_ref()
            .unwrap()
            .ends_with("index.md"));
        assert!(refs[0].meta_path.as_ref().unwrap().ends_with("config.yml"));
    }
}
