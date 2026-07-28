//! Integration tests for `rw confluence render`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
fn render_with_stdin_xhtml_preserves_comment_marker() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "Hello marked text here.\n";
    let out_dir = tmp.path().join("dist");

    let mut child = rw_render(tmp.path(), md)
        .arg("--out")
        .arg(&out_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn rw");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(
            b"<p>Hello <ac:inline-comment-marker ac:ref=\"abc\">marked text\
              </ac:inline-comment-marker> here.</p>",
        )
        .expect("write stdin");

    let status = child.wait().expect("wait");
    assert!(status.success(), "exit: {status:?}");

    let xhtml = std::fs::read_to_string(out_dir.join("page.xhtml")).expect("page.xhtml");
    assert!(xhtml.contains("ac:inline-comment-marker"));
    assert!(xhtml.contains(r#"ac:ref="abc""#));
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

#[test]
fn render_strict_exits_1_when_warning_emitted() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "# T\n\nBody.\n";
    let out_dir = tmp.path().join("dist");

    // Malformed current_xhtml on stdin -> comment_preservation emits a
    // "comment preservation skipped" warning, which --strict promotes to
    // exit 1.
    let mut child = rw_render(tmp.path(), md)
        .arg("--out")
        .arg(&out_dir)
        .arg("--strict")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rw");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"<p>unclosed paragraph")
        .expect("write stdin");

    let output = child.wait_with_output().expect("wait");
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
fn render_dir_mode_prints_unmatched_comment_count_header_to_stderr() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "Completely different content now.\n";
    let out_dir = tmp.path().join("dist");

    let mut child = rw_render(tmp.path(), md)
        .arg("--out")
        .arg(&out_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rw");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(
            b"<p><ac:inline-comment-marker ac:ref=\"abc\">Original sentence here\
              </ac:inline-comment-marker></p>",
        )
        .expect("write stdin");

    let output = child.wait_with_output().expect("wait");
    assert!(output.status.success(), "exit: {:?}", output.status);

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("1 comment(s) could not be placed:"),
        "missing header, stderr: {stderr}"
    );
    assert!(stderr.contains("[abc]"), "missing ref id, stderr: {stderr}");
}

#[test]
fn render_stdout_mode_prints_unmatched_comment_count_header_to_stderr() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "Completely different content now.\n";

    let mut child = rw_render(tmp.path(), md)
        .arg("--out")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rw");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(
            b"<p><ac:inline-comment-marker ac:ref=\"xyz\">Original sentence here\
              </ac:inline-comment-marker></p>",
        )
        .expect("write stdin");

    let output = child.wait_with_output().expect("wait");
    assert!(output.status.success(), "exit: {:?}", output.status);

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("1 comment(s) could not be placed:"),
        "missing header, stderr: {stderr}"
    );
    assert!(stderr.contains("[xyz]"), "missing ref id, stderr: {stderr}");
}

#[test]
fn render_strict_exits_1_on_unmatched_comment_even_with_no_warnings() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = "Completely different content now.\n";
    let out_dir = tmp.path().join("dist");

    let mut child = rw_render(tmp.path(), md)
        .arg("--out")
        .arg(&out_dir)
        .arg("--strict")
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn rw");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(
            b"<p><ac:inline-comment-marker ac:ref=\"abc\">Original sentence here\
              </ac:inline-comment-marker></p>",
        )
        .expect("write stdin");

    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(1), "expected exit 1, got {status:?}");
}

#[test]
fn render_stdout_mode_keeps_diagnostics_off_stdout() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Trigger title and unmatched-comment diagnostics. (Warnings flow through
    // the same `print_diagnostics` writer; the dedicated
    // `render_strict_exits_1_when_warning_emitted` test covers the warning
    // path.)
    let md = "# Stdout title\n\nDifferent content here.\n";

    let mut child = rw_render(tmp.path(), md)
        .arg("--out")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rw");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(
            b"<p><ac:inline-comment-marker ac:ref=\"abc\">Original sentence here\
              </ac:inline-comment-marker></p>",
        )
        .expect("write stdin");

    let output = child.wait_with_output().expect("wait");
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
        stderr.contains("could not be placed"),
        "stderr missing unmatched: {stderr}"
    );
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

    let mut child = rw_render(tmp.path(), md)
        .arg("--out")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rw");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(
            b"<p>Hello <ac:inline-comment-marker ac:ref=\"abc\">marked text\
              </ac:inline-comment-marker> here.</p>",
        )
        .expect("write stdin");

    let output = child.wait_with_output().expect("wait");
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
