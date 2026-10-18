//! Integration tests for `rw comment`.

mod common;

use common::rw_cmd;

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
