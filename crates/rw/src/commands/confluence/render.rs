//! `rw confluence render` command.

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_confluence::{RenderOptions, RenderOutput, render};

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

    /// Do not extract the title from the first H1 heading. Title extraction
    /// is enabled by default.
    #[arg(long)]
    no_extract_title: bool,

    /// Skip the Confluence TOC macro (TOC is prepended by default).
    #[arg(long)]
    no_toc: bool,

    /// Exit non-zero if any warning was emitted.
    #[arg(long)]
    strict: bool,

    /// Path to `rw.toml` (default: auto-discover).
    #[arg(short, long)]
    config: Option<PathBuf>,
}

impl RenderArgs {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        // Load `rw.toml` for [diagrams] defaults.
        let cli_settings = CliSettings {
            kroki_url: self.kroki_url,
            ..Default::default()
        };
        let config = Config::load(self.config.as_deref(), Some(&cli_settings))?;

        let markdown =
            std::fs::read_to_string(&self.markdown_file).map_err(|source| CliError::Read {
                path: self.markdown_file.clone(),
                source,
            })?;

        let current_xhtml = read_current_xhtml_from_stdin()?;

        let opts = RenderOptions {
            kroki_url: config.diagrams_resolved.kroki_url,
            include_dirs: {
                let mut dirs = config.diagrams_resolved.include_dirs;
                dirs.extend(self.include_dirs);
                dirs
            },
            extract_title: !self.no_extract_title,
            prepend_toc: !self.no_toc,
            current_xhtml,
        };

        let to_stdout = self.out == "-";
        let tmp;
        let out_dir = if to_stdout {
            tmp = tempfile::tempdir()?;
            tmp.path()
        } else {
            Path::new(&self.out)
        };
        let result = render(&markdown, out_dir, opts)?;

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

        print_diagnostics(&result);

        let count = result.warnings.len() + result.unmatched_comments.len();
        if self.strict && count > 0 {
            return Err(CliError::StrictWarnings { count });
        }
        Ok(())
    }
}

/// Print title, warnings, and unmatched-comments to stderr as plain text.
///
/// Same format in bundle and stdout modes so publisher scripts can parse
/// stderr consistently regardless of `--out` value.
fn print_diagnostics(result: &RenderOutput) {
    let mut stderr = std::io::stderr().lock();
    if let Some(title) = &result.title {
        let _ = writeln!(stderr, "title: {title}");
    }
    for w in &result.warnings {
        let _ = writeln!(stderr, "warning: {w}");
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
