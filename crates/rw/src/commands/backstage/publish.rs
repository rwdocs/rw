//! `rw backstage publish` command implementation.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use rw_backstage::{BackstagePublisher, PublishConfig};
use rw_config::{CliSettings, Config};
use rw_storage::Storage;
use rw_storage_fs::FsStorage;

use crate::error::CliError;
use crate::output::Output;

/// Arguments for the backstage publish command.
#[derive(Args)]
pub(crate) struct PublishArgs {
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

    /// Override source directory.
    #[arg(short, long)]
    source_dir: Option<PathBuf>,

    /// Path to configuration file (default: auto-discover rw.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,
}

impl PublishArgs {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let output = Output::new();

        let cli_settings = CliSettings {
            source_dir: self.source_dir.clone(),
            ..CliSettings::default()
        };
        let config = Config::load(self.config.as_deref(), Some(&cli_settings))?;

        output.info(&format!(
            "Source: {}",
            config.docs_resolved.source_dir.display()
        ));
        output.info(&format!(
            "Publishing to s3://{}/{}",
            self.bucket, self.entity
        ));

        let storage: Arc<dyn Storage> = Arc::new(FsStorage::with_meta_filename(
            config.docs_resolved.source_dir.clone(),
            &config.metadata.name,
        ));

        let include_dirs = config.diagrams_resolved.include_dirs;

        let publish_config = PublishConfig {
            bucket: self.bucket,
            entity: self.entity,
            endpoint: self.endpoint,
            region: self.region,
            bucket_root_path: self.bucket_root_path,
        };
        let publisher = BackstagePublisher::new(publish_config);

        let rt = tokio::runtime::Runtime::new()?;
        let uploaded = rt.block_on(publisher.publish(storage.as_ref(), &include_dirs))?;

        output.success(&format!("Published {uploaded} files"));
        Ok(())
    }
}
