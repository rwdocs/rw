//! `rw confluence render` command.

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf, absolute};
use std::sync::Arc;

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_confluence::{LocalImages, ProjectFiles, RenderOptions, RenderOutput, render};
use rw_site::{SiteModel, image_may_name};
use rw_storage::Storage;

use crate::commands::{ProjectArgs, build_site, build_storage, kroki_url_from_env, source_paths};
use crate::error::CliError;

/// Arguments for `rw confluence render`.
#[derive(Args)]
pub(crate) struct RenderArgs {
    /// Path to the markdown file to render.
    markdown_file: PathBuf,

    /// Bundle directory (or `-` for stdout-only mode).
    #[arg(long, value_name = "DIR_OR_DASH")]
    out: String,

    /// Kroki server URL (overrides `[diagrams].kroki_url` from `rw.toml`).
    #[arg(long)]
    kroki_url: Option<String>,

    /// `PlantUML` `!include` search directory. May be repeated.
    #[arg(short = 'I', long = "include-dir")]
    include_dirs: Vec<PathBuf>,

    /// Do not extract the title from the H1 that opens the page. Title extraction
    /// is enabled by default.
    #[arg(long)]
    no_extract_title: bool,

    /// Skip the Confluence TOC macro (TOC is prepended by default).
    #[arg(long)]
    no_toc: bool,

    /// Exit non-zero if any warning was emitted.
    #[arg(long)]
    strict: bool,

    #[command(flatten)]
    project: ProjectArgs,
}

impl RenderArgs {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let cli_settings = CliSettings {
            kroki_url: self.kroki_url,
            kroki_url_default: kroki_url_from_env(),
            ..Default::default()
        };
        // A file outside any project still renders, without its local images,
        // once the caller names a root on purpose.
        let config = if self.project.is_explicit() {
            self.project.load_any(Some(&cli_settings))?
        } else {
            self.project.load(Some(&cli_settings))?
        };

        let markdown =
            std::fs::read_to_string(&self.markdown_file).map_err(|source| CliError::Read {
                path: self.markdown_file.clone(),
                source,
            })?;

        let current_xhtml = read_current_xhtml_from_stdin()?;

        let to_stdout = self.out == "-";
        // Stdout can't carry image files, so with `--out -` they stay the
        // publisher's to upload rather than failing the render like diagrams.
        let images = local_images(&config, &self.markdown_file, !to_stdout);

        let model = absolute(&self.markdown_file)
            .is_ok_and(|file| !source_paths(&config, &file).is_empty())
            .then(|| Arc::new(build_site(&config)) as Arc<dyn SiteModel>);
        let opts = RenderOptions {
            kroki_url: config.diagrams.kroki_url,
            include_dirs: {
                let mut dirs = config.diagrams.include_dirs;
                dirs.extend(self.include_dirs);
                dirs
            },
            extract_title: !self.no_extract_title,
            prepend_toc: !self.no_toc,
            current_xhtml,
            images,
            model,
        };

        let tmp;
        let out_dir = if to_stdout {
            tmp = tempfile::tempdir()?;
            tmp.path()
        } else {
            Path::new(&self.out)
        };
        let result = render(&markdown, out_dir, &opts)?;

        if to_stdout {
            // `--out -` cannot stream PNG attachments over a pipe. If render()
            // produced any, the bundle wouldn't survive being piped — reject
            // after the fact rather than pre-scanning the markdown.
            if !result.attachments.is_empty() {
                return Err(CliError::OutStdoutHasAttachments {
                    count: result.attachments.len(),
                });
            }
            std::io::stdout()
                .lock()
                .write_all(result.xhtml.as_bytes())?;
        }

        print_diagnostics(&result, &mut std::io::stderr().lock());

        let count = result.warnings.len() + result.unmatched_comments.len();
        if self.strict && count > 0 {
            return Err(CliError::StrictWarnings { count });
        }
        Ok(())
    }
}

/// What [`render`] needs to place the local images of `markdown_file`, and to
/// bundle its image files if `bundle`, or `None` for a file outside the
/// project.
fn local_images(config: &Config, markdown_file: &Path, bundle: bool) -> Option<LocalImages> {
    let project_dir = &config.project_dir;
    let source_dir = &config.docs.source_dir;
    let storage = build_storage(config);
    let file = markdown_file.canonicalize().ok()?;
    // A source directory symlinked out of the project still serves its
    // pages' images, so a file under it is placed through it.
    let source_path = relative_path(&file, project_dir).or_else(|| {
        let in_source = relative_path(&file, source_dir)?;
        let source_in_project = storage.source_dir_in_project()?;
        Some(if source_in_project.is_empty() {
            in_source
        } else {
            format!("{source_in_project}/{in_source}")
        })
    })?;
    Some(LocalImages {
        files: ProjectFiles::new(image_may_name),
        source_path,
        storage: bundle.then(|| Arc::new(storage) as Arc<dyn Storage>),
        project_dir: project_dir.clone(),
    })
}

/// The canonical `file` relative to `dir`, `/`-separated, or `None` when it
/// is not under `dir`.
fn relative_path(file: &Path, dir: &Path) -> Option<String> {
    let dir = dir.canonicalize().ok()?;
    Some(
        file.strip_prefix(dir)
            .ok()?
            .components()
            .map(|component| component.as_os_str().to_str())
            .collect::<Option<Vec<_>>>()?
            .join("/"),
    )
}

/// Print title, warnings, and unmatched-comments to stderr as plain text.
///
/// Same format in bundle and stdout modes so publisher scripts can parse
/// stderr consistently regardless of `--out` value.
fn print_diagnostics(result: &RenderOutput, stderr: &mut impl Write) {
    if let Some(title) = &result.title {
        let _ = writeln!(stderr, "title: {title}");
    }
    // A warning can span lines, such as a Kroki error body. Indented, its
    // continuation lines cannot pass for diagnostics of their own, and a line
    // like `Error: Syntax error` cannot pass for rw's fatal error.
    for w in &result.warnings {
        let mut lines = w.lines();
        let _ = writeln!(stderr, "warning: {}", lines.next().unwrap_or_default());
        for line in lines {
            let _ = writeln!(stderr, "  {line}");
        }
    }
    if !result.unmatched_comments.is_empty() {
        let _ = writeln!(
            stderr,
            "{} comment(s) could not be placed:",
            result.unmatched_comments.len()
        );
        for c in &result.unmatched_comments {
            let _ = writeln!(stderr, r#"  - [{}] "{}""#, c.ref_id, c.text);
        }
    }
}

fn read_current_xhtml_from_stdin() -> Result<Option<String>, CliError> {
    if std::io::stdin().is_terminal() {
        return Ok(None);
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    if buf.is_empty() {
        Ok(None)
    } else {
        Ok(Some(buf))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// A Kroki error body spans lines, and one of them can read like rw's own
    /// fatal `Error:` line.
    #[test]
    fn a_multi_line_warning_prints_its_continuation_lines_indented() {
        let result = RenderOutput {
            xhtml: String::new(),
            title: None,
            attachments: Vec::new(),
            unmatched_comments: Vec::new(),
            warnings: Vec::from([
                "diagram 1: HTTP 400: Parse error on line 1:\r\n\
                 graph TD; A-->\n\
                 Error: Syntax error in graph\n"
                    .to_owned(),
                "diagram 2: one line".to_owned(),
            ]),
        };

        let mut out = Vec::new();
        print_diagnostics(&result, &mut out);

        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "warning: diagram 1: HTTP 400: Parse error on line 1:\n  \
             graph TD; A-->\n  \
             Error: Syntax error in graph\n\
             warning: diagram 2: one line\n",
        );
    }
}
