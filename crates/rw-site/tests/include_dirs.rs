//! An include directory checked when the config loaded can later become a
//! symlink out of the project, as a `git pull` can make it.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::Arc;

use rw_cache::{Cache, NullCache};
use rw_kroki::test_support::KrokiStub;
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

fn site(project: &Path, kroki_url: Option<String>) -> Site {
    let storage = Arc::new(FsStorage::new(project.to_path_buf(), project.join("docs")));
    let cache: Arc<dyn Cache> = Arc::new(NullCache);
    Site::new(
        storage,
        cache,
        PageRendererConfig {
            kroki_url,
            include_dirs: vec![project.join("later")],
            project_dir: Some(project.to_path_buf()),
        },
    )
}

#[test]
fn an_include_dir_that_now_leads_outside_the_project_is_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let outside = temp.path().join("outside");
    fs::create_dir_all(project.join("docs")).unwrap();
    fs::create_dir_all(project.join("later")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.puml"), "Alice -> Secret\n").unwrap();
    fs::write(
        project.join("docs/guide.md"),
        "```plantuml\n@startuml\n!include secret.puml\n@enduml\n```\n",
    )
    .unwrap();
    let kroki = KrokiStub::start();
    let rendering = site(&project, Some(kroki.url.clone()));
    let indexing = site(&project, None);

    fs::remove_dir(project.join("later")).unwrap();
    symlink(&outside, project.join("later")).unwrap();

    let page = rendering.render("guide").unwrap();
    assert!(
        page.warnings
            .iter()
            .any(|warning| warning.contains(&format!(
                "Include directory leads outside the project, skipped: '{}'",
                project.join("later").display()
            ))),
        "{:?}",
        page.warnings
    );
    let requests = kroki.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(!requests[0].body.contains("Secret"), "{requests:?}");

    let search = indexing.render_search_document("guide").unwrap().unwrap();
    assert!(!search.text.contains("Secret"), "{}", search.text);
}

#[test]
fn a_diagram_file_indexes_nothing_through_an_include_dir_leading_outside() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let outside = temp.path().join("outside");
    fs::create_dir_all(project.join("docs")).unwrap();
    fs::create_dir_all(project.join("later")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.puml"), "Alice -> Secret\n").unwrap();
    fs::write(project.join("docs/guide.md"), "![a](flow.puml)\n").unwrap();
    fs::write(
        project.join("docs/flow.puml"),
        "@startuml\n!include secret.puml\nAlice -> Bob\n@enduml\n",
    )
    .unwrap();
    let indexing = site(&project, None);

    fs::remove_dir(project.join("later")).unwrap();
    symlink(&outside, project.join("later")).unwrap();

    let search = indexing.render_search_document("guide").unwrap().unwrap();
    assert!(search.text.contains("Alice -> Bob"), "{}", search.text);
    assert!(!search.text.contains("Secret"), "{}", search.text);
}
