//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, builds bundles,
//! and uploads them to S3. Only available with the `publish` feature.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::put_object::PutObjectError;
use rw_parser::rewrite_fences;
use rw_plantuml::bundle_source;
use rw_storage::{Document, Storage, StorageError};

use crate::format::{self, MANIFEST_KEY, Manifest, PageBundle};
use crate::s3::{self, S3Config};

/// Errors that can occur during publishing.
#[derive(Debug, thiserror::Error)]
pub enum BundlePublishError {
    #[error("Storage error")]
    Storage(#[from] StorageError),
    #[error("JSON error")]
    Json(#[from] serde_json::Error),
    #[error("S3 error")]
    S3(#[source] Box<SdkError<PutObjectError>>),
    #[error("pages {first:?} and {second:?} both publish to {key:?}; rename one of them")]
    DuplicateBundleKey {
        key: String,
        first: String,
        second: String,
    },
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

        let documents = storage.scan()?;
        check_unique_bundle_keys(&documents)?;
        let client = s3::build_client(&self.config).await;

        let mut tasks: tokio::task::JoinSet<Result<(), BundlePublishError>> =
            tokio::task::JoinSet::new();
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
                        .expect("upload task panicked")?;
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
            result.expect("upload task panicked")?;
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
        .await?;

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

        let bundle = PageBundle {
            content: resolved_content,
        };

        let bundle_json = serde_json::to_vec(&bundle)?;
        let key = format::page_bundle_key(&doc.path);
        num_bundles += 1;

        on_bundle(key, bundle_json).await?;
    }

    let mut seen = HashSet::new();
    warnings.retain(|warning| seen.insert(warning.clone()));
    Ok((num_bundles, warnings))
}

/// Fails when two pages with content map to one bundle key.
///
/// Uploads run concurrently, so pages sharing a key would race. Only the
/// root's `pages/_index.json` can be shared, with a page at `_index`.
fn check_unique_bundle_keys(documents: &[Document]) -> Result<(), BundlePublishError> {
    let mut owners: HashMap<String, &str> = HashMap::new();
    for doc in documents.iter().filter(|doc| doc.has_content) {
        let key = format::page_bundle_key(&doc.path);
        if let Some(&owner) = owners.get(&key) {
            let (first, second) = if owner <= doc.path.as_str() {
                (owner, doc.path.as_str())
            } else {
                (doc.path.as_str(), owner)
            };
            return Err(BundlePublishError::DuplicateBundleKey {
                first: format!("/{first}"),
                second: format!("/{second}"),
                key,
            });
        }
        owners.insert(key, &doc.path);
    }
    Ok(())
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
        let bundle: PageBundle = serde_json::from_slice(json).expect("valid page bundle JSON");
        assert!(
            bundle.content.contains("Bob -> Charlie"),
            "{key} kept the include unresolved: {}",
            bundle.content
        );
        assert!(
            !bundle.content.contains("!include"),
            "{key} still carries an !include: {}",
            bundle.content
        );
    }

    #[tokio::test]
    async fn build_bundles_preserves_declared_name_includes_direct_and_nested() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("shared.iuml"),
            "!include systems/ext/sys_payments_api.iuml\nAlice -> Bob",
        )
        .unwrap();
        let document: Document = serde_json::from_value(serde_json::json!({
            "path":"systems/payments-guide", "title":"Payments documentation",
            "has_content":true, "page_kind":"system", "namespace":"commerce", "name":"payments-api"
        }))
        .unwrap();
        let storage = MockStorage::new().with_scanned_document(document).with_content(
            "systems/payments-guide",
            "# Payments\n\n```plantuml\n@startuml\n!include systems/sys_payments_api.iuml\n!include shared.iuml\n@enduml\n```\n",
        );
        let documents = storage.scan().unwrap();
        let mut bundles = Vec::new();
        let (count, warnings) = build_bundles(
            &storage,
            &documents,
            &[temp.path().to_path_buf()],
            async |key, json| {
                bundles.push((key, json));
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(count, 1);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(bundles[0].0, "pages/systems/payments-guide.json");
        let bundle: PageBundle = serde_json::from_slice(&bundles[0].1).unwrap();
        assert!(
            bundle
                .content
                .contains("!include systems/sys_payments_api.iuml")
        );
        assert!(
            bundle
                .content
                .contains("!include systems/ext/sys_payments_api.iuml")
        );
        assert!(bundle.content.contains("Alice -> Bob"));
        assert!(!bundle.content.contains("!include shared.iuml"));
        assert!(!bundle.content.contains("System("));
    }

    #[tokio::test]
    async fn build_bundles_skips_virtual_pages_and_keeps_raw_content() {
        let storage = MockStorage::new()
            .with_document("selected", "Selected")
            .with_virtual_page("virtual", "title: Virtual")
            .with_content(
                "selected",
                "---\nattrs: {owner: raw, unexpected: true}\n---\n# Body\n",
            );
        let documents = storage.scan().unwrap();
        let mut bundles = Vec::new();
        let (count, warnings) = build_bundles(&storage, &documents, &[], async |key, json| {
            bundles.push((key, json));
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(bundles[0].0, "pages/selected.json");
        let bundle: PageBundle = serde_json::from_slice(&bundles[0].1).unwrap();
        assert!(bundle.content.contains("owner: raw"));
    }

    /// `docs/_index.md` scans to `_index`, whose bundle key is the root's.
    /// The endpoint refuses connections, so reaching the network would
    /// surface an S3 error instead.
    #[tokio::test]
    async fn publish_rejects_shared_bundle_key_before_uploading() {
        let storage = MockStorage::new()
            .with_document("_index", "Underscore")
            .with_content("_index", "# Underscore\n")
            .with_document("", "Home")
            .with_content("", "# Home\n");
        let publisher = BundlePublisher::new(S3Config {
            bucket: "bucket".to_owned(),
            prefix: "default/component/x".to_owned(),
            region: "us-east-1".to_owned(),
            endpoint: Some("http://127.0.0.1:1".to_owned()),
            bucket_root_path: None,
            access_key_id: Some("id".to_owned()),
            secret_access_key: Some("secret".to_owned()),
        });

        let err = publisher.publish(&storage, &[]).await.unwrap_err();

        assert_eq!(
            err.to_string(),
            r#"pages "/" and "/_index" both publish to "pages/_index.json"; rename one of them"#
        );
    }

    #[test]
    fn virtual_root_does_not_claim_its_bundle_key() {
        let storage = MockStorage::new()
            .with_virtual_page("", "title: Home")
            .with_document("_index", "Underscore");

        check_unique_bundle_keys(&storage.scan().unwrap()).unwrap();
    }
}
