//! A link to `README.md` goes wherever that file is published: its folder's
//! page when it serves one, and nowhere when it is not published.

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
fn links_to_a_docs_readme_homepage_lead_to_the_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("README.md", "# Home\n"),
            ("docs/README.md", "# Docs Readme\n"),
            ("docs/guide.md", "[a](README.md) [b](index.md)\n"),
        ],
    );
    let site = site(root, &root.join("docs"));

    assert_links(&site.render("guide").unwrap().html, &["/"]);
    assert!(!site.render("guide").unwrap().html.contains("/README"));
}

#[test]
fn links_to_a_readme_excluded_by_an_index_stay_broken() {
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
    assert!(site.render("README").is_err());
}

#[test]
fn links_to_folder_readmes_follow_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("docs/index.md", "# Home\n"),
            (
                "docs/guide.md",
                "[a](a/README.md#x) [b](b/README.md) [[::a/README]] [[::b/README]]\n",
            ),
            (
                "docs/a/README.md",
                "# A\n\n[step](step.md) [up](../README.md)\n",
            ),
            ("docs/a/step.md", "# Step\n"),
            ("docs/b/index.md", "# B\n"),
            ("docs/b/README.md", "# B Readme\n"),
        ],
    );
    let site = site(root, &root.join("docs"));

    let guide = site.render("guide").unwrap().html;
    assert_links(&guide, &["/a#x", "/b/README", "/a"]);
    // `b/index.md` serves `b`, so no page is published from `b/README.md`.
    assert!(
        guide.contains(r##"<a href="#" class="rw-broken-link">::b/README</a>"##),
        "{guide}"
    );
    assert_links(&site.render("a").unwrap().html, &["/a/step", "/README"]);
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

    assert_links(&site.render("guide").unwrap().html, &["../README.md"]);
}

#[test]
fn links_past_the_source_root_do_not_land_inside_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("README.md", "# Project\n"),
            ("docs/README.md", "# Docs Readme\n"),
            ("docs/guide.md", "[a](../README.md) [b](../guide.md)\n"),
        ],
    );
    let site = site(root, &root.join("docs"));

    assert_links(
        &site.render("guide").unwrap().html,
        &["../README.md", "../guide.md"],
    );
}

#[test]
fn links_from_the_project_readme_resolve_from_the_project_dir() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "README.md",
                "# Project\n\n[self](README.md) [g](docs/guide.md)\n",
            ),
            ("docs/guide.md", "[a](README.md)\n"),
        ],
    );
    let site = site(root, &root.join("docs"));

    assert_links(&site.render("").unwrap().html, &["/", "/guide"]);
    assert_links(&site.render("guide").unwrap().html, &["/README"]);
}

#[test]
fn wikilinks_to_a_folder_page_file_lead_to_the_folder() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "index.md",
                "# Home\n\n[[::a/index#x]] [[::b/README]] [[::c/index]]\n",
            ),
            ("a/index.md", "# A\n"),
            ("b/README.md", "# B\n"),
            ("c.md", "# C\n"),
        ],
    );
    let site = site(root, &root.join("."));

    let home = site.render("").unwrap().html;
    assert_links(&home, &["/a#x", "/b"]);
    assert!(
        home.contains(r##"<a href="#" class="rw-broken-link">::c/index</a>"##),
        "{home}"
    );
    assert!(home.contains(">A</a>"), "{home}");
}

#[test]
fn percent_encoded_links_follow_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "index.md",
                "# Home\n\n[a](my%20dir/index.md) [b](other%20dir/README.md#x) \
                 [c](%D0%B3%D0%B0%D0%B9%D0%B4/index.md) [d](гайд/index.md) \
                 [e](no%20dir/index.md)\n",
            ),
            ("my dir/index.md", "# Mine\n"),
            ("other dir/README.md", "# Other\n"),
            ("гайд/index.md", "# Гайд\n"),
        ],
    );
    let site = site(root, &root.join("."));

    assert_links(
        &site.render("").unwrap().html,
        &[
            "/my%20dir",
            "/other%20dir#x",
            "/%D0%B3%D0%B0%D0%B9%D0%B4",
            "/%D0%B3%D0%B0%D0%B9%D0%B4",
            "/no%20dir/index",
        ],
    );
}

#[test]
fn links_in_folders_named_with_url_delimiters_stay_in_the_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("index.md", "# Home\n"),
            ("c#/a.md", "[b](b.md)\n"),
            ("c#/b.md", "# B\n"),
            ("100%/a.md", "[b](b.md) [i](index.md)\n"),
            ("100%/b.md", "# B\n"),
            ("100%/index.md", "# Percent\n"),
        ],
    );
    let site = site(root, &root.join("."));

    assert!(site.render("c#/a").unwrap().html.contains(
        r#"href="/c%23/b" data-section-ref="section:default/root" data-section-path="c%23/b""#
    ));
    assert_links(
        &site.render("100%/a").unwrap().html,
        &["/100%25/b", "/100%25"],
    );
}

#[test]
fn links_through_the_source_dir_name_follow_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "README.md",
                "# Project\n\n[a](./docs/x.md) [b](docs/x.md)\n",
            ),
            ("docs/guide.md", "[c](../docs/x.md#y)\n"),
            ("docs/x.md", "# X\n"),
        ],
    );
    let site = site(root, &root.join("docs"));

    let home = site.render("").unwrap().html;
    assert_links(&home, &["/x"]);
    assert!(!home.contains("docs/x"), "{home}");
    assert_links(&site.render("guide").unwrap().html, &["/x#y"]);
}

#[test]
fn links_from_the_project_readme_resolve_from_the_project_dir_with_a_nested_source_dir() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "README.md",
                "# Project\n\n[x](docs/a.md) [y](site/docs/b.md) [z](./site/docs/b.md#q)\n",
            ),
            ("site/docs/a.md", "# A\n"),
            (
                "site/docs/b.md",
                "# B\n\n[home](../../README.md) [a](../docs/a.md) [site](../README.md)\n",
            ),
        ],
    );
    let site = site(root, &root.join("site/docs"));

    let home = site.render("").unwrap().html;
    assert_links(&home, &["docs/a.md", "/b", "/b#q"]);
    assert_links(
        &site.render("b").unwrap().html,
        &["/", "/a", "../README.md"],
    );
}

#[test]
fn links_through_every_level_of_a_deep_source_dir_follow_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("README.md", "# Project\n\n[x](a/site/docs/x.md)\n"),
            ("a/site/docs/x.md", "# X\n"),
            (
                "a/site/docs/b.md",
                "[home](../../../README.md) [1](../docs/x.md) [2](../../site/docs/x.md#q) \
                 [3](../../../a/site/docs/x.md)\n",
            ),
        ],
    );
    let site = site(root, &root.join("a/site/docs"));

    assert_links(&site.render("").unwrap().html, &["/x"]);
    let page = site.render("b").unwrap().html;
    assert_links(&page, &["/", "/x", "/x#q"]);
    assert!(!page.contains("docs/x"), "{page}");
}

#[test]
fn links_through_the_source_dir_name_follow_the_file_without_a_readme_homepage() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("site/docs/index.md", "# Home\n"),
            ("site/docs/x.md", "# X\n"),
            (
                "site/docs/b.md",
                "[1](../docs/x.md) [2](../../site/docs/x.md#q)\n",
            ),
        ],
    );
    let site = site(root, &root.join("site/docs"));

    let page = site.render("b").unwrap().html;
    assert_links(&page, &["/x", "/x#q"]);
    assert!(!page.contains("docs/x"), "{page}");
}

#[test]
fn links_to_one_missing_file_lead_to_one_dead_href_however_spelled() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "README.md",
                "# Project\n\n[a](docs/missing.md) [b](./docs/missing.md)\n",
            ),
            ("docs/guide.md", "[c](missing.md) [d](../docs/missing.md)\n"),
        ],
    );
    let site = site(root, &root.join("docs"));

    let home = site.render("").unwrap().html;
    assert_links(&home, &["/missing"]);
    assert!(!home.contains("docs/missing"), "{home}");
    let guide = site.render("guide").unwrap().html;
    assert_links(&guide, &["/missing"]);
    assert!(!guide.contains("docs/missing"), "{guide}");
}
