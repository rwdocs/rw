//! `RW_DIAGRAMS_KROKI_URL` reaches the commands that render diagrams, and only
//! them. An invalid value proves the read without contacting a Kroki server.

use std::net::TcpListener;
use std::process::Output;

mod common;

use common::rw_cmd;

const INVALID: &str = "diagrams.kroki_url must start with http";

/// Runs `rw <args> --project-dir <a one-page project>` from that project, with
/// `RW_DIAGRAMS_KROKI_URL` set to `kroki_url`.
fn run_with(kroki_url: &str, args: &[&str]) -> Output {
    let project = tempfile::tempdir().expect("project tempdir");
    std::fs::create_dir(project.path().join("docs")).expect("create docs");
    std::fs::write(project.path().join("docs").join("guide.md"), "# Guide\n").expect("write guide");

    rw_cmd()
        .args(args)
        .arg("--project-dir")
        .arg(project.path())
        .current_dir(project.path())
        .env("RW_DIAGRAMS_KROKI_URL", kroki_url)
        // Keeps publish from waiting on the EC2 metadata service for credentials.
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .output()
        .expect("spawn rw")
}

fn run(args: &[&str]) -> Output {
    run_with("not-a-url", args)
}

#[test]
fn commands_that_render_diagrams_read_it() {
    // Busy, so a serve that ignored the variable would exit instead of serving.
    let occupied = TcpListener::bind("127.0.0.1:0").expect("bind occupier");
    let port = occupied
        .local_addr()
        .expect("local_addr")
        .port()
        .to_string();

    for args in [
        &["serve", "--port", &port][..],
        &["confluence", "render", "docs/guide.md", "--out", "-"],
        &[
            "comment",
            "add",
            "--document",
            "guide",
            "--quote",
            "Guide",
            "--body",
            "hi",
        ],
    ] {
        let output = run(args);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "rw {args:?}");
        assert!(stderr.contains(INVALID), "rw {args:?}: {stderr}");
    }
}

#[test]
fn commands_that_do_not_render_diagrams_ignore_it() {
    for args in [
        &["comment", "list"][..],
        &["comment", "add", "--document", "guide", "--body", "hi"],
    ] {
        let output = run(args);
        assert!(
            output.status.success(),
            "rw {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Fails reaching S3, after the configuration loaded.
    let output = run(&[
        "backstage",
        "publish",
        "--entity",
        "default/component/guide",
        "--bucket",
        "docs",
        "--endpoint",
        "http://127.0.0.1:1",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("S3 error"), "{stderr}");
}

#[test]
fn an_empty_value_counts_as_unset() {
    let output = run_with("", &["confluence", "render", "docs/guide.md", "--out", "-"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
