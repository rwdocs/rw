//! `rw backstage publish` command implementation.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_storage::Storage;
use rw_storage_fs::{FsStorage, MtimeSource};
use rw_storage_s3::{BundlePublisher, PublishReport};

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

    /// Exit with a non-zero status when diagram warnings are emitted.
    ///
    /// Bundles are still uploaded — strict mode only affects the exit code.
    #[arg(long)]
    strict: bool,
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

        // Publish bakes git-commit times into the S3 manifest, so they stay
        // stable across CI checkouts (fs mtime would be the meaningless
        // checkout time). FsStorage defaults to filesystem mtime, so git is
        // selected explicitly here.
        let storage: Arc<dyn Storage> = Arc::new(
            FsStorage::with_meta_filename(
                config.docs_resolved.source_dir.clone(),
                &config.metadata.name,
            )
            .with_mtime_source(MtimeSource::Git),
        );

        let include_dirs = config.diagrams_resolved.include_dirs;
        let publisher = BundlePublisher::new(self.s3.into_config());

        let rt = tokio::runtime::Runtime::new()?;
        let report = rt.block_on(publisher.publish(storage.as_ref(), &include_dirs))?;

        finish_publish(&report, self.strict, &output)
    }
}

/// Print the publish summary, surface any diagram warnings, and decide the
/// exit status based on `--strict`.
///
/// Extracted as a free function so it can be unit-tested without S3 access.
fn finish_publish(report: &PublishReport, strict: bool, output: &Output) -> Result<(), CliError> {
    output.success(&format!("Published {} files", report.uploaded));

    if !report.warnings.is_empty() {
        output.warning(&format!("Diagram warnings ({}):", report.warnings.len()));
        for w in &report.warnings {
            output.warning(&format!("  - {w}"));
        }
    }

    if strict && !report.warnings.is_empty() {
        return Err(CliError::DiagramWarningsInStrictMode {
            count: report.warnings.len(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(uploaded: usize, warnings: &[&str]) -> PublishReport {
        PublishReport {
            uploaded,
            warnings: warnings.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn finish_publish_ok_when_no_warnings_and_not_strict() {
        let output = Output::new();
        let result = finish_publish(&report(3, &[]), false, &output);
        assert!(result.is_ok());
    }

    #[test]
    fn finish_publish_ok_when_no_warnings_and_strict() {
        let output = Output::new();
        let result = finish_publish(&report(3, &[]), true, &output);
        assert!(result.is_ok());
    }

    #[test]
    fn finish_publish_ok_when_warnings_and_not_strict() {
        let output = Output::new();
        let result = finish_publish(&report(3, &["bad include"]), false, &output);
        assert!(
            result.is_ok(),
            "warnings without --strict must not fail the run",
        );
    }

    #[test]
    fn finish_publish_errors_when_warnings_and_strict() {
        let output = Output::new();
        let result = finish_publish(&report(3, &["bad include", "unknown attr"]), true, &output);
        match result {
            Err(CliError::DiagramWarningsInStrictMode { count }) => {
                assert_eq!(count, 2);
            }
            other => panic!("expected DiagramWarningsInStrictMode, got {other:?}"),
        }
    }
}
