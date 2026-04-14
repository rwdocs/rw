//! S3-backed storage implementation.
//!
//! Reads documentation bundles from S3 using the format defined in [`crate::format`].
//! Tracks the manifest `ETag` to support change detection via [`Storage::has_changed`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aws_sdk_s3::Client;
use aws_sdk_s3::operation::get_object::GetObjectError;
use rw_storage::{Document, Metadata, Storage, StorageError, StorageErrorKind};
use tokio::runtime::{Handle, Runtime};

use crate::format::{self, FORMAT_VERSION, MANIFEST_KEY, Manifest, PageBundle};
use crate::s3::{self, S3Config};

const BACKEND: &str = "S3";

/// S3-backed storage that reads pre-built documentation bundles.
///
/// Shares a caller-provided tokio runtime for async S3 operations
/// within the synchronous `Storage` trait interface.
///
/// Read methods fetch fresh data from S3 on each call. The manifest
/// `ETag` is tracked across calls to support [`Storage::has_changed`].
/// Page modification times from the manifest are cached after each
/// [`Storage::scan`] call to avoid re-fetching the manifest per page.
pub struct S3Storage {
    client: Client,
    runtime: Arc<Runtime>,
    config: S3Config,
    last_etag: Mutex<Option<String>>,
    mtimes: Mutex<HashMap<String, f64>>,
}

impl S3Storage {
    /// Create a new `S3Storage`.
    ///
    /// Uses the provided tokio runtime for async S3 operations.
    pub fn new(config: S3Config, runtime: Arc<Runtime>) -> Result<Self, StorageError> {
        let client = runtime.block_on(s3::build_client(&config));

        Ok(Self {
            client,
            runtime,
            config,
            last_etag: Mutex::new(None),
            mtimes: Mutex::new(HashMap::new()),
        })
    }

    /// Returns a reference to the S3 client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Returns a handle to the tokio runtime.
    pub fn runtime_handle(&self) -> Handle {
        self.runtime.handle().clone()
    }

    /// Returns a reference to the S3 configuration.
    pub fn config(&self) -> &S3Config {
        &self.config
    }

    /// Fetch and parse a JSON file from S3, returning the parsed value and
    /// the `ETag` header from the response.
    fn fetch_json_with_etag<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<(T, Option<String>), StorageError> {
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

            let etag = resp.e_tag().map(String::from);

            let bytes = resp.body.collect().await.map_err(|e| {
                StorageError::new(StorageErrorKind::Other)
                    .with_backend(BACKEND)
                    .with_path(key)
                    .with_source(e)
            })?;

            let value = serde_json::from_slice(&bytes.into_bytes()).map_err(|e| {
                StorageError::new(StorageErrorKind::Other)
                    .with_backend(BACKEND)
                    .with_path(key)
                    .with_source(e)
            })?;

            Ok((value, etag))
        })
    }

    /// Fetch and parse a JSON file from S3.
    fn fetch_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<T, StorageError> {
        self.fetch_json_with_etag(key).map(|(value, _)| value)
    }

    /// HEAD request on manifest.json, returns the `ETag` header value.
    fn head_manifest_etag(&self) -> Result<Option<String>, StorageError> {
        let key = s3::build_key(&self.config, MANIFEST_KEY);
        self.runtime.block_on(async {
            let resp = self
                .client
                .head_object()
                .bucket(&self.config.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| {
                    StorageError::new(StorageErrorKind::Unavailable)
                        .with_backend(BACKEND)
                        .with_path(&key)
                        .with_source(e)
                })?;
            Ok(resp.e_tag().map(String::from))
        })
    }

    /// Fetch and validate the manifest from S3, returning its `ETag`.
    fn fetch_manifest(&self) -> Result<(Manifest, Option<String>), StorageError> {
        let key = s3::build_key(&self.config, MANIFEST_KEY);
        let (manifest, etag): (Manifest, _) = self.fetch_json_with_etag(&key)?;

        if manifest.version != FORMAT_VERSION {
            return Err(StorageError::new(StorageErrorKind::Other)
                .with_backend(BACKEND)
                .with_source(std::io::Error::other(format!(
                    "Unsupported manifest version: {} (expected {FORMAT_VERSION})",
                    manifest.version
                ))));
        }

        Ok((manifest, etag))
    }

    /// Fetch a page bundle from S3.
    fn fetch_page_bundle(&self, path: &str) -> Result<PageBundle, StorageError> {
        let key = s3::build_key(&self.config, &format::page_bundle_key(path));
        self.fetch_json(&key)
    }
}

impl Storage for S3Storage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        let (manifest, etag) = self.fetch_manifest()?;
        *self.last_etag.lock().unwrap() = etag;
        *self.mtimes.lock().unwrap() = manifest.mtimes;
        Ok(manifest.documents)
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        Ok(self.fetch_page_bundle(path)?.content)
    }

    fn exists(&self, path: &str) -> bool {
        let Ok((manifest, _)) = self.fetch_manifest() else {
            return false;
        };
        manifest
            .documents
            .iter()
            .any(|d| d.has_content && d.path == path)
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        Ok(self
            .mtimes
            .lock()
            .unwrap()
            .get(path)
            .copied()
            .unwrap_or(0.0))
    }

    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError> {
        Ok(self.fetch_page_bundle(path)?.metadata)
    }

    fn has_changed(&self) -> Result<bool, StorageError> {
        let remote_etag = self.head_manifest_etag()?;
        let last = self.last_etag.lock().unwrap();
        match (&*last, &remote_etag) {
            (Some(last), Some(remote)) if last == remote => Ok(false),
            _ => Ok(true),
        }
    }
}
