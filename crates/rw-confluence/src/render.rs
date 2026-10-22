//! Public `render()` entry point for `rw-confluence`.
//!
//! Converts markdown into a publish-ready Confluence bundle on disk
//! (`page.xhtml`, diagram PNGs and local images) and returns a
//! [`RenderOutput`] for in-process inspection.

use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rw_diagrams::{Providers, SiteModel};
use rw_kroki::KrokiProvider;

use crate::comment_preservation::{UnmatchedComment, preserve_comments};
use crate::images::{LocalImages, is_attachment_name};
use crate::renderer::render_page;

/// Options for [`render`].
#[derive(Default)]
pub struct RenderOptions {
    /// Kroki server URL. When `None`, diagram code fences fall through to
    /// syntax-highlighted code (the same default as `rw serve`).
    pub kroki_url: Option<String>,
    /// Directories to search for `PlantUML` `!include` resolution.
    pub include_dirs: Vec<PathBuf>,
    /// Pull title from the H1 that opens the page. Default `false`.
    pub extract_title: bool,
    /// Prepend a Confluence TOC macro to the rendered XHTML. Default
    /// `false`.
    pub prepend_toc: bool,
    /// Current page's storage XHTML body. When provided, inline-comment
    /// markers are carried over from this XHTML into the freshly rendered
    /// XHTML. When `None`, no preservation is attempted.
    pub current_xhtml: Option<String>,
    /// Where to find the files local images name. When `None`, every local
    /// image, diagram files included, references an attachment named after
    /// its file, which the publisher uploads.
    pub images: Option<LocalImages>,
    /// The site the page belongs to, for diagrams that reference its
    /// entities, such as `PlantUML` metadata includes. When `None`, those
    /// stay unexpanded.
    pub model: Option<Arc<dyn SiteModel>>,
}

/// Output produced by [`render`].
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// The rendered XHTML body that was written to `<out_dir>/page.xhtml`.
    /// Returned for callers that want to inspect it without re-reading
    /// the file.
    pub xhtml: String,
    /// Extracted title from the opening H1, or `None` if
    /// `extract_title=false` or the markdown had no H1.
    pub title: Option<String>,
    /// Attachment filenames written to `<out_dir>` by this call, diagrams
    /// and local images, sorted alphabetically and deduplicated.
    ///
    /// Exact: a file already in `<out_dir>` that this render did not produce is
    /// never listed. Note [`render`] also deletes every `diagram_*.png` and
    /// `image_<hash>.<ext>` file in `<out_dir>` before rendering.
    pub attachments: Vec<String>,
    /// Comment markers from `current_xhtml` that could not be re-placed in
    /// the new XHTML.
    pub unmatched_comments: Vec<UnmatchedComment>,
    /// Non-fatal warnings: renderer warnings, diagrams that failed to render
    /// or be written, attachments the bundle lacks, links to project files,
    /// which stay as written, and preservation parse failures.
    pub warnings: Vec<String>,
}

/// Render `markdown` into the Confluence bundle layout at `out_dir`.
///
/// Writes `<out_dir>/page.xhtml`, one PNG per diagram and, with
/// [`RenderOptions::images`], one file per local image. Creates `out_dir`
/// (and parents) if absent.
///
/// # Errors
///
/// Returns an I/O error naming the path if creating `out_dir` or writing
/// `page.xhtml` fails; its `source()` is the underlying I/O error. A diagram
/// that fails to render, or whose PNG cannot be written, is not an error: it
/// renders as an error figure and adds a warning.
pub fn render(markdown: &str, out_dir: &Path, opts: &RenderOptions) -> io::Result<RenderOutput> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| with_path(e, "cannot create directory", out_dir))?;

    // `docs/confluence.md` has publishers upload every file in the bundle
    // directory, so an edited or deleted diagram or image would leave a stale
    // attachment that publishes as a phantom. Other files are the caller's.
    //
    // Errors are intentionally ignored: a locked or vanished file is not a
    // reason to fail the render.
    if let Ok(entries) = std::fs::read_dir(out_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let ours = (name.starts_with("diagram_") && name.ends_with(".png"))
                || is_attachment_name(&name);
            if ours && entry.file_type().is_ok_and(|t| t.is_file()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    // Built per render rather than held: `render` is a one-shot entry point, so
    // there is no second page for a pooled HTTP agent to serve.
    let providers = match &opts.kroki_url {
        Some(url) => {
            let mut kroki = KrokiProvider::new(url).include_dirs(&opts.include_dirs);
            if let Some(images) = &opts.images {
                kroki = kroki.project_dir(images.project_dir.clone());
            }
            Providers::empty().with(Arc::new(kroki))
        }
        None => Providers::empty(),
    };

    let (render_result, attachments) = render_page(markdown, &providers, out_dir, opts);

    let mut warnings = render_result.warnings;
    let (final_xhtml, unmatched_comments) = if let Some(current) = opts.current_xhtml.as_deref() {
        let preserve_result = preserve_comments(current, &render_result.html);
        warnings.extend(preserve_result.warnings);
        (preserve_result.html, preserve_result.unmatched_comments)
    } else {
        (render_result.html, Vec::new())
    };

    let page_path = out_dir.join("page.xhtml");
    std::fs::write(&page_path, &final_xhtml)
        .map_err(|e| with_path(e, "cannot write", &page_path))?;

    Ok(RenderOutput {
        xhtml: final_xhtml,
        title: render_result.title.filter(|_| opts.extract_title),
        attachments,
        unmatched_comments,
        warnings,
    })
}

fn with_path(err: io::Error, action: &'static str, path: &Path) -> io::Error {
    io::Error::new(
        err.kind(),
        PathError {
            action,
            path: path.to_owned(),
            source: err,
        },
    )
}

#[derive(Debug)]
struct PathError {
    action: &'static str,
    path: PathBuf,
    source: io::Error,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.action, self.path.display())
    }
}

impl Error for PathError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}
