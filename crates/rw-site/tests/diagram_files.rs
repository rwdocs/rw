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
fn a_cached_page_re_renders_when_an_include_next_to_its_diagram_file_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("docs/guide.md", "![Flow](flow.puml)\n"),
            (
                "docs/flow.puml",
                "@startuml\n!include parts.iuml\n@enduml\n",
            ),
            ("docs/parts.iuml", "Alice -> Bob\n"),
        ],
    );
    let kroki = KrokiStub::start();
    let site = site(root, &kroki);
    assert!(!site.render("guide").unwrap().from_cache);
    assert!(site.render("guide").unwrap().from_cache);

    // Two writes within one mtime tick would leave the stamp unchanged.
    thread::sleep(Duration::from_millis(20));
    write(root, &[("docs/parts.iuml", "Carol -> Dave\n")]);
    let edited = site.render("guide").unwrap();

    assert!(!edited.from_cache);
    assert!(edited.html.contains("Carol -> Dave"), "{}", edited.html);
}

#[test]
fn a_diagram_file_is_not_served() {
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

    let page = site.render("guide").unwrap();

    assert!(page.files.is_empty(), "{:?}", page.files);
    assert!(site.read_file("docs/flow.puml").is_err());
}

#[test]
fn a_page_uses_its_diagram_files_and_the_includes_they_looked_for() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            (
                "docs/guide.md",
                "![Flow](flow.puml)\n\n![Later](later.mmd)\n",
            ),
            (
                "docs/flow.puml",
                "@startuml\n!include parts.iuml\n!include missing.iuml\n@enduml\n",
            ),
            ("docs/parts.iuml", "Alice -> Bob\n"),
            ("docs/other.puml", "@startuml\nA -> B\n@enduml\n"),
        ],
    );
    let kroki = KrokiStub::start();
    let site = site(root, &kroki);

    site.render("guide").unwrap();

    for used in [
        "docs/flow.puml",
        "docs/parts.iuml",
        "docs/missing.iuml",
        "docs/later.mmd",
    ] {
        assert!(site.has_used_file(used), "{used}");
    }
    assert!(!site.has_used_file("docs/other.puml"));
}

#[test]
fn a_diagram_file_named_in_another_case_is_used() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("docs/guide.md", "![Flow](Flow.puml)\n"),
            ("docs/flow.puml", "@startuml\nA -> B\n@enduml\n"),
            ("docs/sub/flow.puml", "@startuml\nA -> B\n@enduml\n"),
        ],
    );
    let kroki = KrokiStub::start();
    let site = site(root, &kroki);

    site.render("guide").unwrap();

    assert!(site.has_used_file("docs/flow.puml"));
    assert!(!site.has_used_file("docs/flow.pu"));
    assert!(!site.has_used_file("docs/sub/flow.puml"));
}

#[test]
fn a_diagram_file_is_indexed_as_the_same_source_in_a_fence_would_be() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let flow = "@startuml\nskinparam monochrome true\nAlice -> Zanzibar\n@enduml\n";
    let steps = "graph TD\n    Quokka-->Bob\n";
    let fences = format!(
        "```plantuml {{alt=\"Payment flow\"}}\n{flow}```\n\n```mermaid {{alt=\"Steps\"}}\n{steps}```\n"
    );
    write(
        root,
        &[
            (
                "docs/files.md",
                "![Payment flow](flow.puml)\n\n![Steps](steps.mmd)\n",
            ),
            ("docs/fences.md", &fences),
            ("docs/flow.puml", flow),
            ("docs/steps.mmd", steps),
        ],
    );
    let kroki = KrokiStub::start();
    let site = site(root, &kroki);

    let files = site.render_search_document("files").unwrap().unwrap();
    let fences = site.render_search_document("fences").unwrap().unwrap();

    assert_eq!(files.text, fences.text);
    assert!(files.text.contains("Zanzibar"), "{}", files.text);
    assert!(files.text.contains("Quokka"), "{}", files.text);
    assert!(files.text.contains("Payment flow"), "{}", files.text);
    assert!(!files.text.contains("skinparam"), "{}", files.text);
    assert_eq!(kroki.paths(), [] as [String; 0]);
}

#[cfg(unix)]
#[test]
fn a_diagram_file_leading_out_of_the_project_is_not_indexed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    write(
        temp.path(),
        &[
            ("secret.puml", "Alice -> Zanzibar\n"),
            ("project/docs/guide.md", "![Flow](flow.puml)\n"),
        ],
    );
    std::os::unix::fs::symlink(temp.path().join("secret.puml"), root.join("docs/flow.puml"))
        .unwrap();
    let kroki = KrokiStub::start();

    let search = site(&root, &kroki)
        .render_search_document("guide")
        .unwrap()
        .unwrap();

    assert!(!search.text.contains("Zanzibar"), "{}", search.text);
}

#[test]
fn a_diagram_file_indexes_what_it_includes_from_beside_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        &[
            ("docs/guide.md", "![Flow](flow.puml)\n"),
            (
                "docs/flow.puml",
                "@startuml\n!include parts.iuml\n@enduml\n",
            ),
            ("docs/parts.iuml", "Alice -> Zanzibar\n"),
        ],
    );
    let kroki = KrokiStub::start();

    let search = site(root, &kroki)
        .render_search_document("guide")
        .unwrap()
        .unwrap();

    assert!(search.text.contains("Zanzibar"), "{}", search.text);
}

#[test]
fn a_diagram_file_indexes_nothing_it_includes_from_outside_the_project() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    write(
        temp.path(),
        &[
            ("secret.iuml", "Alice -> Zanzibar\n"),
            ("project/docs/guide.md", "![Flow](flow.puml)\n"),
            (
                "project/docs/flow.puml",
                "@startuml\n!include ../../secret.iuml\n@enduml\n",
            ),
        ],
    );
    let kroki = KrokiStub::start();

    let search = site(&root, &kroki)
        .render_search_document("guide")
        .unwrap()
        .unwrap();

    assert!(!search.text.contains("Zanzibar"), "{}", search.text);
}
