//! Helpers shared by the `rw` integration tests.

use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

pub const MISSING_ID: &str = "00000000-0000-4000-8000-000000000000";

/// Builds a `Command` for the `rw` binary with the environment cleared, so an
/// exported variable cannot change what a test observes. `rw` takes
/// configuration from the environment, and clearing wholesale rather than
/// removing known names covers variables added later. Callers add the
/// subcommand.
pub fn rw_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rw"));
    cmd.env_clear();
    // Windows needs `SYSTEMROOT` to start a process, and reads `TMP` before
    // `TEMP` when a child resolves a temp directory through `tempfile`,
    // falling back to a directory the CI user cannot write. `PATH` is not
    // needed to execute the binary but can carry a Windows DLL load. Anything
    // outside this list stays cleared, including `RUST_LOG`, which would
    // otherwise let an exported log level pollute the stderr tests assert on.
    // `LLVM_PROFILE_FILE` is how `cargo llvm-cov` collects the child's
    // coverage; without it the binary's profile is lost.
    for key in [
        "PATH",
        "TMPDIR",
        "SYSTEMROOT",
        "TEMP",
        "TMP",
        "LLVM_PROFILE_FILE",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    cmd
}

/// Runs `rw comment <args> --project-dir <project>`.
pub fn comment(project: &Path, args: &[&str]) -> Output {
    rw_cmd()
        .arg("comment")
        .args(args)
        .arg("--project-dir")
        .arg(project)
        .output()
        .expect("spawn rw comment")
}

/// Writes `contents` to `path`, creating its parent directories.
pub fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    std::fs::write(path, contents).expect("write file");
}

/// A project with one page, `guide`.
pub fn guide_project() -> TempDir {
    let project = tempfile::tempdir().expect("project tempdir");
    write(&project.path().join("docs/guide.md"), "# Guide\n");
    project
}

/// A port that stays taken while the listener lives, so an `rw serve` given it
/// exits instead of serving. The command sandbox blocks the bind, so tests
/// that call this need it disabled.
pub fn busy_port() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind occupier");
    let port = listener
        .local_addr()
        .expect("local_addr")
        .port()
        .to_string();
    (listener, port)
}
