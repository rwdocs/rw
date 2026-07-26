//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, builds bundles,
//! and uploads them to S3. Only available with the `publish` feature.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rw_parser::rewrite_fences;
use rw_plantuml::bundle_source;
use rw_storage::{Document, Storage};

use crate::format::{self, MANIFEST_KEY, Manifest, PageBundle};
use crate::s3::{self, S3Config};

/// Errors that can occur during publishing.
#[derive(Debug, thiserror::Error)]
pub enum BundlePublishError {
    #[error("Storage error: {0}")]
    Storage(#[from] rw_storage::StorageError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("S3 error: {0}")]
    S3(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Publisher that builds and uploads documentation bundles to S3.
pub struct BundlePublisher {
    config: S3Config,
}

/// Outcome of a publish run.
///
/// Warnings are accumulated from `PlantUML` `!include` resolution
/// (broken include paths, cyclic includes) across every page.
/// Runtime diagnostics such as unknown attributes or invalid `format`
/// values fire later, when a page is actually rendered, and are not
/// captured here.
///
/// Repeated identical warnings are deduplicated so a missing shared
/// include referenced by many pages reads as a single entry.
///
/// `rw backstage publish --strict` exits non-zero when this vector
/// is non-empty.
#[derive(Debug, Clone)]
pub struct PublishReport {
    /// Number of objects uploaded (page bundles + manifest).
    pub uploaded: usize,
    /// Deduplicated diagram processing warnings accumulated across all pages.
    pub warnings: Vec<String>,
}

impl BundlePublisher {
    #[must_use]
    pub fn new(config: S3Config) -> Self {
        Self { config }
    }

    /// Publish documentation from a storage backend to S3.
    ///
    /// Scans the storage, builds bundles with pre-resolved `PlantUML`
    /// includes, streams them to S3 (uploads start as soon as each bundle
    /// is ready), and returns a [`PublishReport`] with the upload count and
    /// any `!include` resolution warnings (see [`PublishReport`] for what
    /// is and isn't captured).
    ///
    /// Include-resolution warnings from every page accumulate in one vector;
    /// identical warnings are deduplicated before the report is returned.
    pub async fn publish(
        &self,
        storage: &dyn Storage,
        include_dirs: &[PathBuf],
    ) -> Result<PublishReport, BundlePublishError> {
        const MAX_CONCURRENT_UPLOADS: usize = 32;

        let client = s3::build_client(&self.config).await;
        let documents = storage.scan()?;

        let mut tasks: tokio::task::JoinSet<Result<(), String>> = tokio::task::JoinSet::new();
        let config = Arc::new(self.config.clone());

        // Submit each upload as soon as its bundle is ready so memory stays
        // bounded by MAX_CONCURRENT_UPLOADS rather than total site size.
        let (num_bundles, warnings) = build_bundles(
            storage,
            &documents,
            include_dirs,
            async |key: String, bundle_json: Vec<u8>| {
                if tasks.len() >= MAX_CONCURRENT_UPLOADS {
                    tasks
                        .join_next()
                        .await
                        .expect("task set is non-empty")
                        .expect("upload task panicked")
                        .map_err(BundlePublishError::S3)?;
                }

                let client = client.clone();
                let config = Arc::clone(&config);
                tasks.spawn(async move {
                    s3::upload(&client, &config, &key, bundle_json, "application/json").await
                });
                Ok(())
            },
        )
        .await?;

        while let Some(result) = tasks.join_next().await {
            result
                .expect("upload task panicked")
                .map_err(BundlePublishError::S3)?;
        }

        // Resolve modification times for each document.
        let mut mtimes = HashMap::new();
        for doc in &documents {
            if let Ok(mtime) = storage.mtime(&doc.path) {
                mtimes.insert(doc.path.clone(), mtime);
            }
        }

        // Upload manifest last so readers don't see a manifest referencing
        // pages that haven't been uploaded yet.
        let mut manifest = Manifest::from(documents);
        manifest.mtimes = mtimes;
        let manifest_json = serde_json::to_vec(&manifest)?;
        s3::upload(
            &client,
            &self.config,
            MANIFEST_KEY,
            manifest_json,
            "application/json",
        )
        .await
        .map_err(BundlePublishError::S3)?;

        Ok(PublishReport {
            uploaded: num_bundles + 1,
            warnings,
        })
    }
}

/// Build one page bundle per document that has content, handing each
/// `(key, bundle_json)` pair to `on_bundle` as soon as it is ready.
///
/// Returns the number of bundles built and the deduplicated `!include`
/// resolution warnings.
///
/// Bundles are handed over one at a time instead of collected so a caller that
/// uploads them can keep only a bounded number in memory at once, whatever the
/// size of the site. Construction is sequential because include-resolution
/// warnings accumulate across pages, in document order.
async fn build_bundles(
    storage: &dyn Storage,
    documents: &[Document],
    include_dirs: &[PathBuf],
    mut on_bundle: impl AsyncFnMut(String, Vec<u8>) -> Result<(), BundlePublishError>,
) -> Result<(usize, Vec<String>), BundlePublishError> {
    let mut warnings = Vec::new();
    let mut num_bundles = 0;

    for doc in documents {
        if !doc.has_content {
            continue;
        }

        let content = storage.read(&doc.path)?;
        let resolved_content = rewrite_fences(&content, |lang, src| {
            bundle_source(lang, src, include_dirs, &mut warnings)
        });
        let metadata = storage.meta(&doc.path)?;

        let bundle = PageBundle {
            content: resolved_content,
            metadata,
        };

        let bundle_json = serde_json::to_vec(&bundle)?;
        let key = format::page_bundle_key(&doc.path);
        num_bundles += 1;

        on_bundle(key, bundle_json).await?;
    }

    Ok((num_bundles, dedup_preserving_order(&warnings)))
}

/// Deduplicate warnings while preserving first-seen order.
///
/// A single broken include referenced by many pages produces N identical
/// warning strings; operators want to see each unique issue once, not once
/// per page.
fn dedup_preserving_order(warnings: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    warnings
        .iter()
        .filter(|w| seen.insert(w.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rw_storage::MockStorage;

    /// Exercises everything `publish()` does apart from the S3 calls: the
    /// bundle-building loop, the fence rewrite that inlines `!include`s, and
    /// the dedup that folds one broken shared include into a single warning.
    /// Pages `a` and `b` share a broken include (two raw warnings, one
    /// deduplicated); page `c` has a resolvable one and must come out inlined.
    #[tokio::test]
    async fn build_bundles_inlines_includes_and_dedups_warnings() {
        let include_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(include_dir.path().join("shared.iuml"), "Bob -> Charlie")
            .expect("write include");

        let broken = "\
# Page

```plantuml
@startuml
!include nonexistent.iuml
A -> B
@enduml
```
";
        let resolvable = "\
# Page

```plantuml
@startuml
!include shared.iuml
@enduml
```
";
        let storage = MockStorage::new()
            .with_document("a", "A")
            .with_content("a", broken)
            .with_document("b", "B")
            .with_content("b", broken)
            .with_document("c", "C")
            .with_content("c", resolvable);

        let documents = storage.scan().expect("scan");
        let mut bundles = Vec::new();
        let (num_bundles, warnings) = build_bundles(
            &storage,
            &documents,
            &[include_dir.path().to_path_buf()],
            async |key, json| {
                bundles.push((key, json));
                Ok(())
            },
        )
        .await
        .expect("build bundles");

        assert_eq!(num_bundles, 3);
        assert_eq!(warnings.len(), 1, "deduplicated warnings: {warnings:?}");
        assert!(
            warnings[0].contains("Include file not found")
                && warnings[0].contains("nonexistent.iuml"),
            "unexpected warning: {}",
            warnings[0],
        );

        let (key, json) = bundles
            .iter()
            .find(|(key, _)| key == "pages/c.json")
            .expect("a bundle per page with content");
        let bundle = String::from_utf8(json.clone()).expect("bundle is UTF-8");
        assert!(
            bundle.contains("Bob -> Charlie"),
            "{key} kept the include unresolved: {bundle}"
        );
        assert!(
            !bundle.contains("!include"),
            "{key} still carries an !include: {bundle}"
        );
    }

    #[test]
    fn dedup_preserves_first_seen_order() {
        let input = [
            "a".to_owned(),
            "b".to_owned(),
            "a".to_owned(),
            "c".to_owned(),
            "b".to_owned(),
        ];
        let out = dedup_preserving_order(&input);
        assert_eq!(out, vec!["a", "b", "c"]);
    }
}
