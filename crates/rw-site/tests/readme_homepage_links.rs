//! A link to `README.md` goes wherever that file is published: `/` when it
//! serves the homepage, its own page otherwise, and nowhere when it is not
//! published.

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

fn assert_links(html: &str, hrefs: &[&str]) {
    for href in hrefs {
        assert!(
            html.contains(&format!(r#"href="{href}""#)),
            "expected href={href:?} in: {html}"
        );
    }
}

#[test]
fn links_to_a_readme_homepage_inside_source_dir_lead_to_the_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("README.md", "# Home\n"),
            (
                "guide.md",
                "[a](README.md) [b](./README.md#intro) [c](/README.md)\n",
            ),
            ("sub/page.md", "[d](../README.md)\n"),
        ],
    );
    let site = site(root, &root.join("."));

    assert_links(&site.render("guide").unwrap().html, &["/", "/#intro"]);
    assert!(!site.render("guide").unwrap().html.contains("/README"));
    assert_links(&site.render("sub/page").unwrap().html, &["/"]);
}

#[test]
fn links_to_a_readme_homepage_outside_source_dir_lead_to_the_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("README.md", "# Home\n"),
            ("docs/guide.md", "[a](../README.md#intro)\n"),
        ],
    );
    let site = site(root, &root.join("docs"));

    assert_links(&site.render("guide").unwrap().html, &["/#intro"]);
}

#[test]
fn a_page_named_readme_keeps_its_links() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("README.md", "# Home\n"),
            ("docs/README.md", "# Docs Readme\n"),
            ("docs/guide.md", "[a](README.md)\n"),
        ],
    );
    let site = site(root, &root.join("docs"));

    assert_links(&site.render("guide").unwrap().html, &["/README"]);
}

#[test]
fn links_to_a_readme_shadowed_by_an_index_lead_to_its_own_page() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("index.md", "# Index\n"),
            ("README.md", "# Readme\n"),
            ("guide.md", "[a](README.md)\n"),
        ],
    );
    let site = site(root, &root.join("."));

    assert_links(&site.render("guide").unwrap().html, &["/README"]);
}

#[test]
fn links_to_an_unpublished_readme_stay_broken() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("README.md", "# Readme\n"),
            ("docs/index.md", "# Index\n"),
            ("docs/guide.md", "[a](../README.md)\n"),
        ],
    );
    let site = site(root, &root.join("docs"));

    assert_links(&site.render("guide").unwrap().html, &["/README"]);
}
