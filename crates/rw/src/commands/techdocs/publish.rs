//! `rw techdocs publish` command implementation.

use std::path::PathBuf;

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_techdocs::{PublishConfig, S3Publisher};

use crate::error::CliError;
use crate::output::Output;

/// Arguments for the techdocs publish command.
#[derive(Args)]
pub(crate) struct PublishArgs {
    /// Directory containing the built site (default: .rw/techdocs/build/).
    #[arg(short, long)]
    directory: Option<PathBuf>,

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
        let uploaded = rt.block_on(publisher.publish(&directory))?;

        output.success(&format!("Published {uploaded} files"));
        Ok(())
    }
}
