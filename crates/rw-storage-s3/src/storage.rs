//! S3-backed storage implementation.
//!
//! Reads documentation bundles from S3 using the format defined in [`crate::format`].
//! Every call fetches fresh data from S3 — no caching.

use aws_sdk_s3::Client;
use aws_sdk_s3::operation::get_object::GetObjectError;
use rw_storage::{Document, Metadata, Storage, StorageError, StorageErrorKind};

use crate::format::{self, FORMAT_VERSION, MANIFEST_KEY, Manifest, PageBundle};
use crate::s3::{self, S3Config};

const BACKEND: &str = "S3";

/// S3-backed storage that reads pre-built documentation bundles.
///
/// Uses a dedicated tokio runtime for async S3 operations within
/// the synchronous `Storage` trait interface.
///
/// Every method call fetches fresh data from S3 with no caching.
pub struct S3Storage {
    client: Client,
    runtime: tokio::runtime::Runtime,
    config: S3Config,
}

impl S3Storage {
    /// Create a new `S3Storage`.
    ///
    /// Initializes an S3 client and a dedicated tokio runtime.
    pub fn new(config: S3Config) -> Result<Self, StorageError> {
        let runtime = tokio::runtime::Runtime::new().map_err(|e| {
            StorageError::new(StorageErrorKind::Other)
                .with_backend(BACKEND)
                .with_source(e)
        })?;

        let client = runtime.block_on(s3::build_client(&config));

        Ok(Self {
            client,
            runtime,
            config,
        })
    }

    /// Returns a reference to the S3 client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Returns a handle to the tokio runtime.
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    /// Returns a reference to the S3 configuration.
    pub fn config(&self) -> &S3Config {
        &self.config
    }

    /// Fetch and parse a JSON file from S3.
    fn fetch_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<T, StorageError> {
        self.runtime.block_on(async {
            let resp = self
                .client
                .get_object()
                .bucket(&self.config.bucket)
                .key(key)
                .send()
                .await
                .map_err(|e| {
                    let kind = if matches!(e.as_service_error(), Some(GetObjectError::NoSuchKey(_)))
                    {
                        StorageErrorKind::NotFound
                    } else {
                        StorageErrorKind::Unavailable
                    };
                    StorageError::new(kind)
                        .with_backend(BACKEND)
                        .with_path(key)
                        .with_source(e)
                })?;

            let bytes = resp.body.collect().await.map_err(|e| {
                StorageError::new(StorageErrorKind::Other)
                    .with_backend(BACKEND)
                    .with_path(key)
                    .with_source(e)
            })?;

            serde_json::from_slice(&bytes.into_bytes()).map_err(|e| {
                StorageError::new(StorageErrorKind::Other)
                    .with_backend(BACKEND)
                    .with_path(key)
                    .with_source(e)
            })
        })
    }

    /// Fetch and validate the manifest from S3.
    fn fetch_manifest(&self) -> Result<Manifest, StorageError> {
        let key = s3::build_key(&self.config, MANIFEST_KEY);
        let manifest: Manifest = self.fetch_json(&key)?;

        if manifest.version != FORMAT_VERSION {
            return Err(StorageError::new(StorageErrorKind::Other)
                .with_backend(BACKEND)
                .with_source(std::io::Error::other(format!(
                    "Unsupported manifest version: {} (expected {FORMAT_VERSION})",
                    manifest.version
                ))));
        }

        Ok(manifest)
    }

    /// Fetch a page bundle from S3.
    fn fetch_page_bundle(&self, path: &str) -> Result<PageBundle, StorageError> {
        let key = s3::build_key(&self.config, &format::page_bundle_key(path));
        self.fetch_json(&key)
    }
}

impl Storage for S3Storage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        Ok(self.fetch_manifest()?.documents)
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        Ok(self.fetch_page_bundle(path)?.content)
    }

    fn exists(&self, path: &str) -> bool {
        let Ok(manifest) = self.fetch_manifest() else {
            return false;
        };
        manifest
            .documents
            .iter()
            .any(|d| d.has_content && d.path == path)
    }

    fn mtime(&self, _path: &str) -> Result<f64, StorageError> {
        Ok(0.0)
    }

    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError> {
        Ok(self.fetch_page_bundle(path)?.metadata)
    }
}
