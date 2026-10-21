//! A page's local images name files by their path in the project, wherever
//! the source directory sits in it.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use rw_cache::{Cache, FileCache, NullCache};
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

fn write(root: &Path, files: &[(&str, &str)]) {
    for (name, contents) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

fn site(project_dir: &Path, source_dir: &Path) -> Site {
    site_with_cache(project_dir, source_dir, Arc::new(NullCache))
}

fn site_with_cache(project_dir: &Path, source_dir: &Path, cache: Arc<dyn Cache>) -> Site {
    let storage = Arc::new(FsStorage::new(
        project_dir.to_path_buf(),
        source_dir.to_path_buf(),
    ));
    Site::new(storage, cache, PageRendererConfig::default())
}

fn files(site: &Site, path: &str) -> Vec<String> {
    let page = site.render(path).unwrap();
    assert_eq!(page.warnings, [] as [String; 0], "{path}");
    page.files.into_iter().collect()
}

#[test]
fn images_on_pages_name_files_under_the_source_dir() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("docs/guide/intro.md", "![a](img/a.png) ![b](../b.png)\n"),
            ("docs/guide/index.md", "![a](img/a.png)\n"),
            ("docs/api.md", "![x](/x.png) ![l](../.github/l.svg)\n"),
            ("docs/guide/img/a.png", ""),
            ("docs/b.png", ""),
            ("x.png", ""),
            (".github/l.svg", ""),
        ],
    );
    let site = site(root, &root.join("docs"));

    assert_eq!(
        files(&site, "guide/intro"),
        ["docs/b.png", "docs/guide/img/a.png"]
    );
    assert_eq!(files(&site, "guide"), ["docs/guide/img/a.png"]);
    assert_eq!(files(&site, "api"), [".github/l.svg", "x.png"]);
}

#[test]
fn images_on_the_readme_homepage_name_files_from_the_project_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "README.md",
                "# Home\n\n![l](docs/img/l.png) ![s](./s.png)\n",
            ),
            ("docs/guide.md", "# Guide\n"),
            ("docs/img/l.png", ""),
            ("s.png", ""),
        ],
    );
    let site = site(root, &root.join("docs"));

    assert_eq!(files(&site, ""), ["docs/img/l.png", "s.png"]);
}

#[test]
fn images_on_the_readme_homepage_name_files_from_the_project_root_with_a_nested_source_dir() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("README.md", "# Home\n\n![l](site/docs/l.png) ![s](s.png)\n"),
            ("site/docs/b.md", "![h](../../h.png) ![d](d.png)\n"),
            ("site/docs/l.png", ""),
            ("s.png", ""),
            ("h.png", ""),
            ("site/docs/d.png", ""),
        ],
    );
    let site = site(root, &root.join("site/docs"));

    assert_eq!(files(&site, ""), ["s.png", "site/docs/l.png"]);
    assert_eq!(files(&site, "b"), ["h.png", "site/docs/d.png"]);
}

#[test]
fn a_source_dir_outside_the_project_names_no_files() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let docs = temp.path().join("elsewhere");
    write(
        temp.path(),
        &[(
            "elsewhere/guide.md",
            "![a](a.png) ![x](/x.png) ![b](b.bmp) ![c](../../../c.png)\n",
        )],
    );
    fs::create_dir_all(&project).unwrap();
    let site = site(&project, &docs);

    assert_eq!(files(&site, "guide"), [] as [String; 0]);
}

#[test]
fn a_missing_image_warns_on_every_render_until_it_exists() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, &[("docs/guide.md", "![a](img/a%20b.png?v=2)\n")]);
    let cache = Arc::new(FileCache::new(root.join(".cache"), "test"));
    let site = site_with_cache(root, &root.join("docs"), cache);

    let fresh = site.render("guide").unwrap();
    assert_eq!(fresh.warnings, ["image `docs/img/a b.png` not found"]);
    assert_eq!(
        fresh.html,
        "<p><img src=\"/_api/files/docs/img/a%20b.png?v=2\" alt=\"a\"></p>"
    );

    let cached = site.render("guide").unwrap();
    assert!(cached.from_cache);
    assert_eq!(cached.warnings, fresh.warnings);

    write(root, &[("docs/img/a b.png", "")]);
    let found = site.render("guide").unwrap();
    assert!(found.from_cache);
    assert_eq!(found.warnings, [] as [String; 0]);
    assert_eq!(site.read_file("docs/img/a b.png").unwrap(), b"");
}

#[cfg(unix)]
mod watch {
    use std::time::{Duration, Instant};

    use rw_storage::Storage;
    use rw_storage_fs::{StorageEvent, StorageEventReceiver};

    use super::*;

    /// Receives until the watcher has reported a change to each of `wanted`.
    fn recv_files(rx: &StorageEventReceiver, wanted: &[&str]) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut files = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let Ok(event) = rx.recv_timeout(left) else {
                break;
            };
            if let StorageEvent::File { path } = event {
                files.push(path);
                if wanted
                    .iter()
                    .all(|path| files.iter().any(|file| file == path))
                {
                    return;
                }
            }
        }
        panic!("expected {wanted:?}, got: {files:?}");
    }

    #[test]
    fn an_image_edit_under_a_symlinked_source_dir_names_the_file_as_renders_list_it() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let real = temp.path().join("real-docs");
        write(
            temp.path(),
            &[
                ("project/README.md", "# Home\n\n![l](docs/img/l.png)\n"),
                ("real-docs/guide.md", "![a](img/a.png)\n"),
                ("real-docs/img/a.png", ""),
                ("real-docs/img/l.png", ""),
                ("real-docs/img/unused.png", ""),
            ],
        );
        std::os::unix::fs::symlink(&real, root.join("docs")).unwrap();
        let storage = Arc::new(FsStorage::new(root.clone(), root.join("docs")));
        let site = Site::new(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::new(NullCache),
            PageRendererConfig::default(),
        );
        site.render("").unwrap();
        site.render("guide").unwrap();
        let (rx, _handle) = storage.watch().unwrap();

        for name in ["unused.png", "l.png", "a.png"] {
            fs::write(real.join("img").join(name), "edited").unwrap();
        }

        recv_files(&rx, &["docs/img/a.png", "docs/img/l.png"]);
        assert!(site.has_used_file("docs/img/a.png"));
        assert!(site.has_used_file("docs/img/l.png"));
        assert!(!site.has_used_file("docs/img/unused.png"));
    }
}
