//! Integration test for `rw serve --project-dir`.
//!
//! `--project-dir <dir>` is meant to root the *entire* project at `<dir>`:
//! config, docs source dir, and the `.rw/` data directory (render cache,
//! comments DB, server.json) all follow it, regardless of the process's
//! working directory. The plausible wrong implementation wires the flag to a
//! source-directory override instead of the config loader, which would still
//! read the right markdown but leave `.rw/` rooted at the process cwd — this
//! test pins that `.rw/` is created under `--project-dir`, not cwd.
//!
//! `rw serve` normally blocks forever, which makes it awkward to drive from a
//! one-shot integration test. Instead this test gives it an explicitly busy
//! port: an explicit `--port` is a hard requirement (no fallback), so
//! `bind_listener` errors out and the process exits — but only *after*
//! `ensure_data_dir` has already run, so `.rw/` is created before the failure.
//!
//! This binds a real TCP socket to occupy a port, which the command sandbox
//! blocks; run with the sandbox disabled.

use std::net::TcpListener;
use std::process::Command;

/// Path to the `rw` binary built by Cargo.
fn rw_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rw")
}

#[test]
fn project_dir_roots_data_dir_not_cwd() {
    // Occupy a free port so the explicit --port passed below is guaranteed
    // busy; kept alive for the duration of the child process.
    let occupied = TcpListener::bind("127.0.0.1:0").expect("bind occupier");
    let port = occupied.local_addr().expect("local_addr").port();

    // No markdown fixture: the run never gets as far as reading content, so a
    // page file here would imply coverage this test does not have. All that is
    // required is that the directory exists — `load_from_dir` rejects one that
    // does not.
    let project_dir = tempfile::tempdir().expect("project tempdir");

    // A separate directory to run the child process from, distinct from
    // project_dir, standing in for "wherever the caller happened to be".
    let run_cwd = tempfile::tempdir().expect("run cwd tempdir");

    let output = Command::new(rw_bin())
        .arg("serve")
        .arg("--project-dir")
        .arg(project_dir.path())
        .arg("--port")
        .arg(port.to_string())
        .current_dir(run_cwd.path())
        .output()
        .expect("spawn rw serve");

    assert!(
        !output.status.success(),
        "expected failure binding an explicitly busy port, exit: {:?}",
        output.status
    );
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains(&format!("port {port} is already in use")),
        "stderr should report the busy port: {stderr}"
    );

    assert!(
        project_dir.path().join(".rw").is_dir(),
        "--project-dir's .rw/ should have been created before the bind failure"
    );
    assert!(
        !run_cwd.path().join(".rw").exists(),
        "the process cwd must not gain a .rw/ directory"
    );

    // Keep the occupier alive until after the assertions above.
    drop(occupied);
}
