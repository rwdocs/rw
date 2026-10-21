//! On disk, a download type is served only once a page links to it, as S3
//! serves only the files published pages reference.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use rw_cache::NullCache;
use rw_site::{PageRendererConfig, Site};
use rw_storage::StorageErrorKind;
use rw_storage_fs::FsStorage;

fn write(root: &Path, files: &[(&str, &str)]) {
    for (name, contents) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

fn site(root: &Path) -> Site {
    let storage = Arc::new(FsStorage::new(root.to_path_buf(), root.join("docs")));
    Site::new(storage, Arc::new(NullCache), PageRendererConfig::default())
}

fn served(site: &Site, path: &str) -> bool {
    match site.read_file(path) {
        Ok(_) => true,
        Err(err) if err.kind == StorageErrorKind::NotFound => false,
        Err(err) => panic!("{path}: {err}"),
    }
}

#[test]
fn only_linked_downloads_are_served() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "docs/guide/intro.md",
                "[spec](spec.pdf) [deck](../slides%20v2.PPTX?v=1#p3) \
                 [ref][sheet] [[spec.zip]] `[code](code.zip)`\n\n\
                 [sheet]: /data/plan.xlsx\n\n\
                 ```\n[fenced](fenced.zip)\n```\n\n\
                 ![shown](shown.pdf)\n",
            ),
            ("README.md", "# Home\n\n[guide](docs/guide/handbook.docx)\n"),
            ("docs/guide/spec.pdf", "pdf"),
            ("docs/slides v2.PPTX", "pptx"),
            ("data/plan.xlsx", "xlsx"),
            ("docs/guide/handbook.docx", "docx"),
            ("docs/guide/spec.zip", "zip"),
            ("docs/guide/code.zip", "zip"),
            ("docs/guide/fenced.zip", "zip"),
            ("docs/guide/shown.pdf", "pdf"),
            ("docs/customers.csv", "csv"),
            ("docs/img/unlinked.png", "png"),
        ],
    );
    let site = site(root);

    for path in [
        "docs/guide/spec.pdf",
        "docs/slides v2.PPTX",
        "data/plan.xlsx",
        "docs/guide/handbook.docx",
        "docs/img/unlinked.png",
    ] {
        assert!(served(&site, path), "{path}");
    }
    for path in [
        "docs/customers.csv",
        "docs/guide/spec.zip",
        "docs/guide/code.zip",
        "docs/guide/fenced.zip",
        "docs/guide/shown.pdf",
    ] {
        assert!(!served(&site, path), "{path}");
    }
}

#[test]
fn a_page_edit_adds_and_removes_a_link() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[("docs/index.md", "# Home\n"), ("docs/customers.csv", "csv")],
    );
    let site = site(root);
    assert!(!served(&site, "docs/customers.csv"));

    write(root, &[("docs/index.md", "# Home\n\n[c](customers.csv)\n")]);
    site.invalidate_links();
    assert!(served(&site, "docs/customers.csv"));

    write(root, &[("docs/index.md", "# Home\n")]);
    site.invalidate_links();
    assert!(!served(&site, "docs/customers.csv"));
}

#[test]
fn a_new_page_can_link_a_download() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[("docs/index.md", "# Home\n"), ("docs/spec.pdf", "pdf")],
    );
    let site = site(root);
    assert!(!served(&site, "docs/spec.pdf"));

    write(root, &[("docs/new.md", "[spec](spec.pdf)\n")]);
    site.invalidate();
    assert!(served(&site, "docs/spec.pdf"));
}
