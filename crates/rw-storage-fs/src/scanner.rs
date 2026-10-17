//! Document discovery by filesystem walking.
//!
//! Scanning only finds the url paths that may have a page. Which files serve
//! each one is [`PathResolver`]'s call, the same one `read` and the watch path
//! make, so `FsStorage` locates the sources when it builds the page.

use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::source::{Classification, PathResolver};

/// What one walk of the source tree found.
pub(crate) struct Listing {
    /// Every url path that may have a page. Always includes the root: its
    /// content may live outside `source_dir` (the README homepage), where the
    /// walk does not look. A url whose sources form no page is dropped when it
    /// is built.
    pub url_paths: Vec<String>,
    source_dir: PathBuf,
    /// Every source file the walk classified.
    files: HashSet<PathBuf>,
}

impl Listing {
    /// Whether `path` exists, for the resolver to probe instead of the disk,
    /// so resolving every page costs no syscalls.
    ///
    /// Unlike the disk, it never finds what the walk skips (hidden files,
    /// symlinks). Paths outside `source_dir` (the README) are checked on disk.
    pub(crate) fn exists(&self, path: &Path) -> bool {
        if path.starts_with(&self.source_dir) {
            self.files.contains(path)
        } else {
            path.exists()
        }
    }
}

/// Walk `resolver`'s source tree.
pub(crate) fn scan(resolver: &PathResolver) -> Listing {
    let (mut urls, files) = if resolver.source_dir().exists() {
        collect_sources(resolver)
    } else {
        (HashSet::new(), HashSet::new())
    };
    urls.insert(String::new());
    Listing {
        url_paths: urls.into_iter().collect(),
        source_dir: resolver.source_dir().to_path_buf(),
        files,
    }
}

/// Walk the tree in parallel, classifying every file into its url path.
///
/// Hidden files and directories are skipped, as are symlinks.
fn collect_sources(resolver: &PathResolver) -> (HashSet<String>, HashSet<PathBuf>) {
    // Pushed under the lock, indexed after the walk, so the threads only
    // contend on a push.
    let sources: Mutex<Vec<(Classification, PathBuf)>> = Mutex::new(Vec::new());

    WalkBuilder::new(resolver.source_dir())
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
            let sources = &sources;

            Box::new(move |result| {
                let Ok(entry) = result else {
                    return ignore::WalkState::Continue;
                };

                if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                    return ignore::WalkState::Continue;
                }

                let Ok(rel_path) = entry.path().strip_prefix(resolver.source_dir()) else {
                    return ignore::WalkState::Continue;
                };
                let filename = entry.file_name().to_string_lossy();
                if let Some(classification) = resolver.classify_relpath(rel_path, &filename) {
                    sources.lock().push((classification, entry.into_path()));
                }

                ignore::WalkState::Continue
            })
        });

    let sources = sources.into_inner();
    // Per url: how many content and metadata files map to it.
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    let mut files = HashSet::with_capacity(sources.len());
    for (classification, path) in sources {
        match classification {
            Classification::Content { url_path } => counts.entry(url_path).or_default().0 += 1,
            Classification::Metadata { url_path } => counts.entry(url_path).or_default().1 += 1,
        }
        files.insert(path);
    }

    let urls = counts
        .into_iter()
        .map(|(url_path, (content, meta))| {
            if content > 1 {
                tracing::warn!(
                    url_path = %url_path,
                    "Multiple content files for same url_path, preferring index.md"
                );
            }
            if meta > 1 {
                tracing::warn!(
                    url_path = %url_path,
                    "Multiple metadata files for same url_path, using highest precedence"
                );
            }
            url_path
        })
        .collect();
    (urls, files)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn scanned_urls(source_dir: &Path, meta_filename: &str) -> Vec<String> {
        let mut urls = scan(&PathResolver::new(
            source_dir,
            source_dir.to_path_buf(),
            meta_filename,
        ))
        .url_paths;
        urls.sort();
        urls
    }

    #[test]
    fn test_scan_finds_md_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join("guide.md"), "# Guide").unwrap();

        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();

        assert_eq!(
            scanned_urls(temp_dir.path(), "meta.yaml"),
            ["", "domain", "guide"]
        );
    }

    #[test]
    fn test_scan_finds_virtual_pages() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("meta.yaml"), "title: Domain").unwrap();

        assert_eq!(scanned_urls(temp_dir.path(), "meta.yaml"), ["", "domain"]);
    }

    #[test]
    fn test_scan_combines_md_and_meta() {
        let temp_dir = create_test_dir();
        let domain_dir = temp_dir.path().join("domain");
        fs::create_dir(&domain_dir).unwrap();
        fs::write(domain_dir.join("index.md"), "# Domain").unwrap();
        fs::write(domain_dir.join("meta.yaml"), "kind: section").unwrap();

        assert_eq!(scanned_urls(temp_dir.path(), "meta.yaml"), ["", "domain"]);
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let temp_dir = create_test_dir();
        fs::write(temp_dir.path().join(".hidden.md"), "# Hidden").unwrap();
        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        assert_eq!(scanned_urls(temp_dir.path(), "meta.yaml"), ["", "visible"]);
    }

    #[test]
    fn test_scan_empty_dir_still_offers_the_root() {
        let temp_dir = create_test_dir();

        assert_eq!(scanned_urls(temp_dir.path(), "meta.yaml"), [""]);
    }

    #[test]
    fn test_scan_missing_dir_still_offers_the_root() {
        assert_eq!(scanned_urls(Path::new("/nonexistent"), "meta.yaml"), [""]);
    }

    #[test]
    fn test_scan_nested_structure() {
        let temp_dir = create_test_dir();

        fs::write(temp_dir.path().join("index.md"), "# Home").unwrap();

        let l1 = temp_dir.path().join("level1");
        fs::create_dir(&l1).unwrap();
        fs::write(l1.join("index.md"), "# L1").unwrap();
        fs::write(l1.join("guide.md"), "# L1 Guide").unwrap();

        let l2 = l1.join("level2");
        fs::create_dir(&l2).unwrap();
        fs::write(l2.join("index.md"), "# L2").unwrap();

        assert_eq!(
            scanned_urls(temp_dir.path(), "meta.yaml"),
            ["", "level1", "level1/guide", "level1/level2"]
        );
    }

    #[test]
    fn test_scan_with_custom_meta_filename() {
        let temp_dir = create_test_dir();
        fs::create_dir(temp_dir.path().join("custom")).unwrap();
        fs::write(temp_dir.path().join("custom/config.yml"), "kind: section").unwrap();
        fs::create_dir(temp_dir.path().join("default")).unwrap();
        fs::write(temp_dir.path().join("default/meta.yaml"), "ignored").unwrap();

        assert_eq!(scanned_urls(temp_dir.path(), "config.yml"), ["", "custom"]);
    }

    #[test]
    fn test_scan_skips_hidden_directories() {
        let temp_dir = create_test_dir();
        let hidden_dir = temp_dir.path().join(".hidden");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("secret.md"), "# Secret").unwrap();

        fs::write(temp_dir.path().join("visible.md"), "# Visible").unwrap();

        assert_eq!(scanned_urls(temp_dir.path(), "meta.yaml"), ["", "visible"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_scan_skips_symlinks() {
        use std::os::unix::fs::symlink;

        let temp_dir = create_test_dir();

        fs::write(temp_dir.path().join("real.md"), "# Real").unwrap();
        symlink(
            temp_dir.path().join("real.md"),
            temp_dir.path().join("link.md"),
        )
        .unwrap();

        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("doc.md"), "# Doc").unwrap();
        symlink(&subdir, temp_dir.path().join("link_dir")).unwrap();

        assert_eq!(
            scanned_urls(temp_dir.path(), "meta.yaml"),
            ["", "real", "subdir/doc"]
        );
    }

    #[test]
    fn test_scan_deeply_nested() {
        let temp_dir = create_test_dir();
        let deep = temp_dir.path().join("a").join("b").join("c").join("d");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep.md"), "# Deep").unwrap();

        assert_eq!(
            scanned_urls(temp_dir.path(), "meta.yaml"),
            ["", "a/b/c/d/deep"]
        );
    }

    #[test]
    fn test_scan_content_less_sidecar() {
        let temp_dir = create_test_dir();
        fs::write(
            temp_dir.path().join("payments.meta.yaml"),
            "kind: component",
        )
        .unwrap();

        assert_eq!(scanned_urls(temp_dir.path(), "meta.yaml"), ["", "payments"]);
    }

    #[test]
    fn test_scan_index_variant_alone_is_directory_meta() {
        let temp_dir = create_test_dir();
        let dir = temp_dir.path().join("dir");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("index.meta.yaml"), "kind: domain").unwrap();

        assert_eq!(scanned_urls(temp_dir.path(), "meta.yaml"), ["", "dir"]);
    }

    #[test]
    fn test_scan_directory_named_index() {
        let temp_dir = create_test_dir();
        let index_dir = temp_dir.path().join("index");
        fs::create_dir(&index_dir).unwrap();
        fs::write(index_dir.join("index.md"), "# Index Dir").unwrap();

        assert_eq!(scanned_urls(temp_dir.path(), "meta.yaml"), ["", "index"]);
    }
}
