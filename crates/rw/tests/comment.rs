//! Integration tests for `rw comment`.

mod common;

use std::path::Path;

use common::rw_cmd;
use serde_json::Value;

#[test]
fn comment_list_gitignores_data_dir() {
    let project = tempfile::tempdir().expect("project tempdir");
    // Discovery walks upward without bound; this file stops it at the project.
    std::fs::write(project.path().join("rw.toml"), "").expect("write rw.toml");

    let output = rw_cmd()
        .arg("comment")
        .arg("list")
        .current_dir(project.path())
        .output()
        .expect("spawn rw comment list");
    assert!(
        output.status.success(),
        "rw comment list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let data_dir = project.path().join(".rw");
    assert!(data_dir.join("comments").join("sqlite.db").is_file());
    let gitignore =
        std::fs::read_to_string(data_dir.join(".gitignore")).expect("read .rw/.gitignore");
    assert_eq!(gitignore, "# Automatically created by rw\n*\n");
}

const MISSING_ID: &str = "00000000-0000-4000-8000-000000000000";

fn assert_not_found(args: &[&str], message: &str) {
    let project = tempfile::tempdir().expect("project tempdir");
    std::fs::write(project.path().join("rw.toml"), "").expect("write rw.toml");
    std::fs::create_dir(project.path().join("docs")).expect("create docs");
    std::fs::write(project.path().join("docs").join("guide.md"), "# Guide\n").expect("write guide");

    let output = rw_cmd()
        .arg("comment")
        .args(args)
        .current_dir(project.path())
        .output()
        .expect("spawn rw comment");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(4),
        "rw comment {args:?}: {stderr}"
    );
    assert!(stderr.contains(message), "rw comment {args:?}: {stderr}");
}

#[test]
fn missing_comment_exits_4() {
    assert_not_found(&["show", MISSING_ID], MISSING_ID);
    assert_not_found(&["resolve", MISSING_ID], MISSING_ID);
    assert_not_found(&["reply", MISSING_ID, "--body", "hi"], MISSING_ID);
}

#[test]
fn missing_document_exits_4() {
    assert_not_found(&["add", "--document", "nope", "--body", "hi"], "nope");
    assert_not_found(
        &[
            "add",
            "--document",
            "nope",
            "--quote",
            "Guide",
            "--body",
            "hi",
        ],
        "nope",
    );
}

#[test]
fn document_accepts_a_project_relative_path_under_a_nested_source_dir() {
    let project = tempfile::tempdir().expect("project tempdir");
    std::fs::write(
        project.path().join("rw.toml"),
        "[docs]\nsource_dir = \"site/docs\"\n",
    )
    .expect("write rw.toml");
    let docs = project.path().join("site").join("docs");
    std::fs::create_dir_all(&docs).expect("create site/docs");
    std::fs::write(docs.join("guide.md"), "# Guide\n").expect("write guide");
    let comment = |args: &[&str]| {
        let output = rw_cmd()
            .arg("comment")
            .args(args)
            .args(["--format", "json"])
            .current_dir(project.path())
            .output()
            .expect("spawn rw comment");
        assert!(
            output.status.success(),
            "rw comment {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).expect("JSON output")
    };

    let added = comment(&["add", "--document", "site/docs/guide.md", "--body", "hi"]);
    let listed = comment(&["list", "--document", "site/docs/guide.md"]);

    assert_eq!(listed[0]["id"], added["id"], "{listed}");
}

/// What `rw comment add --document <document>` keys the comment on: the page
/// under the root section, or the exit code with stderr.
fn added_to(project: &Path, document: &str) -> Result<String, (i32, String)> {
    let output = rw_cmd()
        .args(["comment", "add", "--document", document, "--body", "hi"])
        .args(["--format", "json"])
        .current_dir(project)
        .output()
        .expect("spawn rw comment add");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err((output.status.code().expect("exit code"), stderr));
    }
    let comment: Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    let key = comment["documentId"].as_str().expect("documentId");
    Ok(key
        .strip_prefix("section:default/root#")
        .expect("root section key")
        .to_owned())
}

fn write_project(project: &Path, source_dir: &str, pages: &[&str]) {
    std::fs::create_dir_all(project).expect("create project");
    std::fs::write(
        project.join("rw.toml"),
        format!("[docs]\nsource_dir = \"{source_dir}\"\n"),
    )
    .expect("write rw.toml");
    for page in pages {
        let path = project.join(page);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
        std::fs::write(&path, "# Page\n").expect("write page");
    }
}

#[test]
fn document_maps_a_source_file_to_the_page_it_publishes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("project");
    write_project(
        &project,
        "docs",
        &[
            "README.md",
            "docs/guide.md",
            "docs/billing/index.md",
            "docs/setup/README.md",
            "docs/both/index.md",
            "docs/both/README.md",
            "docs/x.md",
            "docs/x/index.md",
            "docs/docs/guide.md",
            "docs/.hidden/guide.md",
            "elsewhere/guide.md",
        ],
    );
    let readme = project.join("README.md");
    let guide = project.join("docs/guide.md");
    let dotted = project.join("elsewhere/../docs/guide.md");
    let elsewhere = project.join("elsewhere/guide.md");

    for (document, page) in [
        ("guide.md", Some("guide")),
        ("./guide.md", Some("guide")),
        ("docs/docs/guide.md", Some("docs/guide")),
        ("billing/index.md", Some("billing")),
        ("docs/billing/index.md", Some("billing")),
        ("docs/setup/README.md", Some("setup")),
        ("x/index.md", Some("x")),
        ("README.md", Some("")),
        ("./README.md", Some("")),
        (readme.to_str().unwrap(), Some("")),
        (guide.to_str().unwrap(), Some("guide")),
        (dotted.to_str().unwrap(), Some("guide")),
        // Shadowed by the `index.md` next to them.
        ("both/README.md", None),
        ("x.md", None),
        ("nope.md", None),
        // Not scanned, so no page.
        (".hidden/guide.md", None),
        (elsewhere.to_str().unwrap(), None),
        ("../docs/guide.md", None),
        ("docs/../docs/guide.md", None),
    ] {
        match (added_to(&project, document), page) {
            (Ok(got), Some(page)) => assert_eq!(got, page, "{document}"),
            (Err((3, stderr)), None) => {
                assert!(stderr.contains("not a markdown page"), "{stderr}");
            }
            (got, _) => panic!("{document}: {got:?}"),
        }
    }

    let (code, stderr) = added_to(&project, "docs/guide.md").unwrap_err();
    assert_eq!(code, 3);
    assert!(
        stderr.contains("could be the page 'docs/guide' or 'guide'"),
        "{stderr}"
    );
}

#[test]
fn document_maps_the_project_readme_only_while_it_is_the_homepage() {
    let dir = tempfile::tempdir().expect("tempdir");
    for source_dir in ["docs", "site/docs", ".", "../content"] {
        let project = dir.path().join(source_dir.replace(['.', '/'], "_"));
        write_project(
            &project,
            source_dir,
            &["README.md", &format!("{source_dir}/guide.md")],
        );
        let readme = project.join("README.md");

        for document in ["README.md", readme.to_str().unwrap()] {
            assert_eq!(
                added_to(&project, document),
                Ok(String::new()),
                "{source_dir}: {document}"
            );
        }
    }

    let project = dir.path().join("index");
    write_project(&project, "docs", &["README.md", "docs/index.md"]);
    assert_eq!(added_to(&project, "index.md"), Ok(String::new()));
    assert_eq!(added_to(&project, "README.md").unwrap_err().0, 3);
}

#[cfg(unix)]
#[test]
fn document_sees_an_absolute_path_through_a_symlink() {
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("project");
    write_project(&project, "docs", &["README.md", "docs/guide.md"]);
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&project, &link).expect("symlink");

    for (file, page) in [("README.md", ""), ("docs/guide.md", "guide")] {
        let document = link.join(file);
        let document = document.to_str().unwrap();
        assert_eq!(
            added_to(&project, document),
            Ok(page.to_owned()),
            "{document}"
        );
    }
}
