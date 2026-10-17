//! Document discovery by filesystem walking.
//!
//! Scanning only finds the url paths that may have a page. Which files serve
//! each one is [`PathResolver`]'s call, the same one `read` and the watch path
//! make, so `FsStorage` locates the sources when it builds the page.

use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;

use ignore::WalkBuilder;

use crate::source::{Classification, PathResolver};

/// Every url path that may have a page under `resolver`'s source tree.
///
/// Always includes the root: its content may live outside `source_dir` (the
/// README homepage), where the walk does not look. A url whose sources turn out
/// to form no page is dropped when it is built.
pub(crate) fn scan(resolver: &PathResolver) -> Vec<String> {
    let mut urls = if resolver.source_dir().exists() {
        collect_url_paths(resolver)
    } else {
        HashSet::new()
    };
    urls.insert(String::new());
    urls.into_iter().collect()
}

/// Walk the tree in parallel and classify every file into its url path.
///
/// Hidden files and directories are skipped, as are symlinks.
fn collect_url_paths(resolver: &PathResolver) -> HashSet<String> {
    // Per url: how many content and metadata files map to it.
    let counts: Mutex<HashMap<String, (usize, usize)>> = Mutex::new(HashMap::new());

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
            let counts = &counts;

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
                match resolver.classify_relpath(rel_path, &filename) {
                    Some(Classification::Content { url_path }) => {
                        counts.lock().entry(url_path).or_default().0 += 1;
                    }
                    Some(Classification::Metadata { url_path }) => {
                        counts.lock().entry(url_path).or_default().1 += 1;
                    }
                    None => {}
                }

                ignore::WalkState::Continue
            })
        });

    counts
        .into_inner()
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
        .collect()
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
        ));
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
