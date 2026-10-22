//! `rw backstage publish` command implementation.

use std::collections::{BTreeMap, BTreeSet};
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

    /// Exit with a non-zero status when any warning is emitted: about the
    /// project (conflicting source files, metadata problems, bad `pages`
    /// entries), any page (directives, wikilinks, includes, missing or refused
    /// images and files, diagrams not alone in their paragraph), or its
    /// diagram files (unreadable files, broken includes).
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
        // checkout time).
        let storage = Arc::new(
            FsStorage::new(config.project_dir.clone(), config.docs.source_dir.clone())
                .with_mtime_source(MtimeSource::Git),
        );
        let shown = shown_files(&storage)?;

        let include_dirs = config.diagrams.include_dirs;
        let publisher = BundlePublisher::new(self.s3.into_config());

        let rt = tokio::runtime::Runtime::new()?;
        let mut report = rt.block_on(publisher.publish(
            &*storage,
            &config.project_dir,
            &include_dirs,
            &shown.files,
            &shown.includes,
        ))?;
        report.warnings.extend(shown.warnings);

        finish_publish(&report, self.strict, &output)
    }
}

/// What publishing needs from rendering every page.
struct Shown {
    /// The project files that pages show, found or not.
    files: BTreeSet<String>,
    /// Each page's path mapped to the markdown files it includes.
    includes: BTreeMap<String, BTreeSet<String>>,
    /// The site's, then each page's, prefixed with the page.
    warnings: Vec<String>,
}

/// Renders every page through [`Site`], as the Backstage reader does over the
/// published bundle, so the paths are the ones it will look up. Diagrams are
/// deferred: the reader renders the ones the bundle embeds, and nothing here
/// reaches the network.
fn shown_files(storage: &Arc<FsStorage>) -> Result<Shown, CliError> {
    let site = Site::new(
        Arc::clone(storage) as Arc<dyn Storage>,
        Arc::new(NullCache),
        PageRendererConfig::default(),
    )
    .with_deferred_diagrams();
    let mut files = BTreeSet::new();
    let mut includes = BTreeMap::new();
    let mut warnings = BTreeSet::new();
    for page in site.list_pages()? {
        if !page.has_content {
            continue;
        }
        let render = site.render(&page.path)?;
        warnings.extend(
            render
                .warnings
                .into_iter()
                .map(|warning| format!("/{}: {warning}", page.path)),
        );
        files.extend(render.files);
        if !render.includes.is_empty() {
            includes.insert(page.path, render.includes);
        }
    }
    let mut all = site.warnings()?;
    all.extend(warnings);
    Ok(Shown {
        files,
        includes,
        warnings: all,
    })
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
    let objects = if report.uploaded == 1 {
        "object"
    } else {
        "objects"
    };
    output.success(&format!(
        "Published: uploaded {} {objects}",
        report.uploaded
    ));

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

        let Shown {
            files, warnings, ..
        } = shown_files(&Arc::new(storage)).unwrap();

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
        assert_eq!(
            warnings,
            [
                "/: image `site/docs/img/l.png` not found",
                "/guide/intro: image `.github/x.svg` not found",
                "/guide/intro: image `site/docs/b.png` not found",
                "/guide: image `site/docs/guide/img/a.png` not found",
            ]
        );
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

        let Shown {
            files, warnings, ..
        } = shown_files(&Arc::new(storage)).unwrap();

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

    #[test]
    fn shown_files_names_linked_files_and_warns_about_refused_links() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, contents) in [
            (
                "docs/a.md",
                "# A\n\n[s](spec.pdf) [d](/data/d.csv) [c](config.yaml) [o](../../o.zip)\n",
            ),
            ("docs/config.yaml", ""),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let Shown {
            files, warnings, ..
        } = shown_files(&Arc::new(storage)).unwrap();

        assert_eq!(
            files,
            BTreeSet::from(["data/d.csv", "docs/spec.pdf"].map(str::to_owned))
        );
        assert_eq!(
            warnings,
            [
                "/a: link `../../o.zip` is outside the project",
                "/a: link `config.yaml`: rw does not serve `.yaml` files",
                "/a: linked file `data/d.csv` not found",
                "/a: linked file `docs/spec.pdf` not found",
            ]
        );
    }

    /// The publisher embeds a lone diagram file image itself; one inside text
    /// it can't, so that one is refused.
    #[test]
    fn shown_files_refuses_only_diagram_file_images_not_alone() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, contents) in [
            (
                "docs/a.md",
                "# A\n\n![Flow](flow.puml)\n\nSee ![Flow](flow.puml).\n",
            ),
            ("docs/flow.puml", "@startuml\nA -> B\n@enduml\n"),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let Shown {
            files, warnings, ..
        } = shown_files(&Arc::new(storage)).unwrap();

        assert_eq!(files, BTreeSet::new());
        assert_eq!(
            warnings,
            [
                "/a: diagram `flow.puml` must be alone in its paragraph: put it on its own line with a blank line before and after"
            ]
        );
    }

    #[test]
    fn shown_files_names_each_pages_includes_at_any_depth_and_their_images() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, contents) in [
            (
                "docs/guide/index.md",
                "# Guide\n\n::include[../_partials/note.md]\n\n::include[_missing.md]\n",
            ),
            (
                "docs/_partials/note.md",
                "![logo](logo.png)\n\n::include[/README.md]\n",
            ),
            ("docs/_partials/logo.png", ""),
            ("README.md", "Readme.\n"),
            ("docs/other.md", "# Other\n"),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let shown = shown_files(&Arc::new(storage)).unwrap();

        assert_eq!(
            shown.includes,
            BTreeMap::from([(
                "guide".to_owned(),
                BTreeSet::from(["README.md", "docs/_partials/note.md"].map(str::to_owned))
            )])
        );
        assert_eq!(
            shown.files,
            BTreeSet::from(["docs/_partials/logo.png".to_owned()])
        );
        assert_eq!(
            shown.warnings,
            ["/guide: include `docs/guide/_missing.md` not found"]
        );
    }

    #[test]
    fn shown_files_warns_about_broken_links() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, contents) in [
            (
                "docs/index.md",
                "# Home\n\n[[::guide]] [[::missing]] [[domain:nope::x]] [g](guide.md) [m](missing.md)\n",
            ),
            ("docs/guide.md", "# Guide\n"),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let warnings = shown_files(&Arc::new(storage)).unwrap().warnings;

        assert_eq!(
            warnings,
            [
                "/: link `missing.md`: no page `missing`",
                "/: wikilink `[[::missing]]`: no page `missing` in section `section:default/root`",
                "/: wikilink `[[domain:nope::x]]`: no such section",
            ]
        );
    }

    #[test]
    fn shown_files_warns_about_directives() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let path = root.join("docs/index.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "# Home\n\n:::tab[Lone]\nx\n:::\n\n::youtube[id]\n").unwrap();
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let warnings = shown_files(&Arc::new(storage)).unwrap().warnings;

        assert_eq!(
            warnings,
            [
                "/: `:::tab` outside a `::::tabs` group; its content is rendered without tab chrome",
                "/: unknown leaf directive '::youtube'",
            ]
        );
    }

    /// The publisher skips a missing file without a warning of its own.
    #[test]
    fn shown_files_warns_about_a_missing_image() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let path = root.join("docs/a.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "# A\n\n![m](m.png) ![m](m.png)\n").unwrap();
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let Shown {
            files, warnings, ..
        } = shown_files(&Arc::new(storage)).unwrap();

        assert_eq!(files, BTreeSet::from(["docs/m.png".to_owned()]));
        assert_eq!(warnings, ["/a: image `docs/m.png` not found"]);
    }

    #[test]
    fn shown_files_reports_scan_warnings() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, contents) in [
            ("docs/a.md", "# A\n"),
            ("docs/a/index.md", "# A\n"),
            ("docs/b.md", "# B\n"),
            ("docs/b.meta.yaml", "namespace: bad/value\n"),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let warnings = shown_files(&Arc::new(storage)).unwrap().warnings;

        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(
            warnings[0],
            "Excluding docs/a.md from the site because it conflicts with docs/a/index.md"
        );
        assert!(
            warnings[1].starts_with("docs/b.meta.yaml: sidecar `namespace`"),
            "{warnings:?}"
        );
    }

    #[test]
    fn shown_files_reports_pages_warnings() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, contents) in [
            ("docs/meta.yaml", "pages: [a, a, missing, billing]\n"),
            ("docs/a.md", "# A\n"),
            ("docs/billing/meta.yaml", "kind: domain\n"),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let warnings = shown_files(&Arc::new(storage)).unwrap().warnings;

        assert_eq!(
            warnings,
            [
                "/: slug `a` in `pages` is a duplicate, ignoring",
                "/: slug `billing` in `pages` matches a section directory, skipping",
                "/: slug `missing` in `pages` has no matching child, skipping",
            ]
        );
    }

    #[test]
    fn shown_files_reports_duplicate_sections() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, contents) in [
            ("docs/z/meta.yaml", "kind: domain\nname: billing\n"),
            ("docs/m/meta.yaml", "kind: domain\nname: billing\n"),
            ("docs/a/meta.yaml", "kind: domain\nname: billing\n"),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let storage = FsStorage::new(root.to_path_buf(), root.join("docs"));

        let warnings = shown_files(&Arc::new(storage)).unwrap().warnings;

        assert_eq!(
            warnings,
            [
                "/m: section `domain:default/billing` is also declared by /a; refs resolve to /a",
                "/z: section `domain:default/billing` is also declared by /a; refs resolve to /a",
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
