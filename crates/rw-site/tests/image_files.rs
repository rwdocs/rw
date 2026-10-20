//! A page's local images name files by their path in the project, wherever
//! the source directory sits in it.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use rw_cache::NullCache;
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
    let storage = Arc::new(FsStorage::new(
        project_dir.to_path_buf(),
        source_dir.to_path_buf(),
    ));
    Site::new(storage, Arc::new(NullCache), PageRendererConfig::default())
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
