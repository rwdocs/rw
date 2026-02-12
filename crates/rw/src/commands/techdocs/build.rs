//! `rw techdocs build` command implementation.

use std::path::PathBuf;

use clap::Args;

use crate::error::CliError;
use crate::output::Output;

/// Arguments for the techdocs build command.
#[derive(Args)]
pub(crate) struct BuildArgs {
    /// Output directory for the generated site.
    #[arg(short, long, default_value = "site")]
    output_dir: PathBuf,

    /// Markdown source directory (overrides config).
    #[arg(short, long)]
    source_dir: Option<PathBuf>,

    /// Kroki server URL for diagram rendering (overrides config).
    #[arg(long)]
    kroki_url: Option<String>,

    /// Site name for techdocs_metadata.json.
    #[arg(long)]
    site_name: Option<String>,

    /// Disable caching.
    #[arg(long)]
    no_cache: bool,

    /// Path to configuration file (default: auto-discover rw.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,
}

impl BuildArgs {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let output = Output::new();
        output.info("rw techdocs build: not yet implemented");
        Ok(())
    }
}
