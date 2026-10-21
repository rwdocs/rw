//! `rw backstage publish` command implementation.

use std::collections::BTreeSet;
use std::sync::Arc;

use clap::Args;
use rw_cache::NullCache;
use rw_site::{PageRendererConfig, Site};
use rw_storage::Storage;
use rw_storage_fs::{FsStorage, MtimeSource};
use rw_storage_s3::{BundlePublisher, PublishReport, S3Config};

use crate::commands::ProjectArgs;
use crate::error::CliError;
use crate::output::Output;

/// Arguments for the backstage publish command.
#[derive(Args)]
pub(crate) struct PublishArgs {
    #[command(flatten)]
    s3: S3Args,

    #[command(flatten)]
    project: ProjectArgs,

    /// Exit with a non-zero status when warnings are emitted (broken diagram
    /// includes, images that could not be published).
    ///
    /// Bundles are still uploaded — strict mode only affects the exit code.
    #[arg(long)]
    strict: bool,
}

impl PublishArgs {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let output = Output::new();

        let config = self.project.load(None)?;

        output.info(&format!("Source: {}", config.docs.source_dir.display()));
        output.info(&format!(
            "Publishing to s3://{}/{}",
            self.s3.bucket, self.s3.entity
        ));

        // Publish bakes git-commit times into the S3 manifest, so they stay
        // stable across CI checkouts (fs mtime would be the meaningless
        // checkout time). One storage for both passes, so each metadata problem
        // is logged once.
        let storage = Arc::new(
            FsStorage::new(config.project_dir.clone(), config.docs.source_dir.clone())
                .with_mtime_source(MtimeSource::Git),
        );
        let (files, image_warnings) = shown_files(Arc::clone(&storage) as _)?;

        let include_dirs = config.diagrams.include_dirs;
        let publisher = BundlePublisher::new(self.s3.into_config());

        let rt = tokio::runtime::Runtime::new()?;
        let mut report = rt.block_on(publisher.publish(&*storage, &include_dirs, &files))?;
        report.warnings.extend(image_warnings);

        finish_publish(&report, self.strict, &output)
    }
}

/// The project files that pages show, found or not, and a warning, prefixed
/// with the page, for each image the renderer refused to place.
///
/// Renders every page through [`Site`], as the Backstage reader does over the
/// published bundle, so the paths are the ones it will look up. Diagrams are
/// off, so nothing reaches the network.
fn shown_files(storage: Arc<dyn Storage>) -> Result<(BTreeSet<String>, Vec<String>), CliError> {
    let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());
    let mut files = BTreeSet::new();
    let mut warnings = BTreeSet::new();
    for page in site.list_pages()? {
        if !page.has_content {
            continue;
        }
        let render = site.render(&page.path)?;
        warnings.extend(
            render
                .image_refusals
                .into_iter()
                .map(|warning| format!("/{}: {warning}", page.path)),
        );
        files.extend(render.files);
    }
    Ok((files, warnings.into_iter().collect()))
}

#[derive(Args)]
struct S3Args {
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

impl S3Args {
    fn into_config(self) -> S3Config {
        S3Config {
            bucket: self.bucket,
            prefix: self.entity,
            region: self.region,
            endpoint: self.endpoint,
            bucket_root_path: self.bucket_root_path,
            access_key_id: None,
            secret_access_key: None,
        }
    }
}

/// Print the publish summary, surface any warnings, and decide the
/// exit status based on `--strict`.
///
/// Extracted as a free function so it can be unit-tested without S3 access.
fn finish_publish(report: &PublishReport, strict: bool, output: &Output) -> Result<(), CliError> {
    output.success(&format!("Published {} files", report.uploaded));

    if !report.warnings.is_empty() {
        output.warning(&format!("Warnings ({}):", report.warnings.len()));
        for w in &report.warnings {
            output.warning(&format!("  - {w}"));
        }
    }

    if strict && !report.warnings.is_empty() {
        return Err(CliError::StrictWarnings {
            count: report.warnings.len(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn shown_files_names_images_from_project_root_under_a_nested_source_dir() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, contents) in [
            ("README.md", "# Home\n\n![l](site/docs/img/l.png)\n"),
            ("site/docs/guide/index.md", "# Guide\n\n![a](img/a.png)\n"),
            (
                "site/docs/guide/intro.md",
                "# Intro\n\n![b](../b.png) ![x](/.github/x.svg) ![w](https://example.com/w.png)\n",
            ),
            ("site/docs/empty/child.md", "# Child\n"),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let storage = FsStorage::new(root.to_path_buf(), root.join("site/docs"));

        let (files, warnings) = shown_files(Arc::new(storage)).unwrap();

        assert_eq!(
            files,
            BTreeSet::from(
                [
                    ".github/x.svg",
                    "site/docs/b.png",
                    "site/docs/guide/img/a.png",
                    "site/docs/img/l.png",
                ]
                .map(str::to_owned)
            )
        );
        // None of the files exist; the publisher warns about those.
        assert_eq!(warnings, [] as [String; 0]);
    }

    #[test]
    fn shown_files_warns_about_each_image_the_renderer_refused() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, contents) in [
            (
                "docs/a.md",
                "# A\n\n![s](s.bmp) ![o](../../o.png) ![s](s.bmp)\n",
            ),
            ("docs/b.md", "# B\n\n![s](s.bmp)\n"),
            ("docs/s.bmp", ""),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let (files, warnings) = shown_files(Arc::new(storage)).unwrap();

        assert_eq!(files, BTreeSet::new());
        assert_eq!(
            warnings,
            [
                "/a: image `../../o.png` is outside the project",
                "/a: image `s.bmp` is not a supported image type",
                "/b: image `s.bmp` is not a supported image type",
            ]
        );
    }

    fn report(uploaded: usize, warnings: &[&str]) -> PublishReport {
        PublishReport {
            uploaded,
            warnings: warnings.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn finish_publish_ok_when_no_warnings() {
        let output = Output::new();
        for strict in [false, true] {
            let result = finish_publish(&report(3, &[]), strict, &output);
            assert!(result.is_ok(), "strict={strict}");
        }
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
            Err(CliError::StrictWarnings { count }) => {
                assert_eq!(count, 2);
            }
            other => panic!("expected StrictWarnings, got {other:?}"),
        }
    }
}
