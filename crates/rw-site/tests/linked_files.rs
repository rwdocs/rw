//! On disk, a download type is served only once a page links to it, as S3
//! serves only the files published pages reference.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use percent_encoding::percent_decode_str;
use rw_cache::NullCache;
use rw_site::{PageRendererConfig, Site};
use rw_storage::{MAX_FILE_SIZE, StorageErrorKind};
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

/// The `/_api/files` paths `html` links to, decoded as the route decodes them.
fn file_links(html: &str) -> BTreeSet<String> {
    html.split(r#"<a href="/_api/files/"#)
        .skip(1)
        .map(|rest| {
            let href = &rest[..rest.find(['"', '?', '#']).unwrap()];
            percent_decode_str(href).decode_utf8().unwrap().into_owned()
        })
        .collect()
}

#[test]
fn a_page_links_to_exactly_the_downloads_that_are_served() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let pages = [
        (
            "docs/guide/intro.md",
            "# Intro\n\n[spec](spec.pdf) [deck](../slides%20v2.PPTX?v=1#p3) \
             [ref][sheet] [[spec.zip]] `[code](code.zip)` [up](/data/../data/up.csv)\n\n\
             [sheet]: /data/plan.xlsx\n\n\
             ```\n[fenced](fenced.zip)\n```\n\n\
             ![shown](shown.pdf) ![alt [nested](nested.zip)](img/a.png)\n\n\
             ## See [the table](heading.csv)\n\n\
             > [!NOTE]\n> [quoted](quoted.docx)\n\n\
             | [cell](cell.csv) |\n|---|\n\n\
             Note[^1].\n\n[^1]: [footnote](footnote.zip)\n\n\
             [image](img/a.png) [cfg](config.yaml) [x](../../../x.pdf)\n",
        ),
        (
            "docs/tabs.md",
            "# Tabs\n\n::::tabs\n:::tab[A]\n[in a tab](гайд.pdf)\n:::\n::::\n",
        ),
        ("README.md", "# Home\n\n[guide](docs/guide/handbook.docx)\n"),
        (
            "docs/includes.md",
            "# Includes\n\n::include[_partials/links.md]\n\n[asset](_assets/d.pdf)\n",
        ),
        (
            "docs/_partials/links.md",
            "[included](included.zip)\n\n::include[deeper/more.md]\n",
        ),
        ("docs/_partials/deeper/more.md", "[nested](nested.zip)\n"),
    ];
    let downloads = [
        "docs/guide/spec.pdf",
        "docs/slides v2.PPTX",
        "data/plan.xlsx",
        "data/up.csv",
        "docs/guide/handbook.docx",
        "docs/guide/heading.csv",
        "docs/guide/quoted.docx",
        "docs/guide/cell.csv",
        "docs/guide/footnote.zip",
        "docs/гайд.pdf",
        "docs/guide/spec.zip",
        "docs/guide/code.zip",
        "docs/guide/fenced.zip",
        "docs/guide/shown.pdf",
        "docs/guide/nested.zip",
        "docs/customers.csv",
        "docs/_assets/d.pdf",
        "docs/_partials/included.zip",
        "docs/_partials/deeper/nested.zip",
    ];
    let mut files = Vec::from(pages);
    files.extend(downloads.map(|path| (path, "file")));
    files.extend([
        ("docs/guide/img/a.png", "png"),
        ("docs/guide/config.yaml", "k: v"),
    ]);
    write(root, &files);
    let site = site(root);

    let mut linked = BTreeSet::new();
    for path in ["", "guide/intro", "tabs", "includes"] {
        linked.extend(file_links(&site.render(path).unwrap().html));
    }

    assert!(linked.contains("docs/guide/img/a.png"));
    assert!(linked.contains("docs/_assets/d.pdf"));
    assert!(linked.contains("docs/_partials/included.zip"));
    assert!(linked.contains("docs/_partials/deeper/nested.zip"));
    for path in &linked {
        assert!(served(&site, path), "{path} is linked but not served");
    }
    for path in downloads {
        assert_eq!(served(&site, path), linked.contains(path), "{path}");
    }
}

#[cfg(unix)]
#[test]
fn an_include_refused_as_a_cycle_links_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("docs/a.md", "[r](report.pdf)\n\n::include[sub/link.md]\n"),
            ("docs/report.pdf", "pdf"),
            ("docs/sub/report.pdf", "pdf"),
        ],
    );
    std::os::unix::fs::symlink("../a.md", root.join("docs/sub/link.md")).unwrap();
    let site = site(root);

    assert!(served(&site, "docs/report.pdf"));
    assert!(!served(&site, "docs/sub/report.pdf"));
}

#[test]
fn a_linked_file_that_cannot_be_served_warns_with_the_reason() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[(
            "docs/index.md",
            "# Home\n\n[a](missing.pdf) [b](huge.zip) ![c](huge.png) [d](config.yaml)\n",
        )],
    );
    for name in ["docs/huge.zip", "docs/huge.png"] {
        fs::File::create(root.join(name))
            .unwrap()
            .set_len(MAX_FILE_SIZE + 1)
            .unwrap();
    }
    let site = site(root);

    let page = site.render("").unwrap();

    assert_eq!(
        page.warnings,
        [
            "link `config.yaml`: rw does not serve `.yaml` files",
            "image `docs/huge.png` is larger than 32 MiB",
            "linked file `docs/huge.zip` is larger than 32 MiB",
            "linked file `docs/missing.pdf` not found",
        ]
    );
}

#[test]
fn a_link_to_a_page_with_a_dot_in_its_name_does_not_warn() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "docs/index.md",
                "# Home\n\n[v](v1.2) [r](release-1.10) [x](v1.3)\n",
            ),
            ("docs/v1.2/index.md", "# v1.2\n\n[up](../release-1.10)\n"),
            ("docs/release-1.10.md", "# 1.10\n"),
        ],
    );
    let site = site(root);

    assert_eq!(
        site.render("").unwrap().warnings,
        ["link `v1.3`: rw does not serve `.3` files"]
    );
    assert_eq!(site.render("v1.2").unwrap().warnings, [] as [String; 0]);
}
