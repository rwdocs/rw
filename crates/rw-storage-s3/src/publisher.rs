//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, builds bundles,
//! and uploads them to S3. Only available with the `publish` feature.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rw_kroki::DiagramProcessor;
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
        const MAX_CONCURRENT_UPLOADS: usize = 32;

        let client = s3::build_client(&self.config).await;
        let documents = storage.scan()?;

        // Build bundles and upload with bounded concurrency.
        // Bundles are submitted to the upload pool as they are built so that
        // memory usage is bounded by MAX_CONCURRENT_UPLOADS rather than total
        // site size. Building is sequential because DiagramProcessor is stateful.
        let mut tasks: tokio::task::JoinSet<Result<(), String>> = tokio::task::JoinSet::new();
        let config = Arc::new(self.config.clone());
        let mut num_bundles = 0;
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
        let mut manifest = Manifest::new(documents);
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

        Ok(num_bundles + 1)
    }
}
