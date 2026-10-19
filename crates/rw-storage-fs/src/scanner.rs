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

use crate::source::{Classification, PathResolver, exists_on_disk};

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
            exists_on_disk(path)
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
    // Per url: its content files, and how many metadata files map to it.
    let mut by_url: HashMap<String, (Vec<PathBuf>, usize)> = HashMap::new();
    let mut files = HashSet::with_capacity(sources.len());
    for (classification, path) in sources {
        match classification {
            Classification::Content { url_path } => {
                by_url.entry(url_path).or_default().0.push(path.clone());
            }
            Classification::Metadata { url_path } => by_url.entry(url_path).or_default().1 += 1,
        }
        files.insert(path);
    }

    let urls = by_url
        .into_iter()
        .map(|(url_path, (content, meta))| {
            if content.len() > 1
                && let Some(served) =
                    resolver.resolve_content_with(&url_path, |path| files.contains(path))
            {
                let rel = |path: &Path| {
                    path.strip_prefix(resolver.source_dir())
                        .unwrap_or(path)
                        .display()
                        .to_string()
                };
                for excluded in content.iter().filter(|path| **path != served) {
                    tracing::warn!(
                        "Excluding {} from the site because it conflicts with {}",
                        rel(excluded),
                        rel(&served),
                    );
                }
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
    fn test_scan_directory_named_index() {
        let temp_dir = create_test_dir();
        let index_dir = temp_dir.path().join("index");
        fs::create_dir(&index_dir).unwrap();
        fs::write(index_dir.join("index.md"), "# Index Dir").unwrap();

        assert_eq!(scanned_urls(temp_dir.path(), "meta.yaml"), ["", "index"]);
    }
}
