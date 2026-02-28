//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, builds bundles,
//! and uploads them to S3. Only available with the `publish` feature.

use std::path::PathBuf;
use std::sync::Arc;

use rw_diagrams::DiagramProcessor;
use rw_renderer::bundle_markdown;
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

impl BundlePublisher {
    #[must_use]
    pub fn new(config: S3Config) -> Self {
        Self { config }
    }

    /// Publish documentation from a storage backend to S3.
    ///
    /// Scans the storage for documents, builds bundles with pre-resolved
    /// `PlantUML` includes, and uploads everything to S3.
    ///
    /// Returns the number of files uploaded.
    pub async fn publish(
        &self,
        storage: &dyn Storage,
        include_dirs: &[PathBuf],
    ) -> Result<usize, BundlePublishError> {
        let client = s3::build_client(&self.config).await;
        let documents = storage.scan()?;

        // Build all page bundles sequentially (DiagramProcessor is stateful).
        let mut bundles = Vec::with_capacity(documents.len());
        let mut processor = DiagramProcessor::new("").include_dirs(include_dirs);

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
            bundles.push((key, bundle_json));
        }

        // Upload page bundles with bounded concurrency.
        const MAX_CONCURRENT_UPLOADS: usize = 32;
        let mut tasks: tokio::task::JoinSet<Result<(), String>> = tokio::task::JoinSet::new();
        let config = Arc::new(self.config.clone());
        let num_bundles = bundles.len();

        for (key, body) in bundles {
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
                s3::upload(&client, &config, &key, body, "application/json").await
            });
        }

        while let Some(result) = tasks.join_next().await {
            result
                .expect("upload task panicked")
                .map_err(BundlePublishError::S3)?;
        }

        // Upload manifest last so readers don't see a manifest referencing
        // pages that haven't been uploaded yet.
        let manifest = Manifest::new(documents);
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

        Ok(num_bundles + 1)
    }
}
