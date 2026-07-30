//! Frontmatter, sidecar `meta.yaml`, and the first H1 must resolve to one
//! title, description, and kind — the same values navigation already reports.
//!
//! These run over a real `FsStorage` and a temp docs directory on purpose. The
//! defect they pin lives in `Storage::meta`, whose lookup only ever finds a
//! sidecar file; a `MockStorage` whose metadata is injected by hand cannot
//! reproduce it and would pass against the broken code.

use std::fs;
use std::sync::Arc;

use rw_cache::NullCache;
use rw_site::{PageRendererConfig, Site};
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

    assert_eq!(result.title, "Billing");
}

#[test]
fn page_without_an_h1_still_reports_a_title() {
    let (_temp_dir, site) = site_with(&[("setup-guide.md", "Just prose, no heading.\n")]);

    let result = site.render("setup-guide").unwrap();

    assert_eq!(
        result.title, "Setup Guide",
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

    assert_eq!(
        result.description.as_deref(),
        Some("Money stuff"),
        "frontmatter description must reach the page response"
    );
    assert_eq!(
        result.page_kind.as_deref(),
        Some("domain"),
        "frontmatter kind must reach the page response"
    );
}

#[test]
fn root_page_reports_no_kind_when_it_declares_none() {
    let (_temp_dir, site) = site_with(&[("index.md", "# Home\n")]);

    let result = site.render("").unwrap();

    assert_eq!(
        result.page_kind, None,
        "the implicit root section (kind \"section\") must not leak onto a page \
         that declared no kind"
    );
}
