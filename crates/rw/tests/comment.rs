//! Integration tests for `rw comment`.

mod common;

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
