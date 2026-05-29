//! Integration tests for `rw confluence render`.

use std::io::Write;
use std::process::{Command, Stdio};

/// Path to the `rw` binary built by Cargo.
///
/// `CARGO_BIN_EXE_rw` is set by Cargo for integration tests and points to
/// the binary under test without relying on `current_exe()` heuristics.
fn rw_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rw")
}

fn write_markdown(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, content).expect("write markdown");
    p
}

#[test]
fn render_bundle_mode_writes_page_xhtml_and_emits_title_to_stderr() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let md = write_markdown(tmp.path(), "in.md", "# Title\n\nBody.\n");
    let out_dir = tmp.path().join("dist");

    let output = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(tmp.path(), "in.md", "# Stdout title\n\nHello.\n");

    let output = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let kroki_url =
        std::env::var("KROKI_URL").expect("set KROKI_URL=https://kroki.io to run this test");

    let tmp = tempfile::tempdir().expect("tempdir");
    let md = write_markdown(
        tmp.path(),
        "in.md",
        "# Diag\n\n```mermaid\ngraph TD\nA-->B\n```\n",
    );

    let output = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(tmp.path(), "in.md", "Hello marked text here.\n");
    let out_dir = tmp.path().join("dist");

    let mut child = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(
        tmp.path(),
        "in.md",
        "# Title\n\n```mermaid\ngraph TD\nA-->B\n```\n",
    );

    // No --kroki-url and no [diagrams] in rw.toml → diagrams fall through to
    // syntax-highlighted code blocks; no attachments are produced, so the
    // post-render attachments guard does not fire and --out - succeeds.
    let output = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(tmp.path(), "in.md", "# T\n\nBody.\n");
    let out_dir = tmp.path().join("dist");

    // Malformed current_xhtml on stdin -> comment_preservation emits a
    // "comment preservation skipped" warning, which --strict promotes to
    // exit 1.
    let mut child = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(tmp.path(), "in.md", "Completely different content now.\n");
    let out_dir = tmp.path().join("dist");

    let mut child = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(tmp.path(), "in.md", "Completely different content now.\n");

    let mut child = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(tmp.path(), "in.md", "Completely different content now.\n");
    let out_dir = tmp.path().join("dist");

    let mut child = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(
        tmp.path(),
        "in.md",
        // Trigger title and unmatched-comment diagnostics. (Warnings flow
        // through the same `print_diagnostics` writer; the dedicated
        // `render_strict_exits_1_when_warning_emitted` test covers the
        // warning path.)
        "# Stdout title\n\nDifferent content here.\n",
    );

    let mut child = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(tmp.path(), "in.md", "# Title\n\nBody.\n");
    let out_dir = tmp.path().join("dist");

    let output = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(tmp.path(), "in.md", "# Title\n\n## Sub\n\nBody.\n");
    let out_dir = tmp.path().join("dist");

    let status = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
    let md = write_markdown(tmp.path(), "in.md", "Hello marked text here.\n");

    let mut child = Command::new(rw_bin())
        .arg("confluence")
        .arg("render")
        .arg(&md)
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
