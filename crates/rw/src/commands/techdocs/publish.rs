//! `rw techdocs publish` command implementation.

use std::path::PathBuf;

use clap::Args;
use rw_techdocs::{PublishConfig, S3Publisher};

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

        output.info(&format!(
            "Publishing {} to s3://{}/{}",
            self.directory.display(),
            self.bucket,
            self.entity
        ));

        let publish_config = PublishConfig {
            bucket: self.bucket,
            entity: self.entity,
            endpoint: self.endpoint,
            region: self.region,
            bucket_root_path: self.bucket_root_path,
        };
        let publisher = S3Publisher::new(publish_config);

        let rt = tokio::runtime::Runtime::new()?;
        let uploaded = rt.block_on(publisher.publish(&self.directory))?;

        output.success(&format!("Published {uploaded} files"));
        Ok(())
    }
}
