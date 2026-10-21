//! Where `rw` roots the project and what it requires there.

use std::path::Path;
use std::process::{Output, Stdio};

use crate::common::{MISSING_ID, busy_port, rw_cmd, write};

fn rw(cwd: &Path, args: &[&str]) -> Output {
    rw_cmd()
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        // Fails a publish that gets past the project check fast, instead of
        // probing for credentials.
        .env("AWS_ACCESS_KEY_ID", "test")
        .env("AWS_SECRET_ACCESS_KEY", "test")
        .output()
        .expect("spawn rw")
}

/// Every project command, run without naming a project, so it roots itself
/// by discovery.
fn each_project_command(cwd: &Path, mut check: impl FnMut(&[&str], Output)) {
    let (_occupied, port) = busy_port();
    for args in [
        &["comment", "list"][..],
        &["comment", "show", MISSING_ID],
        &[
            "comment",
            "add",
            "--document",
            "guides/intro",
            "--body",
            "hi",
        ],
        &["serve", "--port", &port],
        &["confluence", "render", "intro.md", "--out", "-"],
        &[
            "backstage",
            "publish",
            "--entity",
            "default/Component/x",
            "--bucket",
            "b",
            "--endpoint",
            "http://127.0.0.1:1",
        ],
    ] {
        check(args, rw(cwd, args));
    }
}

#[test]
fn every_command_fails_from_a_subdirectory_of_a_project_without_rw_toml() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path();
    assert!(
        project.ancestors().all(|dir| !dir.join("rw.toml").exists()),
        "an rw.toml above {} would root these commands there",
        project.display()
    );
    write(&project.join("docs/index.md"), "# Home\n");
    write(&project.join("docs/guides/intro.md"), "# Intro\n");
    let guides = project.join("docs/guides");
    // Only the tail: the reported cwd differs from the temp path in its
    // prefix (`/private/var` on macOS, no `\\?\` on Windows).
    let message = format!("{} has no docs", Path::new("docs").join("guides").display());

    each_project_command(&guides, |args, output| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(3), "rw {args:?}: {stderr}");
        assert!(
            stderr.contains("project root ") && stderr.contains(&message),
            "rw {args:?}: {stderr}"
        );
        assert!(stderr.contains("--project-dir"), "rw {args:?}: {stderr}");
    });
    assert!(!guides.join(".rw").exists());
}

#[test]
fn a_readme_alone_is_a_project() {
    let project = tempfile::tempdir().expect("tempdir");
    write(&project.path().join("README.md"), "# Home\n");

    let output = rw(project.path(), &["comment", "list", "--project-dir", "."]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
}

#[test]
fn project_dir_without_docs_fails() {
    let project = tempfile::tempdir().expect("tempdir");
    let dir = project.path().to_str().expect("utf8");

    let output = rw(project.path(), &["comment", "list", "--project-dir", dir]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(3), "{stderr}");
    assert!(
        stderr.contains(&format!("project root {dir} has no docs")),
        "{stderr}"
    );
}

#[test]
fn a_missing_explicit_root_exits_4() {
    let tmp = tempfile::tempdir().expect("tempdir");

    for args in [
        &["comment", "list", "--project-dir", "missing"][..],
        &["comment", "list", "-c", "missing.toml"],
        &["serve", "--project-dir", "missing"],
        &[
            "confluence",
            "render",
            "a.md",
            "--out",
            "-",
            "-c",
            "missing.toml",
        ],
    ] {
        let output = rw(tmp.path(), args);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(4), "rw {args:?}: {stderr}");
        assert!(
            stderr.contains("not found: missing"),
            "rw {args:?}: {stderr}"
        );
    }
}

/// A file outside any project renders without its local images, so naming a
/// root without docs is enough.
#[test]
fn confluence_render_takes_an_explicit_root_without_docs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write(&tmp.path().join("page.md"), "# Page\n\nBody.\n");
    write(&tmp.path().join("rw.toml"), "");

    for flags in [["--project-dir", "."], ["-c", "rw.toml"]] {
        let output = rw(
            tmp.path(),
            &[
                &["confluence", "render", "page.md", "--out", "-"][..],
                &flags,
            ]
            .concat(),
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{flags:?}: {stderr}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Body."),
            "{flags:?}"
        );
    }
}

#[test]
fn read_only_comment_commands_create_nothing() {
    let project = tempfile::tempdir().expect("tempdir");
    write(&project.path().join("docs/guide.md"), "# Guide\n");

    for (args, code) in [
        (&["comment", "list", "--project-dir", "."][..], 0),
        (
            &[
                "comment",
                "list",
                "--project-dir",
                ".",
                "--document",
                "guide",
            ],
            0,
        ),
        (&["comment", "show", "--project-dir", ".", MISSING_ID], 4),
    ] {
        let output = rw(project.path(), args);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(code), "rw {args:?}: {stderr}");
    }
    assert!(!project.path().join(".rw").exists());
}

#[test]
fn comment_takes_project_dir_and_config() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().join("project");
    write(&project.join("rw.toml"), "[docs]\nsource_dir = \"site\"\n");
    write(&project.join("site/guide.md"), "# Guide\n");
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).expect("create elsewhere");
    let project_dir = project.to_str().expect("utf8");
    let config = project.join("rw.toml");

    for flags in [
        ["--project-dir", project_dir],
        ["--config", config.to_str().expect("utf8")],
    ] {
        let output = rw(
            &elsewhere,
            &[
                &["comment", "add", "--document", "guide", "--body", "hi"][..],
                &flags,
            ]
            .concat(),
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{flags:?}: {stderr}");
    }
    assert!(project.join(".rw/comments/sqlite.db").is_file());
    assert!(!elsewhere.join(".rw").exists());
}

#[test]
fn confluence_render_takes_project_dir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().join("project");
    write(&project.join("docs/img/flow.png"), "flow");
    write(
        &project.join("docs/page.md"),
        "![Flow](/docs/img/flow.png)\n",
    );
    let out = tmp.path().join("dist");

    let output = rw(
        tmp.path(),
        &[
            "confluence",
            "render",
            "project/docs/page.md",
            "--project-dir",
            "project",
            "--out",
            out.to_str().expect("utf8"),
        ],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("warning:"), "{stderr}");
}

/// `.rw/` follows `--project-dir`, not the working directory. The busy port
/// makes `rw serve` exit after it creates `.rw/`.
#[test]
fn serve_roots_the_data_dir_at_project_dir() {
    let project = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(project.path().join("docs")).expect("create docs");
    let cwd = tempfile::tempdir().expect("tempdir");
    let (_occupied, port) = busy_port();
    let dir = project.path().to_str().expect("utf8");

    let output = rw(
        cwd.path(),
        &["serve", "--project-dir", dir, "--port", &port],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("port {port} is already in use")),
        "{stderr}"
    );
    assert!(project.path().join(".rw").is_dir());
    assert!(!cwd.path().join(".rw").exists());
}
