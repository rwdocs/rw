//! Bundle publisher.
//!
//! Scans local documentation, resolves `PlantUML` includes, builds bundles,
//! and uploads them to S3. Only available with the `publish` feature.

use std::path::PathBuf;

use aws_sdk_s3::Client;
use rw_diagrams::DiagramProcessor;
use rw_renderer::bundle_markdown;
use rw_storage::Storage;

use crate::format::{self, Manifest, PageBundle};
use crate::s3::{self, S3Config};

/// Configuration for publishing documentation bundles to S3.
#[derive(Debug, Clone)]
pub struct PublishConfig {
    /// S3 bucket name.
    pub bucket: String,
    /// S3 key prefix (e.g., `"default/Component/arch"`).
    pub prefix: String,
    /// AWS region (default: `"us-east-1"`).
    pub region: String,
    /// Optional S3-compatible endpoint URL.
    pub endpoint: Option<String>,
    /// Optional prefix path within the bucket.
    pub bucket_root_path: Option<String>,
}

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
    config: PublishConfig,
}

impl BundlePublisher {
    #[must_use]
    pub fn new(config: PublishConfig) -> Self {
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
        let s3_config = self.s3_config();
        let client = s3::build_client(&s3_config).await;
        let documents = storage.scan()?;

        let manifest = Manifest::new(documents.clone());
        let manifest_json = serde_json::to_vec(&manifest)?;
        self.upload(
            &client,
            &s3_config,
            "manifest.json",
            manifest_json,
            "application/json",
        )
        .await?;

        let mut uploaded = 1; // manifest

        for doc in &documents {
            if !doc.has_content {
                continue;
            }

            let content = storage.read(&doc.path)?;
            let mut processor = DiagramProcessor::new("").include_dirs(include_dirs);
            let resolved_content = bundle_markdown(&content, &mut [&mut processor]);
            let metadata = storage.meta(&doc.path)?;

            let bundle = PageBundle {
                content: resolved_content,
                metadata,
            };

            let bundle_json = serde_json::to_vec(&bundle)?;
            let key = format::page_bundle_key(&doc.path);
            self.upload(&client, &s3_config, &key, bundle_json, "application/json")
                .await?;

            uploaded += 1;
            tracing::debug!(path = %doc.path, "Published page bundle");
        }

        Ok(uploaded)
    }

    fn s3_config(&self) -> S3Config {
        S3Config {
            region: self.config.region.clone(),
            endpoint: self.config.endpoint.clone(),
            bucket_root_path: self.config.bucket_root_path.clone(),
            prefix: self.config.prefix.clone(),
        }
    }

    async fn upload(
        &self,
        client: &Client,
        s3_config: &S3Config,
        relative_key: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<(), PublishError> {
        let key = s3::build_key(s3_config, relative_key);
        client
            .put_object()
            .bucket(&self.config.bucket)
            .key(&key)
            .body(body.into())
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| PublishError::S3(s3::error_chain(&e)))?;
        tracing::debug!(key = %key, "Uploaded");
        Ok(())
    }
}
