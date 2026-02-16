//! Document discovery by filesystem walking.
//!
//! This module separates the discovery phase (finding files) from the building
//! phase (creating Documents). The Scanner only identifies files that could
//! form documents, returning lightweight references for `FsStorage` to process.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ignore::WalkBuilder;

use crate::source::{SourceFile, SourceKind};

/// Reference to a document's source files.
///
/// Contains only file locations - no content is read at this stage.
/// `FsStorage`'s `build_document` method converts these to full `Document` structs.
#[derive(Debug)]
#[allow(clippy::struct_field_names)]
pub(crate) struct DocumentRef {
    /// URL path (e.g., "", "domain", "domain/guide")
    pub url_path: String,
    /// Path to content file (.md), if present
    pub content_path: Option<PathBuf>,
    /// Path to metadata file (e.g., "meta.yaml"), if present
    pub meta_path: Option<PathBuf>,
}

/// Discovers document references by walking the filesystem.
///
/// The Scanner performs Phase 1 of document loading:
/// 1. Walk directory tree in parallel using the `ignore` crate
/// 2. Classify files using `SourceFile`
/// 3. Group files by `url_path` into `DocumentRef`s
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
    pub fn new(source_dir: &Path, meta_filename: &str) -> Self {
        Self {
            source_dir: source_dir.to_path_buf(),
            meta_filename: meta_filename.to_owned(),
        }
    }

    /// Scan filesystem and return document references.
    ///
    /// Returns an empty Vec if the source directory doesn't exist.
    pub fn scan(&self) -> Vec<DocumentRef> {
        if !self.source_dir.exists() {
            return Vec::new();
        }
        let files = self.collect_source_files();
        Self::group_into_documents(files)
    }

    /// Walk directory tree in parallel and collect all source files.
    ///
    /// Uses the `ignore` crate's parallel walker which distributes directory
    /// traversal across multiple threads with work-stealing. Hidden files
    /// and hidden directories are skipped automatically.
    fn collect_source_files(&self) -> Vec<SourceFile> {
        let files: Mutex<Vec<SourceFile>> = Mutex::new(Vec::new());

        WalkBuilder::new(&self.source_dir)
            .hidden(true)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .follow_links(false)
            .threads(
                std::thread::available_parallelism()
                    .map_or(1, NonZeroUsize::get)
                    .min(12),
            )
            .build_parallel()
            .run(|| {
                let files = &files;
                let source_dir = &self.source_dir;
                let meta_filename = &self.meta_filename;

                Box::new(move |result| {
                    let Ok(entry) = result else {
                        return ignore::WalkState::Continue;
                    };

                    // Only process regular files (skip directories, symlinks, etc.)
                    let is_file = entry.file_type().is_some_and(|ft| ft.is_file());
                    if !is_file {
                        return ignore::WalkState::Continue;
                    }

                    let filename = entry.file_name().to_os_string();
                    let path = entry.into_path();

                    if let Some(source) =
                        SourceFile::classify(path, &filename, source_dir, meta_filename)
                    {
                        files.lock().unwrap().push(source);
                    }

                    ignore::WalkState::Continue
                })
            });

        files.into_inner().unwrap()
    }

    /// Group source files into document references by `url_path`.
    fn group_into_documents(files: Vec<SourceFile>) -> Vec<DocumentRef> {
        let mut docs: HashMap<String, DocumentRef> = HashMap::new();

        for file in files {
            let doc = docs
                .entry(file.url_path.clone())
                .or_insert_with(|| DocumentRef {
                    url_path: file.url_path,
                    content_path: None,
                    meta_path: None,
                });

            let (target, kind_name) = match file.kind {
                SourceKind::Content => (&mut doc.content_path, "content"),
                SourceKind::Metadata => (&mut doc.meta_path, "metadata"),
            };

            if target.is_some() {
                tracing::warn!(
                    url_path = %doc.url_path,
                    "Multiple {kind_name} files for same url_path, using last"
                );
            }
            *target = Some(file.path);
        }

        docs.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_scan_finds_md_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
        let refs = scanner.scan();

        assert_eq!(refs.len(), 2);

        let guide_ref = refs.iter().find(|r| r.url_path == "guide").unwrap();
        assert!(
            guide_ref
                .content_path
                .as_ref()
                .unwrap()
                .ends_with("guide.md")
        );
        assert!(guide_ref.meta_path.is_none());

        let domain_ref = refs.iter().find(|r| r.url_path == "domain").unwrap();
        assert!(
            domain_ref
                .content_path
                .as_ref()
                .unwrap()
                .ends_with("index.md")
        );
        assert!(domain_ref.meta_path.is_none());
    }

    #[test]
    fn test_scan_finds_virtual_pages() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        // No index.md, only meta.yaml
        fs::write(domain_dir.join("meta.yaml"), "title: Domain").unwrap();

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
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

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].url_path, "domain");
        assert!(refs[0].content_path.as_ref().unwrap().ends_with("index.md"));
        assert!(refs[0].meta_path.as_ref().unwrap().ends_with("meta.yaml"));
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden").unwrap();
        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].url_path, "visible");
    }

    #[test]
    fn test_scan_empty_dir() {
        let temp_dir = create_test_dir();

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
        let refs = scanner.scan();

        assert!(refs.is_empty());
    }

    #[test]
    fn test_scan_missing_dir() {
        let scanner = Scanner::new(Path::new("/nonexistent"), "meta.yaml");
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

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
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

        let scanner = Scanner::new(temp_dir.path(), "config.yml");
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        // Should include config.yml, not meta.yaml
        assert!(refs[0].content_path.as_ref().unwrap().ends_with("index.md"));
        assert!(refs[0].meta_path.as_ref().unwrap().ends_with("config.yml"));
    }

    #[test]
    fn test_scan_skips_hidden_directories() {
        let temp_dir = create_test_dir();
        let hidden_dir = temp_dir.path().join(".hidden");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("secret.md"), "# Secret").unwrap();

        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].url_path, "visible");
    }

    #[cfg(unix)]
    #[test]
    fn test_scan_skips_symlinks() {
        use std::os::unix::fs::symlink;

        let temp_dir = create_test_dir();

        // Create real file
        fs::write(temp_dir.path().join("real.md"), "# Real").unwrap();

        // Create symlink to file
        symlink(
            temp_dir.path().join("real.md"),
            temp_dir.path().join("link.md"),
        )
        .unwrap();

        // Create directory and symlink to directory
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("doc.md"), "# Doc").unwrap();
        symlink(&subdir, temp_dir.path().join("link_dir")).unwrap();

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
        let refs = scanner.scan();

        // Should find real.md and subdir/doc.md, not link.md or link_dir/*
        assert_eq!(refs.len(), 2);
        let paths: Vec<_> = refs.iter().map(|r| r.url_path.as_str()).collect();
        assert!(paths.contains(&"real"));
        assert!(paths.contains(&"subdir/doc"));
    }

    #[test]
    fn test_scan_deeply_nested() {
        let temp_dir = create_test_dir();
        let deep = temp_dir.path().join("a").join("b").join("c").join("d");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep.md"), "# Deep").unwrap();

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].url_path, "a/b/c/d/deep");
    }

    #[test]
    fn test_scan_root_only_meta() {
        let temp_dir = create_test_dir();
        // Only meta.yaml at root, no index.md
        fs::write(temp_dir.path().join("meta.yaml"), "title: Root").unwrap();

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].url_path, "");
        assert!(refs[0].content_path.is_none());
        assert!(refs[0].meta_path.is_some());
    }

    #[test]
    fn test_scan_directory_named_index() {
        let temp_dir = create_test_dir();
        // Directory named "index" with its own index.md
        let index_dir = temp_dir.path().join("index");
        fs::create_dir(&index_dir).unwrap();
        fs::write(index_dir.join("index.md"), "# Index Dir").unwrap();

        let scanner = Scanner::new(temp_dir.path(), "meta.yaml");
        let refs = scanner.scan();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].url_path, "index");
    }
}
