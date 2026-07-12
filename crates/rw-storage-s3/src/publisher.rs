//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, builds bundles,
//! and uploads them to S3. Only available with the `publish` feature.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rw_kroki::DiagramProcessor;
use rw_renderer::{CodeBlockProcessor, bundle_markdown};
use rw_storage::Storage;

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
/// values fire later, inside the Kroki render path, and are not
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
    /// Uses a single shared `DiagramProcessor` so warnings from every page
    /// accumulate in one place; identical warnings are deduplicated before
    /// the report is returned.
    pub async fn publish(
        &self,
        storage: &dyn Storage,
        include_dirs: &[PathBuf],
    ) -> Result<PublishReport, BundlePublishError> {
        const MAX_CONCURRENT_UPLOADS: usize = 32;

        let client = s3::build_client(&self.config).await;
        let documents = storage.scan()?;

        // Build bundles and submit uploads as each one is ready so memory
        // stays bounded by MAX_CONCURRENT_UPLOADS rather than total site
        // size. Bundle construction is sequential because `DiagramProcessor`
        // is stateful.
        let mut tasks: tokio::task::JoinSet<Result<(), String>> = tokio::task::JoinSet::new();
        let config = Arc::new(self.config.clone());
        let mut processor = DiagramProcessor::new("").include_dirs(include_dirs);
        let mut num_bundles = 0;

        for doc in &documents {
            if !doc.has_content {
                continue;
            }

            let content = storage.read(&doc.path)?;
            let resolved_content = bundle_markdown(&content, &mut [&mut processor]);
            let metadata = storage.meta(&doc.path)?;

            let bundle = PageBundle {
                content: resolved_content,
                metadata,
            };

            let bundle_json = serde_json::to_vec(&bundle)?;
            let key = format::page_bundle_key(&doc.path);
            num_bundles += 1;

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
        }

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
            warnings: dedup_preserving_order(processor.warnings()),
        })
    }
}

/// Deduplicate warnings while preserving first-seen order.
///
/// A single broken include referenced by many pages produces N identical
/// warning strings via the shared `DiagramProcessor`; operators want to
/// see each unique issue once, not once per page.
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

    /// Drive the same shared-processor loop `publish()` uses (minus S3) so the
    /// test exercises the actual warning-collection path without needing a
    /// network. Two pages reference the same broken include — the raw
    /// processor accumulates one warning per page; dedup folds them.
    #[test]
    fn shared_processor_collects_diagram_warnings_across_pages() {
        let markdown = "\
# Page

```plantuml
@startuml
!include nonexistent.iuml
A -> B
@enduml
```
";
        let storage = MockStorage::new()
            .with_document("a", "A")
            .with_content("a", markdown)
            .with_document("b", "B")
            .with_content("b", markdown);

        let mut processor = DiagramProcessor::new("").include_dirs(&[]);
        for doc in storage.scan().expect("scan") {
            if !doc.has_content {
                continue;
            }
            let content = storage.read(&doc.path).expect("read");
            bundle_markdown(&content, &mut [&mut processor]);
        }

        let raw = processor.warnings();
        assert!(
            raw.len() >= 2,
            "shared processor accumulates a warning per page, got {raw:?}",
        );

        let deduped = dedup_preserving_order(raw);
        assert_eq!(deduped.len(), 1, "deduped warnings: {deduped:?}");
        assert!(
            deduped[0].contains("Include file not found")
                && deduped[0].contains("nonexistent.iuml"),
            "unexpected warning: {}",
            deduped[0],
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
