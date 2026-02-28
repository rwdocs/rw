//! `rw backstage publish` command implementation.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_storage::Storage;
use rw_storage_fs::FsStorage;
use rw_storage_s3::BundlePublisher;

use crate::commands::S3Args;
use crate::error::CliError;
use crate::output::Output;

/// Arguments for the backstage publish command.
#[derive(Args)]
pub(crate) struct PublishArgs {
    #[command(flatten)]
    s3: S3Args,

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
            source_dir: self.source_dir,
            ..CliSettings::default()
        };
        let config = Config::load(self.config.as_deref(), Some(&cli_settings))?;

        output.info(&format!(
            "Source: {}",
            config.docs_resolved.source_dir.display()
        ));
        output.info(&format!(
            "Publishing to s3://{}/{}",
            self.s3.bucket, self.s3.entity
        ));

        let storage: Arc<dyn Storage> = Arc::new(FsStorage::with_meta_filename(
            config.docs_resolved.source_dir.clone(),
            &config.metadata.name,
        ));

        let include_dirs = config.diagrams_resolved.include_dirs;
        let publisher = BundlePublisher::new(self.s3.into_config());

        let rt = tokio::runtime::Runtime::new()?;
        let uploaded = rt.block_on(publisher.publish(storage.as_ref(), &include_dirs))?;

        output.success(&format!("Published {uploaded} files"));
        Ok(())
    }
}
