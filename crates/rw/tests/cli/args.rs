//! Argument parsing.

use crate::common::rw_cmd;

fn run(args: &[&str]) -> (i32, String) {
    let output = rw_cmd().args(args).output().expect("spawn rw");
    let code = output.status.code().expect("rw exited by signal");
    (code, String::from_utf8_lossy(&output.stderr).into_owned())
}

#[test]
fn usage_errors_exit_2() {
    for args in [
        &["comment", "show", "notauuid"][..],
        &["comment", "add"],
        &["comment", "bogus"],
        &["comment", "list", "-c", "rw.toml", "--project-dir", "."],
        &[
            "confluence",
            "render",
            "a.md",
            "--out",
            "-",
            "-c",
            "rw.toml",
            "--project-dir",
            ".",
        ],
        &["--bogus"],
        &["bogus"],
        &["serve", "--port", "notanumber"],
        &["confluence", "render", "--bogus"],
        &["backstage", "publish", "--bogus"],
        &["update", "--bogus"],
    ] {
        let (code, stderr) = run(args);
        assert_eq!(code, 2, "rw {args:?}: {stderr}");
        assert!(stderr.starts_with("error: "), "rw {args:?}: {stderr}");
    }
}

#[test]
fn missing_subcommand_prints_help_and_exits_2() {
    for args in [&[][..], &["comment"]] {
        let (code, stderr) = run(args);
        assert_eq!(code, 2, "rw {args:?}: {stderr}");
        assert!(stderr.contains("Usage: rw"), "rw {args:?}: {stderr}");
    }
}

#[test]
fn help_and_version_exit_0() {
    for args in [
        &["--help"][..],
        &["-h"],
        &["help"],
        &["help", "comment"],
        &["comment", "--help"],
        &["--version"],
        &["-V"],
    ] {
        let output = rw_cmd().args(args).output().expect("spawn rw");
        assert_eq!(output.status.code(), Some(0), "rw {args:?}");
        assert!(!output.stdout.is_empty(), "rw {args:?} printed nothing");
    }
}
