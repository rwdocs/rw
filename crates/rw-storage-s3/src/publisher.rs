//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, builds bundles,
//! and uploads them to S3. Only available with the `publish` feature.

use std::path::PathBuf;

use aws_sdk_s3::Client;
use rw_diagrams::DiagramProcessor;
use rw_renderer::bundle_markdown;
use rw_storage::Storage;

use crate::format::{self, MANIFEST_KEY, Manifest, PageBundle};
use crate::s3::{self, S3Config};

/// Errors that can occur during publishing.
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
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
    ) -> Result<usize, PublishError> {
        let client = s3::build_client(&self.config).await;
        let documents = storage.scan()?;

        let mut uploaded = 1; // manifest counts as 1
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
            self.upload(&client, &key, bundle_json).await?;

            uploaded += 1;
            tracing::debug!(path = %doc.path, "Published page bundle");
        }

        // Upload manifest last so readers don't see a manifest referencing
        // pages that haven't been uploaded yet.
        let manifest = Manifest::new(documents);
        let manifest_json = serde_json::to_vec(&manifest)?;
        self.upload(&client, MANIFEST_KEY, manifest_json).await?;

        Ok(uploaded)
    }

    async fn upload(
        &self,
        client: &Client,
        relative_key: &str,
        body: Vec<u8>,
    ) -> Result<(), PublishError> {
        let key = s3::build_key(&self.config, relative_key);
        client
            .put_object()
            .bucket(&self.config.bucket)
            .key(&key)
            .body(body.into())
            .content_type("application/json")
            .send()
            .await
            .map_err(|e| PublishError::S3(s3::error_chain(&e)))?;
        tracing::debug!(key = %key, "Uploaded");
        Ok(())
    }
}
