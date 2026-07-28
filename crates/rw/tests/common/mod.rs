//! Helpers shared by the `rw` integration tests.
//!
//! Lives in `common/mod.rs` rather than `common.rs` because Cargo turns every
//! *file* directly under `tests/` into its own test binary — a `tests/common.rs`
//! would be built and run as a third one, containing no tests. Files in a
//! subdirectory are not targets, so this is reachable only via `mod common;`.

use std::process::Command;

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
    for key in ["PATH", "TMPDIR", "SYSTEMROOT", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    cmd
}
