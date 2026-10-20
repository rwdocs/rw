//! Integration tests for `rw confluence render`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod common;

use common::rw_cmd;

/// Writes an empty `rw.toml` into `dir` and returns its path, for commands that
/// locate their project by discovery rather than by an explicit root.
///
/// `Config::discover_config` searches *upward* without bound, so running a
/// child in a temporary directory only means "not this repository", not "no
/// config" — an `rw.toml` above `dir` still wins. This file is what stops the
/// search, since discovery halts at its first hit. Pass the returned path to
/// the subcommand's `--config` as well, to name it outright instead of relying
/// on where discovery starts.
pub fn empty_config(dir: &Path) -> PathBuf {
    let path = dir.join("rw.toml");
    std::fs::write(&path, "").expect("write empty rw.toml");
    path
}

/// Writes `markdown` into `dir` and returns a hermetic [`rw_cmd`] that renders
/// it, with config discovery pinned to an empty `rw.toml` in `dir`. Callers
/// append their own `--out` and any flags under test.
fn rw_render(dir: &Path, markdown: &str) -> Command {
    let source = dir.join("in.md");
    std::fs::write(&source, markdown).expect("write markdown");

    let mut cmd = rw_cmd();
    cmd.current_dir(dir)
        .arg("confluence")
        .arg("render")
        .arg(&source)
        .arg("--config")
        .arg(empty_config(dir));
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
fn render_stdout_mode_writes_body_to_stdout() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# Stdout title\n\nHello.\n";

    let output = rw_render(tmp.path(), md)
        .arg("--out")
        .arg("-")
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    assert!(output.status.success(), "exit: {:?}", output.status);

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Hello"), "stdout: {stdout}");

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("title: Stdout title"),
        "stderr did not contain title: {stderr}"
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
fn render_stdout_mode_allows_diagrams_when_no_kroki_url() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# Title\n\n```mermaid\ngraph TD\nA-->B\n```\n";

    // No --kroki-url and no [diagrams] in rw.toml → diagrams fall through to
    // syntax-highlighted code blocks; no attachments are produced, so the
    // post-render attachments guard does not fire and --out - succeeds.
    //
    // "No kroki url" covers the child's whole configuration, not just its
    // arguments: `rw_cmd`'s cleared environment is what keeps an exported
    // `RW_DIAGRAMS_KROKI_URL` from supplying one.
    let output = rw_render(tmp.path(), md)
        .arg("--out")
        .arg("-")
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");

    assert!(
        output.status.success(),
        "expected success, exit: {:?}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The only test that has `rw` read `RW_DIAGRAMS_KROKI_URL` from its
/// environment; the fallback itself is tested in-process with an injected
/// lookup. An invalid value proves the read without contacting a Kroki server.
#[test]
fn render_reads_kroki_url_fallback_from_the_environment() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let output = rw_render(tmp.path(), "# Title\n\nBody.\n")
        .arg("--out")
        .arg("-")
        .env("RW_DIAGRAMS_KROKI_URL", "not-a-url")
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(!output.status.success());
    assert!(
        stderr.contains("diagrams.kroki_url must start with http"),
        "{stderr}"
    );
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
fn render_strict_accepts_bare_colon_words_in_prose() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "Use the :id key. Check the :status of the build.\n";

    let output = rw_render(tmp.path(), md)
        .arg("--out")
        .arg("-")
        .arg("--strict")
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        output.status.success(),
        "exit: {:?}\n{stderr}",
        output.status
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Use the :id key. Check the :status of the build."),
        "stdout: {stdout}"
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
fn render_stdout_mode_with_malformed_stdin_writes_only_the_body() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# T\n\nBody.\n";

    let fresh = rw_render(tmp.path(), md)
        .arg("--out")
        .arg("-")
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    assert!(fresh.status.success(), "exit: {:?}", fresh.status);

    let output = run_with_stdin(
        rw_render(tmp.path(), md).arg("--out").arg("-"),
        b"<p>unclosed paragraph",
    );
    assert!(output.status.success(), "exit: {:?}", output.status);

    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        String::from_utf8(fresh.stdout).expect("utf8 stdout"),
    );
}

#[test]
fn render_dir_mode_prints_unmatched_comment_count_header_to_stderr() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "Completely different content now.\n";
    let out_dir = tmp.path().join("dist");

    let output = run_with_stdin(
        rw_render(tmp.path(), md).arg("--out").arg(&out_dir),
        b"<p><ac:inline-comment-marker ac:ref=\"abc\">Original sentence here\
              </ac:inline-comment-marker></p>",
    );
    assert!(output.status.success(), "exit: {:?}", output.status);

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("1 comment(s) could not be placed:"),
        "missing header, stderr: {stderr}"
    );
    assert!(stderr.contains("[abc]"), "missing ref id, stderr: {stderr}");
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
fn render_stdout_mode_with_stdin_preserves_comment_marker() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "Hello marked text here.\n";

    let output = run_with_stdin(
        rw_render(tmp.path(), md).arg("--out").arg("-"),
        b"<p>Hello <ac:inline-comment-marker ac:ref=\"abc\">marked text\
              </ac:inline-comment-marker> here.</p>",
    );
    assert!(output.status.success(), "exit: {:?}", output.status);

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("ac:inline-comment-marker"),
        "stdout missing marker: {stdout}"
    );
    assert!(
        stdout.contains(r#"ac:ref="abc""#),
        "stdout missing ref: {stdout}"
    );
}

#[test]
fn render_bundle_write_error_names_the_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_dir = tmp.path().join("dist");
    let page = out_dir.join("page.xhtml");
    std::fs::create_dir_all(&page).expect("blocking dir");

    let output = rw_render(tmp.path(), "# T\n")
        .arg("--out")
        .arg(&out_dir)
        .stdin(Stdio::null())
        .output()
        .expect("spawn rw");
    assert_eq!(output.status.code(), Some(1), "exit: {:?}", output.status);

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains(&format!("Error: cannot write {}: ", page.display())),
        "{stderr}"
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
        .arg("--config")
        .arg(empty_config(tmp.path()))
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
    let config = tmp.path().join("rw.toml");
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
