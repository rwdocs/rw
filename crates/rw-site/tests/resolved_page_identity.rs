//! Frontmatter, sidecar `meta.yaml`, and the first H1 must resolve to one
//! title, description, and kind used consistently by rendering and search.
//!
//! These run over a real `FsStorage` and a temp docs directory on purpose so
//! they exercise metadata resolution end to end.

use std::fs;
use std::sync::Arc;

use rw_cache::{FileCache, NullCache};
use rw_site::{PageRendererConfig, Site};
use rw_storage::Storage;
use rw_storage_fs::FsStorage;

/// A page whose frontmatter title deliberately differs from its H1, so a test
/// can tell which one a surface reports. Asserting on a page where the two
/// agree would pass against either behavior.
const FRONTMATTER_TITLE_PAGE: &str = "\
---
title: Billing
---

# Billing API

Invoices and dunning.
";

/// Build a `Site` over a temp docs directory holding `files` (relative path →
/// contents). Returns the `TempDir` too, so it outlives the `Site`.
fn site_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Site) {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs = temp_dir.path().join("docs");
    fs::create_dir_all(&docs).unwrap();

    for (name, contents) in files {
        let path = docs.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    let storage = Arc::new(FsStorage::new(temp_dir.path().to_path_buf(), docs));
    let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());
    (temp_dir, site)
}

#[test]
fn search_document_uses_the_resolved_title_and_still_indexes_the_h1() {
    let (_temp_dir, site) = site_with(&[("billing.md", FRONTMATTER_TITLE_PAGE)]);

    let doc = site
        .render_search_document("billing")
        .unwrap()
        .expect("page has content");

    assert_eq!(
        doc.title, "Billing",
        "frontmatter title must win over the H1"
    );
    assert!(
        doc.text.contains("Billing API"),
        "the H1's words must stay searchable, got: {}",
        doc.text
    );
}

#[test]
fn page_response_uses_the_resolved_title() {
    let (_temp_dir, site) = site_with(&[("billing.md", FRONTMATTER_TITLE_PAGE)]);

    let result = site.render("billing").unwrap();

    assert_eq!(result.meta.title, "Billing");
}

#[test]
fn page_without_an_h1_still_reports_a_title() {
    let (_temp_dir, site) = site_with(&[("setup-guide.md", "Just prose, no heading.\n")]);

    let result = site.render("setup-guide").unwrap();

    assert_eq!(
        result.meta.title, "Setup Guide",
        "the filename fallback must reach the page response"
    );
}

#[test]
fn page_response_uses_frontmatter_description_and_kind() {
    let (_temp_dir, site) = site_with(&[(
        "billing.md",
        "---\ndescription: Money stuff\nkind: domain\n---\n\n# Billing\n",
    )]);

    let result = site.render("billing").unwrap();

    assert_eq!(result.meta.title, "Billing");
    assert_eq!(
        result.meta.description.as_deref(),
        Some("Money stuff"),
        "frontmatter description must reach the page response"
    );
    assert_eq!(
        result.meta.kind.as_deref(),
        Some("domain"),
        "frontmatter kind must reach the page response"
    );
}

#[test]
fn root_page_reports_no_kind_when_it_declares_none() {
    let (_temp_dir, site) = site_with(&[("index.md", "# Home\n")]);

    let result = site.render("").unwrap();

    assert_eq!(
        result.meta.kind, None,
        "the implicit root section (kind \"section\") must not leak onto a page \
         that declared no kind"
    );
}

#[test]
fn filesystem_meta_arc_is_shared_across_scan_fresh_and_cache_hit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs = temp_dir.path().join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(docs.join("guide.md"), "# Guide\n\nBody.\n").unwrap();

    let storage = Arc::new(FsStorage::new(temp_dir.path().to_path_buf(), docs));
    let mut scanned_docs = storage.scan().unwrap();
    assert_eq!(scanned_docs.len(), 1);
    let scanned = scanned_docs.remove(0);
    let meta = Arc::clone(&scanned.meta);

    let cache_dir = tempfile::tempdir().unwrap();
    let site = Site::new(
        storage,
        Arc::new(FileCache::new(cache_dir.path().join("cache"), "1.0.0")),
        PageRendererConfig::default(),
    );

    let fresh = site.render("guide").unwrap();
    assert!(!fresh.from_cache);
    assert!(Arc::ptr_eq(&meta, &fresh.meta));

    let cached = site.render("guide").unwrap();
    assert!(cached.from_cache);
    assert!(Arc::ptr_eq(&meta, &cached.meta));
}

#[test]
fn declared_section_name_changes_identity_not_path_or_title() {
    let (_temp, site) = site_with(&[
        (
            "systems/payments-guide/meta.yaml",
            "kind: system\nname: payments-api\nnamespace: commerce\ntitle: Payments documentation\n",
        ),
        ("systems/payments-guide/index.md", "# Body heading\n"),
        ("links.md", "# Links\n\n[[system:commerce/payments-api]]"),
    ]);
    assert_eq!(
        site.page_path_for("system:commerce/payments-api", "")
            .as_deref(),
        Some("systems/payments-guide")
    );
    assert!(
        site.page_path_for("system:commerce/payments-guide", "")
            .is_none()
    );
    assert_eq!(
        site.render("systems/payments-guide").unwrap().meta.title,
        "Payments documentation"
    );
    let linked = site.render("links").unwrap();
    assert!(
        linked.html.contains(r#"href="/systems/payments-guide""#),
        "{}",
        linked.html
    );
    assert!(
        linked
            .html
            .contains(r#"data-section-ref="system:commerce/payments-api""#)
    );
}

#[test]
fn declared_names_frontmatter_overlays_sidecar_with_invalid_field_recovery() {
    let (_temp, site) = site_with(&[
        ("named.md", "---\nname: Frontmatter\n---\n# Named"),
        ("named.meta.yaml", "kind: system\nname: Sidecar"),
        ("invalid.md", "---\nname: bad/value\n---\n# Invalid"),
        ("invalid.meta.yaml", "kind: system\nname: recovered"),
    ]);
    for (reference, path) in [
        ("system:default/Frontmatter", "named"),
        ("system:default/recovered", "invalid"),
    ] {
        assert_eq!(
            site.page_path_for(reference, "").as_deref(),
            Some(path),
            "{reference}"
        );
        assert_eq!(
            site.section_location(path).unwrap(),
            (reference.to_owned(), String::new())
        );
    }
}

#[test]
fn declared_names_use_selected_sidecars_and_discover_metadata_only_pages() {
    let (_temp, site) = site_with(&[
        ("virtual.meta.yaml", "kind: system\nname: metadata-only"),
        (
            "indexed/index.meta.yaml",
            "kind: system\nname: index-sidecar",
        ),
        ("selected.meta.yaml", "kind: system\nname: unselected"),
        (
            "selected/meta.yaml",
            "kind: system\nname: selected-directory",
        ),
    ]);
    for (reference, path) in [
        ("system:default/metadata-only", "virtual"),
        ("system:default/index-sidecar", "indexed"),
        ("system:default/selected-directory", "selected"),
    ] {
        assert_eq!(
            site.page_path_for(reference, "").as_deref(),
            Some(path),
            "{reference}"
        );
        assert_eq!(
            site.section_location(path).unwrap(),
            (reference.to_owned(), String::new())
        );
    }
    assert!(
        site.page_path_for("system:default/unselected", "")
            .is_none()
    );
    let pages = site.list_pages().unwrap();
    assert!(
        !pages
            .iter()
            .find(|p| p.path == "virtual")
            .unwrap()
            .has_content
    );
}

#[test]
fn declared_name_alone_keeps_implicit_root_identity() {
    let (_temp, site) = site_with(&[("index.md", "---\nname: ignored-home\n---\n# Home")]);
    assert_eq!(
        site.page_path_for("section:default/root", "").as_deref(),
        Some("")
    );
    assert_eq!(
        site.section_location("").unwrap(),
        ("section:default/root".to_owned(), String::new())
    );
    assert!(
        site.page_path_for("section:default/ignored-home", "")
            .is_none()
    );
}

#[test]
fn declared_names_do_not_inherit_or_make_ordinary_pages_sections() {
    let (_temp, site) = site_with(&[
        (
            "parent/meta.yaml",
            "kind: domain\nname: commerce\nnamespace: shop",
        ),
        ("parent/first.md", "---\nname: ordinary-only\n---\n# First"),
        ("parent/child.md", "---\nkind: system\n---\n# Child"),
    ]);
    for (reference, path) in [
        ("domain:shop/commerce", "parent"),
        ("system:shop/child", "parent/child"),
    ] {
        assert_eq!(
            site.page_path_for(reference, "").as_deref(),
            Some(path),
            "{reference}"
        );
        assert_eq!(
            site.section_location(path).unwrap(),
            (reference.to_owned(), String::new())
        );
    }
    assert_eq!(
        site.section_location("parent/first").unwrap(),
        ("domain:shop/commerce".to_owned(), "first".to_owned())
    );
    assert_eq!(
        site.render("parent/first").unwrap().meta.name.as_deref(),
        Some("ordinary-only")
    );
    assert_eq!(site.render("parent/child").unwrap().meta.name, None);
}

#[test]
fn declared_names_preserve_navigation_slug_order_and_listing_ancestry() {
    let (_temp, site) = site_with(&[
        (
            "parent/meta.yaml",
            "kind: domain\nname: commerce\nnamespace: shop\npages: [second, first]",
        ),
        ("parent/first.md", "---\nname: ordinary-only\n---\n# First"),
        ("parent/second.md", "# Second"),
        ("parent/child.md", "---\nkind: system\n---\n# Child"),
    ]);
    let nav = site.navigation(Some("domain:shop/commerce")).unwrap();
    assert_eq!(nav.scope.unwrap().section.name, "commerce");
    let paths: Vec<_> = nav.items.iter().map(|item| item.path.as_str()).collect();
    assert!(
        paths.iter().position(|p| *p == "parent/second").unwrap()
            < paths.iter().position(|p| *p == "parent/first").unwrap()
    );
    let sections = site.list_sections().unwrap();
    assert_eq!(
        sections
            .iter()
            .find(|s| s.path == "parent/child")
            .unwrap()
            .ancestors,
        vec!["domain:shop/commerce", "section:default/root"]
    );
    let pages = site.list_pages().unwrap();
    let first = pages.iter().find(|p| p.path == "parent/first").unwrap();
    assert_eq!(first.section_ref, "domain:shop/commerce");
    assert_eq!(first.subpath, "first");
    assert_eq!(first.anchors[1].section_ref, "section:default/root");
}

#[test]
fn declared_homepage_name_from_project_readme() {
    let temp = tempfile::tempdir().unwrap();
    let docs = temp.path().join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(
        temp.path().join("README.md"),
        "---\nkind: system\nname: homepage-api\n---\n# Readme title",
    )
    .unwrap();
    let storage = Arc::new(FsStorage::new(temp.path().to_path_buf(), docs));
    let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());
    assert_eq!(
        site.page_path_for("system:default/homepage-api", "")
            .as_deref(),
        Some("")
    );
    assert_eq!(site.render("").unwrap().meta.title, "Readme title");
}

#[test]
fn declared_name_from_configured_sidecar() {
    let temp = tempfile::tempdir().unwrap();
    let docs = temp.path().join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(
        docs.join("guide.config.yml"),
        "kind: system\nname: custom-sidecar",
    )
    .unwrap();
    let storage = Arc::new(FsStorage::with_meta_filename(
        temp.path().to_path_buf(),
        docs,
        "config.yml",
    ));
    let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());
    assert_eq!(
        site.page_path_for("system:default/custom-sidecar", "")
            .as_deref(),
        Some("guide")
    );
}

#[test]
fn declared_duplicate_refs_keep_first_path_and_all_pages() {
    let (_temp, site) = site_with(&[
        ("z.md", "---\nkind: system\nname: shared\n---\n# Z"),
        ("a.md", "---\nkind: system\nname: shared\n---\n# A"),
    ]);
    assert_eq!(
        site.page_path_for("system:default/shared", "").as_deref(),
        Some("a")
    );
    assert_eq!(site.render("z").unwrap().meta.title, "Z");
    assert_eq!(site.render("a").unwrap().meta.title, "A");
    assert!(site.page_path_for("system:default/a", "").is_none());
    assert!(site.page_path_for("system:default/z", "").is_none());
}

#[test]
fn declared_name_preserves_comment_identity_across_directory_move() {
    let (temp, before) = site_with(&[
        (
            "old/meta.yaml",
            "kind: system\nname: stable-api\nnamespace: commerce",
        ),
        ("old/api.md", "# API"),
    ]);
    let expected = ("system:commerce/stable-api".to_owned(), "api".to_owned());
    assert_eq!(before.section_location("old/api").unwrap(), expected);
    fs::rename(temp.path().join("docs/old"), temp.path().join("docs/new")).unwrap();
    let after = Site::new(
        Arc::new(FsStorage::new(
            temp.path().to_path_buf(),
            temp.path().join("docs"),
        )),
        Arc::new(NullCache),
        PageRendererConfig::default(),
    );
    assert_eq!(after.section_location("new/api").unwrap(), expected);
    assert_eq!(
        after.page_path_for(&expected.0, &expected.1).as_deref(),
        Some("new/api")
    );
}
