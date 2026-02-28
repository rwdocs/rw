//! `rw techdocs publish` command implementation.

use std::path::PathBuf;

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_techdocs::S3Publisher;

use crate::commands::S3Args;
use crate::error::CliError;
use crate::output::Output;

/// Arguments for the techdocs publish command.
#[derive(Args)]
pub(crate) struct PublishArgs {
    /// Directory containing the built site (default: .rw/techdocs/build/).
    #[arg(short, long)]
    directory: Option<PathBuf>,

    #[command(flatten)]
    s3: S3Args,

    /// Path to configuration file (default: auto-discover rw.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,
}

impl PublishArgs {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let output = Output::new();

        let config = Config::load(self.config.as_deref(), Some(&CliSettings::default()))?;

        let directory = self
            .directory
            .unwrap_or_else(|| config.docs_resolved.project_dir.join("techdocs/build"));

        output.info(&format!(
            "Publishing {} to s3://{}/{}",
            directory.display(),
            self.s3.bucket,
            self.s3.entity
        ));

        let publisher = S3Publisher::new(self.s3.into_config());

        let rt = tokio::runtime::Runtime::new()?;
        let uploaded = rt.block_on(publisher.publish(&directory))?;

        output.success(&format!("Published {uploaded} files"));
        Ok(())
    }
}
