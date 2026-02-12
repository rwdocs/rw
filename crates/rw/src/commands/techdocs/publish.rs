//! `rw techdocs publish` command implementation.

use std::path::PathBuf;

use clap::Args;

use crate::error::CliError;
use crate::output::Output;

/// Arguments for the techdocs publish command.
#[derive(Args)]
pub(crate) struct PublishArgs {
    /// Directory containing the built site.
    #[arg(short, long, default_value = "site")]
    directory: PathBuf,

    /// Backstage entity (e.g. "default/Component/arch").
    #[arg(long)]
    entity: String,

    /// S3 bucket name.
    #[arg(long)]
    bucket: String,

    /// S3-compatible endpoint URL (for non-AWS, e.g. Yandex Cloud).
    #[arg(long)]
    endpoint: Option<String>,

    /// AWS region.
    #[arg(long, default_value = "us-east-1")]
    region: String,

    /// Optional prefix path within the bucket.
    #[arg(long)]
    bucket_root_path: Option<String>,
}

impl PublishArgs {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let output = Output::new();
        output.info("rw techdocs publish: not yet implemented");
        Ok(())
    }
}
