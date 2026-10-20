//! Integration tests for the thread-shape and quote rules of `rw comment`.

mod common;

use std::path::Path;
use std::process::Output;

use common::rw_cmd;
use tempfile::TempDir;

fn project() -> TempDir {
    let project = tempfile::tempdir().expect("project tempdir");
    std::fs::create_dir(project.path().join("docs")).expect("create docs");
    std::fs::write(
        project.path().join("docs").join("guide.md"),
        "# Guide\n\nThe quick brown fox jumps.\n",
    )
    .expect("write guide.md");
    project
}

fn comment(project: &Path, args: &[&str]) -> Output {
    rw_cmd()
        .arg("comment")
        .args(args)
        .arg("--project-dir")
        .arg(project)
        .output()
        .expect("spawn rw comment")
}

/// Run a command expected to succeed and return the created comment's id.
fn created_id(project: &Path, args: &[&str]) -> String {
    let output = comment(project, &[args, &["--format", "json"]].concat());
    assert!(
        output.status.success(),
        "rw comment {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    json["id"].as_str().expect("id").to_owned()
}

fn assert_exit_3(output: &Output, message: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(3), "stderr: {stderr}");
    assert!(stderr.contains(message), "stderr: {stderr}");
}

#[test]
fn reply_to_a_reply_exits_3() {
    let project = project();
    let thread = created_id(
        project.path(),
        &["add", "--document", "guide", "--body", "thread"],
    );
    let reply = created_id(project.path(), &["reply", &thread, "--body", "reply"]);

    let output = comment(project.path(), &["reply", &reply, "--body", "nested"]);

    assert_exit_3(
        &output,
        &format!("parent {reply} is a reply; reply to its thread {thread} instead"),
    );
}

#[test]
fn resolving_a_reply_exits_3() {
    let project = project();
    let thread = created_id(
        project.path(),
        &["add", "--document", "guide", "--body", "thread"],
    );
    let reply = created_id(project.path(), &["reply", &thread, "--body", "reply"]);

    let output = comment(project.path(), &["resolve", &reply]);

    assert_exit_3(
        &output,
        &format!("comment {reply} is a reply; only top-level comments can be resolved"),
    );
}

#[test]
fn empty_quote_exits_3() {
    let project = project();

    let output = comment(
        project.path(),
        &["add", "--document", "guide", "--body", "b", "--quote", ""],
    );

    assert_exit_3(&output, "quote is empty; omit it for a page-level comment");
}

#[test]
fn reply_on_a_removed_page_exits_4() {
    let project = project();
    let thread = created_id(
        project.path(),
        &["add", "--document", "guide", "--body", "thread"],
    );
    std::fs::remove_file(project.path().join("docs").join("guide.md")).expect("remove guide.md");

    let output = comment(project.path(), &["reply", &thread, "--body", "reply"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(4), "stderr: {stderr}");
    assert!(
        stderr.contains("'guide' does not exist"),
        "stderr: {stderr}"
    );
}

#[test]
fn comment_on_a_virtual_page_is_accepted() {
    let project = project();
    let area = project.path().join("docs").join("area");
    std::fs::create_dir(&area).expect("create area");
    std::fs::write(area.join("meta.yaml"), "title: Area\n").expect("write meta.yaml");

    created_id(
        project.path(),
        &["add", "--document", "area", "--body", "thread"],
    );
}
