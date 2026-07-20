//! Public `render()` entry point for `rw-confluence`.
//!
//! Converts markdown into a publish-ready Confluence bundle on disk
//! (`page.xhtml` + diagram PNGs) and returns a [`RenderOutput`] for
//! in-process inspection.

use std::path::{Path, PathBuf};

use crate::comment_preservation::{UnmatchedComment, preserve_comments};
use crate::error::ConfluenceError;
use crate::renderer::PageRenderer;

/// Options for [`render`].
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Kroki server URL. When `None`, diagram code fences fall through to
    /// syntax-highlighted code (the same default as `rw serve`).
    pub kroki_url: Option<String>,
    /// Directories to search for `PlantUML` `!include` resolution.
    pub include_dirs: Vec<PathBuf>,
    /// DPI for diagram rendering. `None` means "use the renderer's
    /// Pull title from the first H1 heading. Default `false`.
    pub extract_title: bool,
    /// Prepend a Confluence TOC macro to the rendered XHTML. Default
    /// `false`.
    pub prepend_toc: bool,
    /// Current page's storage XHTML body. When provided, inline-comment
    /// markers are carried over from this XHTML into the freshly rendered
    /// XHTML. When `None`, no preservation is attempted.
    pub current_xhtml: Option<String>,
}

/// Output produced by [`render`].
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// The rendered XHTML body that was written to `<out_dir>/page.xhtml`.
    /// Returned for callers that want to inspect it without re-reading
    /// the file.
    pub xhtml: String,
    /// Extracted title from the first H1, or `None` if
    /// `extract_title=false` or the markdown had no H1.
    pub title: Option<String>,
    /// Diagram PNG filenames written to `<out_dir>`, sorted
    /// alphabetically. Empty when no diagrams were rendered.
    pub attachments: Vec<String>,
    /// Comment markers from `current_xhtml` that could not be re-placed in
    /// the new XHTML.
    pub unmatched_comments: Vec<UnmatchedComment>,
    /// Non-fatal warnings: renderer warnings and preservation parse
    /// failures. Concatenation of `PageRenderer` and `preserve_comments`
    /// warnings.
    pub warnings: Vec<String>,
}

/// Render `markdown` into the Confluence bundle layout at `out_dir`.
///
/// Writes `<out_dir>/page.xhtml` and one PNG per diagram. Creates
/// `out_dir` (and parents) if absent.
///
/// # Errors
///
/// Returns [`ConfluenceError::Io`] if directory creation or file writes
/// fail.
// `opts` is owned by value: callers typically construct it at the call site and
// passing by value is the ergonomic default for builder-style option structs.
#[allow(clippy::needless_pass_by_value)]
pub fn render(
    markdown: &str,
    out_dir: &Path,
    opts: RenderOptions,
) -> Result<RenderOutput, ConfluenceError> {
    std::fs::create_dir_all(out_dir)?;

    // Remove stale PNGs left over from a previous render so the post-render
    // directory scan only sees attachments produced by this invocation.
    // Errors are intentionally ignored: a locked or vanished file just means
    // the post-scan reflects truth (we'll re-list what's actually on disk).
    if let Ok(entries) = std::fs::read_dir(out_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_file = entry.file_type().is_ok_and(|t| t.is_file());
            if is_file && path.extension().is_some_and(|ext| ext == "png") {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    let page_renderer = PageRenderer::new()
        .prepend_toc(opts.prepend_toc)
        .extract_title(opts.extract_title)
        .include_dirs(opts.include_dirs);

    let render_result = page_renderer.render(markdown, opts.kroki_url.as_deref(), Some(out_dir));

    let mut warnings = render_result.warnings;
    let (final_xhtml, unmatched_comments) = if let Some(current) = opts.current_xhtml.as_deref() {
        let preserve_result = preserve_comments(current, &render_result.html);
        warnings.extend(preserve_result.warnings);
        (preserve_result.html, preserve_result.unmatched_comments)
    } else {
        (render_result.html, Vec::new())
    };

    // Collect attachments: scan out_dir for PNGs written by DiagramProcessor.
    let mut attachments: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(out_dir)? {
        let entry = entry?;
        let is_file = entry.file_type().is_ok_and(|t| t.is_file());
        let path = entry.path();
        if is_file
            && path.extension().is_some_and(|ext| ext == "png")
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            attachments.push(name.to_owned());
        }
    }
    attachments.sort();

    std::fs::write(out_dir.join("page.xhtml"), &final_xhtml)?;

    Ok(RenderOutput {
        xhtml: final_xhtml,
        title: render_result.title,
        attachments,
        unmatched_comments,
        warnings,
    })
}
