//! S3-backed storage implementation.
//!
//! Reads documentation bundles from S3 using the format defined in [`crate::format`].
//! Tracks the manifest `ETag` to support change detection via [`Storage::has_changed`].

use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;

use aws_sdk_s3::Client;
use aws_sdk_s3::operation::get_object::{GetObjectError, GetObjectOutput};
use aws_sdk_s3::primitives::ByteStreamError;
use rw_storage::{Document, MAX_FILE_SIZE, Storage, StorageError, StorageErrorKind};
use sha2::{Digest, Sha256};
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
/// The rest of the manifest is kept from each [`Storage::scan`] call to
/// avoid re-fetching it, so mtimes, content versions, files and the source
/// directory are valid only after a scan.
pub struct S3Storage {
    client: Client,
    runtime: Arc<Runtime>,
    config: S3Config,
    last_etag: Mutex<Option<String>>,
    /// The last scanned manifest, without its documents.
    manifest: Mutex<Manifest>,
}

impl S3Storage {
    /// Create a new `S3Storage`.
    ///
    /// Uses the provided tokio runtime for async S3 operations.
    pub fn new(config: S3Config, runtime: Arc<Runtime>) -> Self {
        let client = runtime.block_on(s3::build_client(&config));

        Self {
            client,
            runtime,
            config,
            last_etag: Mutex::new(None),
            manifest: Mutex::new(Manifest::new(Vec::new(), String::new())),
        }
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

    async fn get_object(&self, key: &str) -> Result<GetObjectOutput, StorageError> {
        self.client
            .get_object()
            .bucket(&self.config.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                let kind = if matches!(e.as_service_error(), Some(GetObjectError::NoSuchKey(_))) {
                    StorageErrorKind::NotFound
                } else {
                    StorageErrorKind::Unavailable
                };
                StorageError::new(kind)
                    .with_backend(BACKEND)
                    .with_path(key)
                    .with_source(e)
            })
    }

    /// Fetch and parse a JSON file from S3, returning the parsed value and
    /// the `ETag` header from the response.
    fn fetch_json_with_etag<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<(T, Option<String>), StorageError> {
        self.runtime.block_on(async {
            let resp = self.get_object(key).await?;

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
        let (mut manifest, etag): (Manifest, _) = self.fetch_json_with_etag(&key)?;

        if manifest.version != FORMAT_VERSION {
            return Err(StorageError::new(StorageErrorKind::Other)
                .with_backend(BACKEND)
                .with_source(std::io::Error::other(format!(
                    "Unsupported manifest version: {} (expected {FORMAT_VERSION})",
                    manifest.version
                ))));
        }

        manifest.drop_invalid_identifiers();
        Ok((manifest, etag))
    }

    /// Fetch a page bundle from S3.
    fn fetch_page_bundle(&self, path: &str) -> Result<PageBundle, StorageError> {
        let key = s3::build_key(&self.config, &format::page_bundle_key(path));
        self.fetch_json_with_etag(&key).map(|(bundle, _)| bundle)
    }
}

impl Storage for S3Storage {
    fn scan(&self) -> Result<(Vec<Document>, Vec<String>), StorageError> {
        let (mut manifest, etag) = self.fetch_manifest()?;
        let documents = manifest.take_documents();
        *self.last_etag.lock() = etag;
        *self.manifest.lock() = manifest;
        Ok((documents, Vec::new()))
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        Ok(self.fetch_page_bundle(path)?.content)
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        let not_found = || StorageError::not_found(path).with_backend(BACKEND);
        let (key, sha256) = locate_file(&self.manifest.lock().files, path).ok_or_else(not_found)?;
        let key = s3::build_key(&self.config, &key);
        let bytes = self.runtime.block_on(async {
            let object = self.get_object(&key).await.map_err(|e| match e.kind {
                StorageErrorKind::NotFound => not_found(),
                _ => e,
            })?;
            read_file_object(&key, &sha256, object, MAX_FILE_SIZE)
                .await
                .map_err(|e| {
                    StorageError::new(StorageErrorKind::Other)
                        .with_backend(BACKEND)
                        .with_path(&key)
                        .with_source(e)
                })
        })?;
        bytes.ok_or_else(not_found)
    }

    fn has_file(&self, path: &str) -> bool {
        locate_file(&self.manifest.lock().files, path).is_some()
    }

    fn serves_referenced_files_only(&self) -> bool {
        true
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        Ok(self
            .manifest
            .lock()
            .mtimes
            .get(path)
            .copied()
            .unwrap_or(0.0))
    }

    fn content_version(&self, path: &str) -> Option<String> {
        self.manifest.lock().bundles.get(path).cloned()
    }

    fn has_changed(&self) -> Result<bool, StorageError> {
        let remote_etag = self.head_manifest_etag()?;
        let last = self.last_etag.lock();
        match (&*last, &remote_etag) {
            (Some(last), Some(remote)) if last == remote => Ok(false),
            _ => Ok(true),
        }
    }

    fn source_dir_in_project(&self) -> String {
        self.manifest.lock().source_dir.clone()
    }
}

/// The bundle key of `path` and the SHA-256 its content must hash to, or
/// `None` when the manifest doesn't list it or [`format::file_key`] refuses
/// it.
fn locate_file(files: &BTreeMap<String, String>, path: &str) -> Option<(String, String)> {
    let sha256 = files.get(path)?;
    Some((format::file_key(path, sha256)?, sha256.clone()))
}

/// The bytes of a fetched file object, or `None` when it is larger than
/// `limit` or doesn't hash to `sha256`.
///
/// The declared length spares reading an oversized body, but a body that
/// outruns it still stops just past `limit`.
async fn read_file_object(
    key: &str,
    sha256: &str,
    object: GetObjectOutput,
    limit: u64,
) -> Result<Option<Vec<u8>>, ByteStreamError> {
    if object
        .content_length()
        .and_then(|length| u64::try_from(length).ok())
        .is_some_and(|length| length > limit)
    {
        return Ok(None);
    }
    let mut body = object.body;
    let mut bytes = Vec::new();
    while let Some(chunk) = body.try_next().await? {
        if (bytes.len() + chunk.len()) as u64 > limit {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk);
    }
    if hex::encode(Sha256::digest(&bytes)) != sha256 {
        tracing::warn!("S3 object {key:?} does not match its SHA-256 in the manifest");
        return Ok(None);
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use aws_sdk_s3::primitives::ByteStream;
    use tempfile::NamedTempFile;

    use super::*;

    const PNG: &[u8] = b"\x89PNG";

    fn sha256(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn object(bytes: &'static [u8], content_length: Option<i64>) -> GetObjectOutput {
        GetObjectOutput::builder()
            .body(ByteStream::from_static(bytes))
            .set_content_length(content_length)
            .build()
    }

    async fn read(object: GetObjectOutput, limit: u64) -> Option<Vec<u8>> {
        read_file_object("files/x.png", &sha256(PNG), object, limit)
            .await
            .unwrap()
    }

    #[test]
    fn locate_file_finds_listed_allowlisted_files() {
        let hash = sha256(PNG);
        let files = BTreeMap::from([
            ("docs/img/Flow.PNG".to_owned(), hash.clone()),
            (".env".to_owned(), hash.clone()),
            ("docs/bad.png".to_owned(), "../manifest".to_owned()),
        ]);

        assert_eq!(
            locate_file(&files, "docs/img/Flow.PNG"),
            Some((format!("files/{hash}.png"), hash))
        );
        for path in ["docs/img/flow.png", ".env", "docs/bad.png"] {
            assert_eq!(locate_file(&files, path), None, "{path}");
        }
    }

    #[tokio::test]
    async fn read_file_object_returns_bytes_that_match_the_hash() {
        assert_eq!(read(object(PNG, None), 4).await.as_deref(), Some(PNG));
        assert_eq!(read(object(PNG, Some(4)), 4).await.as_deref(), Some(PNG));
    }

    #[tokio::test]
    async fn read_file_object_refuses_an_object_over_the_limit() {
        // Declared too long, whatever the body holds.
        assert_eq!(read(object(PNG, Some(5)), 4).await, None);
        // Longer than declared.
        assert_eq!(read(object(PNG, Some(1)), 3).await, None);
        assert_eq!(read(object(PNG, None), 3).await, None);
    }

    #[tokio::test]
    async fn read_file_object_caps_the_total_of_a_chunked_body() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), PNG).unwrap();
        let body = ByteStream::read_from()
            .path(file.path())
            .buffer_size(1)
            .build()
            .await
            .unwrap();
        let object = GetObjectOutput::builder().body(body).build();

        assert_eq!(read(object, 3).await, None);
    }

    #[tokio::test]
    async fn read_file_object_refuses_bytes_that_do_not_match_the_hash() {
        assert_eq!(read(object(b"\x89PNG!", None), 1024).await, None);
    }
}
