//! An image alone in its paragraph that names a diagram file renders as that
//! diagram, read from the project directory.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rw_cache::{Cache, FileCache};
use rw_kroki::test_support::KrokiStub;
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

fn write(root: &Path, files: &[(&str, &str)]) {
    for (name, contents) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

fn site(root: &Path, kroki: &KrokiStub) -> Site {
    let storage = Arc::new(FsStorage::new(root.to_path_buf(), root.join("docs")));
    let cache: Arc<dyn Cache> = Arc::new(FileCache::new(root.join(".cache"), "test"));
    Site::new(
        storage,
        cache,
        PageRendererConfig {
            kroki_url: Some(kroki.url.clone()),
            project_dir: Some(root.to_path_buf()),
            ..PageRendererConfig::default()
        },
    )
}

#[test]
fn a_lone_diagram_file_image_renders_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "docs/guide.md",
                "# Guide\n\n![Payment flow](diagrams/pay.puml)\n",
            ),
            (
                "docs/diagrams/pay.puml",
                "@startuml\n!include parts.iuml\n@enduml\n",
            ),
            ("docs/diagrams/parts.iuml", "Alice -> Bob\n"),
        ],
    );
    let kroki = KrokiStub::start();

    let page = site(root, &kroki).render("guide").unwrap();

    assert_eq!(page.warnings, [] as [String; 0]);
    assert!(page.files.is_empty(), "{:?}", page.files);
    assert!(
        page.html.contains(r#"aria-label="Payment flow""#),
        "{}",
        page.html
    );
    assert!(!page.html.contains("<p>"), "{}", page.html);
    let requests = kroki.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(requests[0].body.contains("Alice -> Bob"), "{requests:?}");
}

#[test]
fn a_diagram_file_image_inside_text_warns_and_shows_its_alt_text() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("docs/guide.md", "The flow: ![Payment flow](pay.puml)\n"),
            ("docs/pay.puml", "@startuml\nA -> B\n@enduml\n"),
        ],
    );
    let kroki = KrokiStub::start();

    let page = site(root, &kroki).render("guide").unwrap();

    assert_eq!(
        page.warnings,
        [
            "diagram `pay.puml` must be alone in its paragraph: put it on its own line with a blank line before and after"
        ]
    );
    assert_eq!(page.html, "<p>The flow: Payment flow</p>");
    assert!(page.files.is_empty(), "{:?}", page.files);
    assert_eq!(kroki.paths(), [] as [String; 0]);
}

#[test]
fn a_cached_page_re_renders_when_its_diagram_file_appears_or_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, &[("docs/guide.md", "![Flow](flow.mmd)\n")]);
    let kroki = KrokiStub::start();
    let site = site(root, &kroki);

    let missing = site.render("guide").unwrap();
    assert!(!missing.from_cache);
    assert_eq!(missing.warnings.len(), 1, "{:?}", missing.warnings);
    assert!(
        missing.warnings[0].contains("diagram file 'docs/flow.mmd'"),
        "{:?}",
        missing.warnings
    );
    assert!(site.render("guide").unwrap().from_cache);

    write(root, &[("docs/flow.mmd", "graph TD; first")]);
    let created = site.render("guide").unwrap();
    assert!(!created.from_cache);
    assert_eq!(created.warnings, [] as [String; 0]);
    assert!(created.html.contains("first"), "{}", created.html);
    assert!(site.render("guide").unwrap().from_cache);

    // Two writes within one mtime tick would leave the stamp unchanged.
    thread::sleep(Duration::from_millis(20));
    write(root, &[("docs/flow.mmd", "graph TD; second")]);
    let edited = site.render("guide").unwrap();
    assert!(!edited.from_cache);
    assert!(edited.html.contains("second"), "{}", edited.html);
}

#[test]
fn a_diagram_file_is_neither_shown_nor_served() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("docs/guide.md", "![Flow](flow.puml)\n"),
            ("docs/flow.puml", "@startuml\nA -> B\n@enduml\n"),
        ],
    );
    let kroki = KrokiStub::start();
    let site = site(root, &kroki);

    site.render("guide").unwrap();

    assert!(!site.has_shown_file("docs/flow.puml"));
    assert!(site.read_file("docs/flow.puml").is_err());
}
