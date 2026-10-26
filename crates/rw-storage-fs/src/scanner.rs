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

use crate::source::{
    Classification, INDEX_META_FILENAME, META_FILENAME, PathResolver, classify_relpath,
    dir_url_path, exists_on_disk,
};

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
    /// Source files the site leaves out or that need renaming.
    pub warnings: Vec<String>,
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
    let (mut urls, files, warnings) = if resolver.source_dir().exists() {
        collect_sources(resolver)
    } else {
        (HashSet::new(), HashSet::new(), Vec::new())
    };
    urls.insert(String::new());
    Listing {
        url_paths: urls.into_iter().collect(),
        source_dir: resolver.source_dir().to_path_buf(),
        files,
        warnings,
    }
}

/// Walk the tree in parallel, classifying every file into its url path.
///
/// Hidden files and directories are skipped, as are symlinks.
fn collect_sources(resolver: &PathResolver) -> (HashSet<String>, HashSet<PathBuf>, Vec<String>) {
    // Pushed under the lock, indexed after the walk, so the threads only
    // contend on a push.
    let sources: Mutex<Vec<(Classification, PathBuf)>> = Mutex::new(Vec::new());
    let walked: Mutex<Vec<String>> = Mutex::new(Vec::new());

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
            let walked = &walked;

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
                walked.lock().push(dir_url_path(rel_path));
                let filename = entry.file_name().to_string_lossy();
                if let Some(classification) = classify_relpath(rel_path, &filename) {
                    sources.lock().push((classification, entry.into_path()));
                }

                ignore::WalkState::Continue
            })
        });

    let sources = sources.into_inner();
    // Per url: its content files and its metadata files.
    let mut by_url: HashMap<String, (Vec<PathBuf>, Vec<PathBuf>)> = HashMap::new();
    let mut files = HashSet::with_capacity(sources.len());
    for (classification, path) in sources {
        match classification {
            Classification::Content { url_path } => {
                by_url.entry(url_path).or_default().0.push(path.clone());
            }
            Classification::Metadata { url_path } => {
                by_url.entry(url_path).or_default().1.push(path.clone());
            }
        }
        files.insert(path);
    }

    let exists = |path: &Path| files.contains(path);
    let mut warnings = Vec::new();
    let mut urls = HashSet::with_capacity(by_url.len());
    for (url_path, (content, meta)) in by_url {
        if content.len() > 1
            && let Some(served) = resolver.resolve_content_with(&url_path, exists)
        {
            for excluded in content.iter().filter(|path| **path != served) {
                warnings.push(format!(
                    "Excluding {} from the site because it conflicts with {}",
                    resolver.display_path(excluded),
                    resolver.display_path(&served),
                ));
            }
        }
        let used = meta
            .first()
            .and_then(|_| resolver.resolve_meta_with(&url_path, exists));
        for path in &meta {
            if let Some(used) = used.as_ref().filter(|used| *used != path) {
                warnings.push(format!(
                    "Ignoring {} because {} takes precedence",
                    resolver.display_path(path),
                    resolver.display_path(used),
                ));
            } else if path.file_name() == Some(INDEX_META_FILENAME.as_ref()) {
                warnings.push(format!(
                    "{}: `{INDEX_META_FILENAME}` is treated as directory metadata; \
                     rename it to `{META_FILENAME}`",
                    resolver.display_path(path),
                ));
            }
        }
        urls.insert(url_path);
    }
    let walked = walked.into_inner();
    warnings.extend(
        case_twins(walked.iter().map(String::as_str))
            .iter()
            .map(|group| case_twin_warning(resolver, group)),
    );
    (urls, files, warnings)
}

fn case_twin_warning(resolver: &PathResolver, group: &[&str]) -> String {
    let shown: Vec<String> = group
        .iter()
        .map(|rel| {
            let path = resolver.display_path(&resolver.source_dir().join(rel));
            if rel.ends_with('/') {
                format!("{path}/")
            } else {
                path
            }
        })
        .collect();
    let (last, rest) = shown.split_last().expect("a group has two or more paths");
    let effect = if group[0].ends_with('/') {
        "merges them into one folder"
    } else {
        "keeps only one"
    };
    format!(
        "{} and {last} differ only in case; a macOS or Windows checkout {effect}",
        rest.join(", "),
    )
}

/// The groups of `files` (`/`-separated, relative) and of their folders
/// (spelled with a trailing `/`) whose paths differ only in case, each sorted.
/// A group counts only if its spellings differ in their last name;
/// differences above it are their folders' groups.
fn case_twins<'a>(files: impl IntoIterator<Item = &'a str>) -> Vec<Vec<&'a str>> {
    let mut paths = HashSet::new();
    for file in files {
        paths.insert(file);
        for (end, _) in file.rmatch_indices('/') {
            if !paths.insert(&file[..=end]) {
                break; // seen before, so the folders above it are in too
            }
        }
    }
    // Real filesystems fold case in their own ways; `to_lowercase` is close
    // enough for a warning.
    let mut by_key: HashMap<String, Vec<&str>> = HashMap::new();
    for path in paths {
        by_key.entry(path.to_lowercase()).or_default().push(path);
    }
    by_key.retain(|_, spellings| spellings.len() > 1);

    let mut groups: Vec<(&str, Vec<&str>)> = by_key
        .iter()
        .filter(|(_, spellings)| {
            let names: HashSet<_> = spellings
                .iter()
                .map(|spelling| spelling.trim_end_matches('/').rsplit('/').next())
                .collect();
            names.len() > 1
        })
        .map(|(key, spellings)| {
            let mut spellings = spellings.clone();
            spellings.sort_unstable();
            (key.as_str(), spellings)
        })
        .collect();
    groups.sort_unstable_by_key(|(key, _)| *key);
    groups.into_iter().map(|(_, spellings)| spellings).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn scanned_urls(source_dir: &Path) -> Vec<String> {
        let mut urls = scan(&PathResolver::new(source_dir, source_dir.to_path_buf())).url_paths;
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

        assert_eq!(scanned_urls(temp_dir.path()), ["", "visible"]);
    }

    #[test]
    fn scan_skips_underscore_files_and_folders() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        for (path, content) in [
            ("_partials/warning.md", "# Warning"),
            ("_partials/index.md", "# Partials"),
            ("_partials/meta.yaml", "title: Partials"),
            ("_draft.meta.yaml", "title: Draft"),
            ("guide/index.md", "# Guide"),
            ("guide/_note.md", "# Note"),
            ("guide/_old/faq.md", "# FAQ"),
            ("snake_case.md", "# Snake"),
        ] {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }

        assert_eq!(scanned_urls(root), ["", "guide", "snake_case"]);
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

        assert_eq!(scanned_urls(temp_dir.path()), ["", "real", "subdir/doc"]);
    }

    #[test]
    fn test_scan_deeply_nested() {
        let temp_dir = create_test_dir();
        let deep = temp_dir.path().join("a").join("b").join("c").join("d");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep.md"), "# Deep").unwrap();

        assert_eq!(scanned_urls(temp_dir.path()), ["", "a/b/c/d/deep"]);
    }

    #[test]
    fn test_scan_directory_named_index() {
        let temp_dir = create_test_dir();
        let index_dir = temp_dir.path().join("index");
        fs::create_dir(&index_dir).unwrap();
        fs::write(index_dir.join("index.md"), "# Index Dir").unwrap();

        assert_eq!(scanned_urls(temp_dir.path()), ["", "index"]);
    }

    #[test]
    fn case_twins_groups_paths_that_differ_only_in_case() {
        let files = [
            "Guide.md",
            "guide.md",
            "GUIDE.md",
            "img/Logo.png",
            "img/logo.PNG",
            "guide/index.md",
            "API/a.md",
            "api/b.md",
            "API/c.md",
            "api/c.md",
            "api/Guide.md",
            "api/guide.md",
            "API/d.md",
            "api/D.md",
            "_partials/Note.md",
            "_partials/note.md",
        ];

        assert_eq!(
            case_twins(files),
            [
                vec!["_partials/Note.md", "_partials/note.md"],
                vec!["API/", "api/"],
                vec!["API/d.md", "api/D.md"],
                vec!["api/Guide.md", "api/guide.md"],
                vec!["GUIDE.md", "Guide.md", "guide.md"],
                vec!["img/Logo.png", "img/logo.PNG"],
            ]
        );
    }

    #[test]
    fn scan_warns_about_case_twins() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        let docs = root.join("docs");
        fs::create_dir_all(docs.join("API")).unwrap();
        fs::write(docs.join("API/index.md"), "# API").unwrap();
        fs::write(docs.join("Guide.md"), "# Guide").unwrap();
        if docs.join("api").exists() || docs.join("guide.md").exists() {
            return; // A case-insensitive filesystem holds no twins.
        }
        fs::create_dir_all(docs.join("api")).unwrap();
        fs::write(docs.join("api/index.md"), "# api").unwrap();
        fs::write(docs.join("guide.md"), "# guide").unwrap();

        let mut warnings = scan(&PathResolver::new(root, docs)).warnings;
        warnings.sort();

        assert_eq!(
            warnings,
            [
                "docs/API/ and docs/api/ differ only in case; \
                 a macOS or Windows checkout merges them into one folder",
                "docs/Guide.md and docs/guide.md differ only in case; \
                 a macOS or Windows checkout keeps only one",
            ]
        );
    }

    #[test]
    fn test_scan_warns_about_shadowed_and_misnamed_metadata() {
        let temp_dir = create_test_dir();
        let root = temp_dir.path();
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/meta.yaml"), "title: A").unwrap();
        fs::write(root.join("a.meta.yaml"), "title: Sibling").unwrap();
        fs::write(root.join("a/index.meta.yaml"), "title: Index").unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        fs::write(root.join("b/index.meta.yaml"), "title: B").unwrap();

        let mut warnings = scan(&PathResolver::new(root, root.to_path_buf())).warnings;
        warnings.sort();

        assert_eq!(
            warnings,
            [
                "Ignoring a.meta.yaml because a/meta.yaml takes precedence",
                "Ignoring a/index.meta.yaml because a/meta.yaml takes precedence",
                "b/index.meta.yaml: `index.meta.yaml` is treated as directory metadata; \
                 rename it to `meta.yaml`",
            ]
        );
    }
}
