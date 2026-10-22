//! `::include[path]` renders a project markdown file in place of the line,
//! read through the storage.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rw_cache::{Cache, FileCache, NullCache};
use rw_site::{PageRendererConfig, Site};
use rw_storage::{MockStorage, StorageErrorKind};
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
    let cache: Arc<dyn Cache> = Arc::new(FileCache::new(root.join(".cache"), "test"));
    Site::new(
        storage,
        cache,
        PageRendererConfig {
            project_dir: Some(root.to_path_buf()),
            ..PageRendererConfig::default()
        },
    )
}

fn mock_site(storage: MockStorage, cache_dir: &Path) -> Site {
    let cache: Arc<dyn Cache> = Arc::new(FileCache::new(cache_dir.to_path_buf(), "test"));
    Site::new(Arc::new(storage), cache, PageRendererConfig::default())
}

#[test]
fn an_underscore_partial_renders_with_links_and_images_from_its_folder() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "docs/guide/index.md",
                "# Guide\n\n::include[../_partials/note.md]\n",
            ),
            (
                "docs/_partials/note.md",
                "---\ntitle: Ignored\n---\n# Note\n\nSee [setup](../setup.md).\n\n![logo](logo.png)\n",
            ),
            ("docs/_partials/logo.png", ""),
            ("docs/setup.md", "# Setup\n"),
        ],
    );

    let page = site(root).render("guide").unwrap();

    assert_eq!(page.warnings, [] as [String; 0]);
    assert_eq!(page.meta.title, "Guide");
    assert!(
        page.html.contains(r#"<h1 id="note">Note</h1>"#),
        "{}",
        page.html
    );
    assert!(page.html.contains(r#"href="/setup""#), "{}", page.html);
    assert!(!page.html.contains("Ignored"), "{}", page.html);
    assert_eq!(Vec::from_iter(&page.files), ["docs/_partials/logo.png"]);
}

#[test]
fn a_failed_markdown_include_warns_and_renders_as_written() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[("docs/guide.md", "::include[_partials/missing.md]\n")],
    );

    let page = site(root).render("guide").unwrap();

    assert_eq!(
        page.warnings,
        ["include `docs/_partials/missing.md` not found"]
    );
    assert!(
        page.html.contains("::include[_partials/missing.md]"),
        "{}",
        page.html
    );
}

#[test]
fn creating_editing_or_deleting_a_markdown_include_invalidates_the_cached_page() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, &[("docs/guide.md", "::include[_note.md]\n")]);
    let site = site(root);
    assert!(!site.render("guide").unwrap().from_cache);
    assert!(site.render("guide").unwrap().from_cache);

    write(root, &[("docs/_note.md", "first")]);
    let created = site.render("guide").unwrap();
    assert!(!created.from_cache);
    assert!(created.html.contains("first"), "{}", created.html);
    assert!(site.render("guide").unwrap().from_cache);

    // Two writes within one mtime tick would leave the stamp unchanged.
    thread::sleep(Duration::from_millis(20));
    write(root, &[("docs/_note.md", "second")]);
    let edited = site.render("guide").unwrap();
    assert!(!edited.from_cache);
    assert!(edited.html.contains("second"), "{}", edited.html);

    fs::remove_file(root.join("docs/_note.md")).unwrap();
    let deleted = site.render("guide").unwrap();
    assert!(!deleted.from_cache);
    assert_eq!(deleted.warnings, ["include `docs/_note.md` not found"]);
}

#[test]
fn a_rendered_or_cached_page_has_used_its_markdown_includes_found_or_not() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "docs/guide.md",
                "::include[_a.md]\n\n::include[_missing.md]\n",
            ),
            ("docs/_a.md", "A"),
        ],
    );
    for from_cache in [false, true] {
        let site = site(root);
        assert_eq!(site.render("guide").unwrap().from_cache, from_cache);

        assert!(site.has_used_file("docs/_a.md"));
        assert!(site.has_used_file("docs/_missing.md"));
        assert!(!site.has_used_file("docs/guide.md"));
    }
}

#[test]
fn a_rendered_or_cached_page_lists_the_files_it_included_at_any_depth() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "docs/guide.md",
                "::include[_a.md]\n\n::include[_missing.md]\n",
            ),
            ("docs/_a.md", "::include[_shared/b.md]\n"),
            ("docs/_shared/b.md", "B"),
        ],
    );
    for from_cache in [false, true] {
        let page = site(root).render("guide").unwrap();
        assert_eq!(page.from_cache, from_cache);

        assert_eq!(
            Vec::from_iter(&page.includes),
            ["docs/_a.md", "docs/_shared/b.md"]
        );
    }
}

#[test]
fn a_refused_markdown_include_is_cached() {
    let cache = tempfile::tempdir().unwrap();
    let storage = MockStorage::new()
        .with_file("guide", "Guide", "::include[_bad.md]\n")
        .with_mtime("guide", 1000.0)
        .with_project_file("guide/_bad.md", [0xff]);
    let site = mock_site(storage, cache.path());

    let first = site.render("guide").unwrap();
    assert_eq!(
        first.warnings,
        ["include `guide/_bad.md` is not UTF-8 text"]
    );
    assert!(site.render("guide").unwrap().from_cache);
}

#[test]
fn a_markdown_include_that_fails_to_read_is_not_cached() {
    let cache = tempfile::tempdir().unwrap();
    let storage = MockStorage::new()
        .with_file("guide", "Guide", "::include[_note.md]\n")
        .with_mtime("guide", 1000.0)
        .with_unreadable_project_file("guide/_note.md", StorageErrorKind::Unavailable);
    let site = mock_site(storage, cache.path());

    let first = site.render("guide").unwrap();
    assert_eq!(first.warnings.len(), 1, "{:?}", first.warnings);
    assert!(
        first.warnings[0].starts_with("include `guide/_note.md` can't be read: "),
        "{:?}",
        first.warnings
    );
    assert!(!site.render("guide").unwrap().from_cache);
}

#[test]
fn a_page_read_with_its_includes_renders_them_and_reads_none_on_demand() {
    let cache = tempfile::tempdir().unwrap();
    let storage = MockStorage::new()
        .with_file(
            "guide",
            "Guide",
            "::include[_note.md]\n\n::include[_unpublished.md]\n",
        )
        .with_mtime("guide", 1000.0)
        .with_page_includes("guide", [("guide/_note.md", "Published.")])
        .with_project_file("guide/_note.md", "On disk.")
        .with_project_file("guide/_unpublished.md", "On disk.");
    let site = mock_site(storage, cache.path());

    let page = site.render("guide").unwrap();

    assert!(page.html.contains("Published."), "{}", page.html);
    assert!(!page.html.contains("On disk."), "{}", page.html);
    assert_eq!(page.warnings, ["include `guide/_unpublished.md` not found"]);
    assert!(site.render("guide").unwrap().from_cache);
}

#[test]
fn a_download_counts_as_linked_from_the_includes_read_with_its_page() {
    let storage = MockStorage::new()
        .with_file("guide", "Guide", "::include[_links.md]\n")
        .with_page_includes("guide", [("guide/_links.md", "[a](published.zip)")])
        .with_project_file("guide/_links.md", "[a](on-disk.zip)")
        .with_project_file("guide/published.zip", "zip")
        .with_project_file("guide/on-disk.zip", "zip");
    let site = Site::new(
        Arc::new(storage),
        Arc::new(NullCache),
        PageRendererConfig::default(),
    );

    assert!(site.read_file("guide/published.zip").is_ok());
    assert_eq!(
        site.read_file("guide/on-disk.zip").unwrap_err().kind,
        StorageErrorKind::NotFound
    );
}
