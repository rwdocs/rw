//! `rw confluence render`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use rw_kroki::test_support::{KrokiStub, PNG_HEADER, Reply};

use crate::common::rw_cmd;

/// Gives `dir` the `docs/` directory every command requires of a project root,
/// and returns `dir`.
fn project(dir: &Path) -> &Path {
    std::fs::create_dir_all(dir.join("docs")).expect("create docs");
    dir
}

/// Writes `markdown` into `dir` and returns a hermetic [`rw_cmd`] that renders
/// it, with the project rooted at `dir`. Callers append their own `--out` and
/// any flags under test.
fn rw_render(dir: &Path, markdown: &str) -> Command {
    let source = dir.join("in.md");
    std::fs::write(&source, markdown).expect("write markdown");

    let mut cmd = rw_cmd();
    cmd.current_dir(dir)
        .arg("confluence")
        .arg("render")
        .arg(&source)
        .arg("--project-dir")
        .arg(project(dir));
    cmd
}

/// Runs `cmd` with `stdin` as the current page XHTML, capturing stdout and stderr.
fn run_with_stdin(cmd: &mut Command, stdin: &[u8]) -> Output {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rw");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    child.wait_with_output().expect("wait")
}

#[test]
fn render_bundle_mode_writes_page_xhtml_and_emits_title_to_stderr() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# Title\n\nBody.\n";
    let out_dir = tmp.path().join("dist");

    let output = rw_render(tmp.path(), md)
        .arg("--out")
        .arg(&out_dir)
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    assert!(output.status.success(), "exit: {:?}", output.status);

    let xhtml = std::fs::read_to_string(out_dir.join("page.xhtml")).expect("page.xhtml");
    assert!(xhtml.contains("Body"));

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("title: Title"),
        "stderr should contain extracted title: {stderr}"
    );

    // Regression guard: the bundle is page.xhtml + PNGs only — no manifest.
    assert!(
        !out_dir.join("manifest.json").exists(),
        "manifest.json should not exist"
    );
}

#[test]
#[ignore = "requires KROKI_URL env var pointing at a live Kroki server"]
fn render_stdout_mode_errors_when_render_produces_attachments() {
    // Read in this process and handed to the child as `--kroki-url`; the child
    // never reads the variable itself.
    //
    // The only test whose child opens a TLS connection, so the only one whose
    // child needs `SSL_CERT_FILE`, `SSL_CERT_DIR` and `HTTPS_PROXY` — none of
    // which `rw_cmd` passes through.
    let kroki_url =
        std::env::var("KROKI_URL").expect("set KROKI_URL=https://kroki.io to run this test");

    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# Diag\n\n```mermaid\ngraph TD\nA-->B\n```\n";

    let output = rw_render(tmp.path(), md)
        .arg("--out")
        .arg("-")
        .arg("--kroki-url")
        .arg(&kroki_url)
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");

    assert_eq!(
        output.status.code(),
        Some(3),
        "expected exit code 3, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("attachment"), "stderr: {stderr}");
    assert!(stderr.contains("--out -"), "stderr: {stderr}");
}

#[test]
fn render_strict_exits_1_when_warning_emitted() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# T\n\nBody.\n";
    let out_dir = tmp.path().join("dist");

    // Malformed current_xhtml on stdin -> comment_preservation emits a
    // "comment preservation skipped" warning, which --strict promotes to
    // exit 1.
    let output = run_with_stdin(
        rw_render(tmp.path(), md)
            .arg("--out")
            .arg(&out_dir)
            .arg("--strict"),
        b"<p>unclosed paragraph",
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1, got {:?}",
        output.status
    );

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    // The CLI prints the underlying warning before the strict-mode error.
    assert!(
        stderr.contains("comment preservation skipped"),
        "stderr should mention the underlying warning: {stderr}"
    );
    // And the strict-mode error message must surface.
    assert!(
        stderr.contains("--strict was set"),
        "stderr should mention --strict: {stderr}"
    );
}

#[test]
fn render_strict_stdout_mode_writes_body_then_exits_1() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# T\n\nBody.\n";

    let output = run_with_stdin(
        rw_render(tmp.path(), md)
            .arg("--out")
            .arg("-")
            .arg("--strict"),
        b"<p>unclosed paragraph",
    );
    assert_eq!(output.status.code(), Some(1), "exit: {:?}", output.status);

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Body"), "stdout missing body: {stdout}");

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("comment preservation skipped"),
        "stderr missing warning: {stderr}"
    );
    assert!(
        stderr.contains("completed with 1 warning(s); --strict was set"),
        "stderr missing strict error: {stderr}"
    );
}

#[test]
fn render_strict_exits_1_on_unmatched_comment_even_with_no_warnings() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "Completely different content now.\n";
    let out_dir = tmp.path().join("dist");

    let output = run_with_stdin(
        rw_render(tmp.path(), md)
            .arg("--out")
            .arg(&out_dir)
            .arg("--strict"),
        b"<p><ac:inline-comment-marker ac:ref=\"abc\">Original sentence here\
              </ac:inline-comment-marker></p>",
    );
    assert_eq!(output.status.code(), Some(1), "exit: {:?}", output.status);
}

#[test]
fn render_stdout_mode_keeps_diagnostics_off_stdout() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Trigger title and unmatched-comment diagnostics. (Warnings flow through
    // the same `print_diagnostics` writer; the dedicated
    // `render_strict_exits_1_when_warning_emitted` test covers the warning
    // path.)
    let md = "# Stdout title\n\nDifferent content here.\n";

    let output = run_with_stdin(
        rw_render(tmp.path(), md).arg("--out").arg("-"),
        b"<p><ac:inline-comment-marker ac:ref=\"abc\">Original sentence here\
              </ac:inline-comment-marker></p>",
    );
    assert!(output.status.success(), "exit: {:?}", output.status);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    // Stdout MUST contain only the XHTML body.
    assert!(stdout.contains("Different content here."), "{stdout}");
    assert!(
        !stdout.contains("title:"),
        "stdout leaked title diagnostic: {stdout}"
    );
    assert!(
        !stdout.contains("warning:"),
        "stdout leaked warning diagnostic: {stdout}"
    );
    assert!(
        !stdout.contains("could not be placed"),
        "stdout leaked unmatched-comments diagnostic: {stdout}"
    );

    // Stderr MUST carry the diagnostics it produced.
    assert!(
        stderr.contains("title: Stdout title"),
        "stderr missing title: {stderr}"
    );
    assert!(
        stderr.contains("1 comment(s) could not be placed:"),
        "stderr missing unmatched header: {stderr}"
    );
    assert!(stderr.contains("[abc]"), "stderr missing ref id: {stderr}");
}

#[test]
fn render_no_extract_title_omits_title_from_stderr() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# Title\n\nBody.\n";
    let out_dir = tmp.path().join("dist");

    let output = rw_render(tmp.path(), md)
        .arg("--out")
        .arg(&out_dir)
        .arg("--no-extract-title")
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    assert!(output.status.success(), "exit: {:?}", output.status);

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        !stderr.contains("title:"),
        "stderr should not have a title line when --no-extract-title is set: {stderr}"
    );
}

#[test]
fn render_no_toc_omits_confluence_toc_macro() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# Title\n\n## Sub\n\nBody.\n";
    let out_dir = tmp.path().join("dist");

    let status = rw_render(tmp.path(), md)
        .arg("--out")
        .arg(&out_dir)
        .arg("--no-toc")
        .stdin(Stdio::null())
        .status()
        .expect("spawn rw");
    assert!(status.success());

    let xhtml = std::fs::read_to_string(out_dir.join("page.xhtml")).expect("page.xhtml");
    assert!(
        !xhtml.contains(r#"ac:name="toc""#),
        "page.xhtml should not contain a toc macro: {xhtml}"
    );
}

#[test]
fn render_missing_markdown_error_names_the_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("missing.md");

    let output = rw_cmd()
        .current_dir(tmp.path())
        .arg("confluence")
        .arg("render")
        .arg(&missing)
        .arg("--project-dir")
        .arg(project(tmp.path()))
        .arg("--out")
        .arg("-")
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    assert_eq!(output.status.code(), Some(1), "exit: {:?}", output.status);

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains(&format!("Error: cannot read {}: ", missing.display())),
        "{stderr}"
    );
}

/// The only test that has `rw` read a variable named in `rw.toml` from its
/// environment; expansion itself is tested in-process with an injected lookup.
#[test]
fn render_expands_rw_toml_reference_from_the_environment() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let source = tmp.path().join("in.md");
    std::fs::write(&source, "# Title\n\nBody.\n").expect("write markdown");
    let config = project(tmp.path()).join("rw.toml");
    std::fs::write(
        &config,
        "[diagrams]\nkroki_url = \"${RW_TEST_KROKI_URL}/$path\"\n",
    )
    .expect("write rw.toml");

    let render = || {
        let mut cmd = rw_cmd();
        cmd.current_dir(tmp.path())
            .arg("confluence")
            .arg("render")
            .arg(&source)
            .arg("--config")
            .arg(&config)
            .arg("--out")
            .arg("-")
            .stdin(Stdio::null());
        cmd
    };

    let set = render()
        .env("RW_TEST_KROKI_URL", "https://kroki.invalid")
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(set.stderr).expect("utf8 stderr");
    assert!(set.status.success(), "{stderr}");

    let unset = render().output().expect("spawn rw");
    let stderr = String::from_utf8(unset.stderr).expect("utf8 stderr");
    assert!(!unset.status.success());
    assert!(stderr.contains("${RW_TEST_KROKI_URL} not set"), "{stderr}");
}

/// A page of the site sees the site's entities, as it does under `rw serve`.
#[test]
fn render_expands_a_site_pages_metadata_includes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let docs = tmp.path().join("docs");
    std::fs::create_dir_all(docs.join("payments")).expect("mkdir");
    std::fs::write(
        docs.join("payments/index.md"),
        "---\nkind: system\n---\n# Payments API\n",
    )
    .expect("write system");
    std::fs::write(
        docs.join("arch.md"),
        "# Arch\n\n```c4plantuml\n@startuml\n\
         !include systems/sys_payments.iuml\n\
         !include systems/sys_missing.iuml\n\
         @enduml\n```\n",
    )
    .expect("write page");
    let kroki = KrokiStub::start();

    let output = rw_cmd()
        .current_dir(tmp.path())
        .args(["confluence", "render", "docs/arch.md", "--out", "dist"])
        .args(["--kroki-url", &kroki.url])
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "{stderr}");

    let requests = kroki.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    let source = &requests[0].body;
    assert!(source.contains(r#""Payments API""#), "{source}");
    assert!(!source.contains("sys_payments.iuml"), "{source}");
    assert!(
        stderr.contains("Include file not found: 'systems/sys_missing.iuml'"),
        "{stderr}"
    );
}

/// `sha256("flow")`, the attachment name of an image file holding `flow`.
const FLOW: &str = "image_3b21278178ed.png";

/// Files in `dir`, sorted.
fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .into_string()
                .expect("utf8")
        })
        .collect();
    names.sort();
    names
}

#[test]
fn render_copies_a_local_image_into_the_bundle() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(tmp.path().join("img")).expect("mkdir");
    std::fs::write(tmp.path().join("img/flow.png"), "flow").expect("write image");
    let out_dir = tmp.path().join("dist");

    let output = rw_render(tmp.path(), "![Flow](img/flow.png)\n")
        .arg("--out")
        .arg(&out_dir)
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("warning:"), "{stderr}");

    assert_eq!(files_in(&out_dir), [FLOW, "page.xhtml"]);
    assert_eq!(std::fs::read(out_dir.join(FLOW)).expect("read"), b"flow");
    let xhtml = std::fs::read_to_string(out_dir.join("page.xhtml")).expect("page.xhtml");
    assert!(
        xhtml.contains(&format!(r#"<ri:attachment ri:filename="{FLOW}" />"#)),
        "{xhtml}"
    );
}

/// `--config rw.toml` roots the project at the current directory.
#[test]
fn render_copies_a_local_image_with_a_bare_config_file_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(tmp.path().join("img")).expect("mkdir");
    std::fs::write(tmp.path().join("img/flow.png"), "flow").expect("write image");
    std::fs::write(tmp.path().join("in.md"), "![Flow](img/flow.png)\n").expect("write md");
    std::fs::write(project(tmp.path()).join("rw.toml"), "").expect("write rw.toml");

    let output = rw_cmd()
        .current_dir(tmp.path())
        .args(["confluence", "render", "in.md", "--config", "rw.toml"])
        .args(["--out", "dist"])
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("warning:"), "{stderr}");

    assert_eq!(files_in(&tmp.path().join("dist")), [FLOW, "page.xhtml"]);
}

/// `rw serve` shows the images of a docs directory symlinked out of the
/// project, so the bundle carries them too.
#[cfg(unix)]
#[test]
fn render_copies_a_local_image_from_a_docs_dir_symlinked_out_of_the_project() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().join("project");
    let shared = tmp.path().join("shared/docs");
    std::fs::create_dir(&project).expect("mkdir");
    std::fs::create_dir_all(shared.join("img")).expect("mkdir");
    std::fs::write(shared.join("img/flow.png"), "flow").expect("write image");
    std::fs::write(shared.join("page.md"), "![Flow](img/flow.png)\n").expect("write md");
    std::os::unix::fs::symlink(&shared, project.join("docs")).expect("symlink");
    let out_dir = tmp.path().join("dist");

    let output = rw_cmd()
        .current_dir(&project)
        .args(["confluence", "render", "docs/page.md", "--project-dir"])
        .arg(&project)
        .arg("--out")
        .arg(&out_dir)
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("warning:"), "{stderr}");

    assert_eq!(files_in(&out_dir), [FLOW, "page.xhtml"]);
}

/// `rw serve` refuses these files, so the bundle must not carry them either.
#[cfg(unix)]
#[test]
fn render_does_not_bundle_a_file_storage_refuses() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().join("project");
    std::fs::create_dir(&project).expect("mkdir");
    std::fs::write(tmp.path().join("secret.png"), "secret").expect("write secret");
    std::os::unix::fs::symlink("../secret.png", project.join("escape.png")).expect("symlink");
    std::fs::write(project.join(".env"), "SECRET=1").expect("write env");
    std::os::unix::fs::symlink(".env", project.join("env.png")).expect("symlink");
    std::fs::File::create(project.join("huge.png"))
        .expect("create")
        .set_len(32 * 1024 * 1024 + 1)
        .expect("grow");
    let out_dir = tmp.path().join("dist");

    let output = rw_render(&project, "![e](escape.png) ![v](env.png) ![h](huge.png)\n")
        .arg("--out")
        .arg(&out_dir)
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "{stderr}");

    assert_eq!(files_in(&out_dir), ["page.xhtml"]);
    for name in ["escape.png", "env.png", "huge.png"] {
        assert!(
            stderr.contains(&format!(
                "warning: attachment '{name}' is not in the bundle; upload it to the page yourself"
            )),
            "{stderr}"
        );
    }
}

/// There is no directory to copy into, but a local image is not a reason to
/// fail the render the way a diagram is.
#[test]
fn render_stdout_mode_leaves_a_local_image_to_the_publisher() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(tmp.path().join("img")).expect("mkdir");
    std::fs::write(tmp.path().join("img/flow.png"), "flow").expect("write image");

    let output = rw_render(tmp.path(), "![Flow](img/flow.png)\n")
        .arg("--out")
        .arg("-")
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(output.status.code(), Some(0), "{stderr}");

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains(r#"<ri:attachment ri:filename="flow.png" />"#),
        "{stdout}"
    );
    assert!(
        stderr.contains(
            "warning: attachment 'flow.png' is not in the bundle; upload it to the page yourself"
        ),
        "{stderr}"
    );
}

/// A lone image naming a diagram file is that diagram, attached as a PNG like
/// a fence, its includes found next to the file.
#[test]
fn render_attaches_a_lone_diagram_file_image_as_a_diagram() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(tmp.path().join("flows")).expect("mkdir");
    std::fs::write(
        tmp.path().join("flows/pay.puml"),
        "@startuml\n!include parts.iuml\n@enduml\n",
    )
    .expect("write diagram");
    std::fs::write(tmp.path().join("flows/parts.iuml"), "Alice -> Bob\n").expect("write include");
    let kroki = KrokiStub::answering(Reply::Fixed {
        status: 200,
        body: PNG_HEADER,
    });
    let out_dir = tmp.path().join("dist");

    let output = rw_render(tmp.path(), "# Pay\n\n![Payment flow](flows/pay.puml)\n")
        .arg("--out")
        .arg(&out_dir)
        .args(["--kroki-url", &kroki.url, "--strict"])
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "{stderr}");

    let requests = kroki.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(requests[0].path.ends_with("/png"), "{requests:?}");
    assert!(requests[0].body.contains("Alice -> Bob"), "{requests:?}");
    let files = files_in(&out_dir);
    let [diagram, page] = files.as_slice() else {
        panic!("{files:?}");
    };
    assert_eq!(page, "page.xhtml");
    assert!(
        diagram
            .strip_prefix("diagram_")
            .and_then(|rest| rest.strip_suffix(".png"))
            .is_some_and(|hash| hash.len() == 12),
        "{diagram}"
    );
    let xhtml = std::fs::read_to_string(out_dir.join(page)).expect("page.xhtml");
    assert!(
        xhtml.contains(&format!(
            r#"ac:alt="Payment flow"><ri:attachment ri:filename="{diagram}" />"#
        )),
        "{xhtml}"
    );
}

/// A diagram file is a diagram on stdout too, so it fails the render as a
/// fence does, rather than leaving the publisher an attachment to upload.
#[test]
fn render_stdout_mode_rejects_a_lone_diagram_file_image() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("pay.puml"), "@startuml\nA -> B\n@enduml\n")
        .expect("write diagram");
    let kroki = KrokiStub::answering(Reply::Fixed {
        status: 200,
        body: PNG_HEADER,
    });

    let output = rw_render(tmp.path(), "![Payment flow](pay.puml)\n")
        .args(["--out", "-", "--kroki-url", &kroki.url])
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    assert_eq!(output.status.code(), Some(3), "{stderr}");
    assert_eq!(kroki.requests().len(), 1);
}

/// Renders a page whose one `PlantUML` fence includes `include`, into `dist`,
/// against `kroki`.
fn render_including(dir: &Path, include: &str, kroki: &KrokiStub) -> Command {
    let mut cmd = rw_render(
        dir,
        &format!("```plantuml\n@startuml\n!include {include}\n@enduml\n```\n"),
    );
    cmd.arg("--out")
        .arg(dir.join("dist"))
        .args(["--kroki-url", &kroki.url])
        .stdin(Stdio::null());
    cmd
}

/// A cloned repo's `rw.toml` names its include directories and, through
/// `kroki_url`, where their files are sent.
#[test]
fn render_refuses_an_rw_toml_include_dir_outside_the_project() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        project(tmp.path()).join("rw.toml"),
        "[diagrams]\ninclude_dirs = [\"/\"]\n",
    )
    .expect("write rw.toml");
    let kroki = KrokiStub::start();

    let output = render_including(tmp.path(), "etc/hosts", &kroki)
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains(r#"diagrams.include_dirs entry "/" is outside the project"#),
        "{stderr}"
    );
    assert_eq!(kroki.requests(), []);
}

#[test]
fn render_inlines_only_plantuml_files_from_an_include_dir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        project(tmp.path()).join("rw.toml"),
        "[diagrams]\ninclude_dirs = [\".\"]\n",
    )
    .expect("write rw.toml");
    std::fs::write(tmp.path().join(".env"), "API_KEY=secret\n").expect("write .env");
    let kroki = KrokiStub::answering(Reply::Fixed {
        status: 200,
        body: PNG_HEADER,
    });

    let output = render_including(tmp.path(), ".env", &kroki)
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains("Include file is not a PlantUML file (.puml, .iuml, ...): '.env'"),
        "{stderr}"
    );
    let requests = kroki.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(!requests[0].body.contains("secret"), "{requests:?}");
}

/// Unlike `rw.toml`, the command line is the user's own.
#[test]
fn render_reads_a_command_line_include_dir_outside_the_project() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let shared = tempfile::tempdir().expect("tempdir");
    std::fs::write(shared.path().join("common.iuml"), "Alice -> Bob\n").expect("write include");
    let kroki = KrokiStub::answering(Reply::Fixed {
        status: 200,
        body: PNG_HEADER,
    });

    let output = render_including(tmp.path(), "common.iuml", &kroki)
        .arg("-I")
        .arg(shared.path())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    assert!(output.status.success(), "{stderr}");
    let requests = kroki.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(requests[0].body.contains("Alice -> Bob"), "{requests:?}");
}
