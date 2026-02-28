//! S3-backed storage implementation.
//!
//! Reads documentation bundles from S3 using the format defined in [`crate::format`].
//! Optimized to minimize S3 requests:
//! - `scan()` loads manifest (1 request), caches it for `exists()` and `mtime()`
//! - `read()` loads page bundle (1 request), caches it so `meta()` is free

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

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
/// **Note:** The page cache grows without bound — every page bundle fetched via
/// `read()` or `meta()` is kept in memory for the lifetime of this instance.
/// This is acceptable when each `S3Storage` serves a single prefix, but callers
/// should be aware of memory usage for very large sites.
pub struct S3Storage {
    client: Client,
    runtime: tokio::runtime::Runtime,
    config: S3Config,
    /// Cached manifest from `scan()`.
    manifest: RwLock<Option<CachedManifest>>,
    /// Cached page bundles from `read()`/`meta()`.
    /// Grows without bound — see struct-level docs.
    page_cache: RwLock<HashMap<String, PageBundle>>,
}

/// Cached manifest data for fast lookups.
struct CachedManifest {
    documents: Vec<Document>,
    /// Set of paths with content for `exists()` checks.
    content_paths: HashSet<String>,
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
            manifest: RwLock::new(None),
            page_cache: RwLock::new(HashMap::new()),
        })
    }

    /// Fetch and parse a JSON file from S3.
    async fn fetch_json<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<T, StorageError> {
        let resp = self
            .client
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
    }

    /// Ensure manifest is loaded, returning cached documents.
    fn ensure_manifest(&self) -> Result<(), StorageError> {
        // Fast path: already cached.
        if self
            .manifest
            .read()
            .expect("manifest lock poisoned")
            .is_some()
        {
            return Ok(());
        }

        // Slow path: fetch from S3.
        let key = s3::build_key(&self.config, MANIFEST_KEY);
        let manifest: Manifest = self.runtime.block_on(self.fetch_json(&key))?;

        if manifest.version != FORMAT_VERSION {
            return Err(StorageError::new(StorageErrorKind::Other)
                .with_backend(BACKEND)
                .with_source(std::io::Error::other(format!(
                    "Unsupported manifest version: {} (expected {FORMAT_VERSION})",
                    manifest.version
                ))));
        }

        let content_paths: HashSet<String> = manifest
            .documents
            .iter()
            .filter(|d| d.has_content)
            .map(|d| d.path.clone())
            .collect();

        // Re-check under write lock to avoid duplicate S3 fetches under concurrency.
        let mut guard = self.manifest.write().expect("manifest lock poisoned");
        if guard.is_none() {
            *guard = Some(CachedManifest {
                documents: manifest.documents,
                content_paths,
            });
        }

        Ok(())
    }

    /// Ensure a page bundle is loaded and cached.
    fn ensure_page_bundle(&self, path: &str) -> Result<(), StorageError> {
        // Fast path: already cached.
        if self
            .page_cache
            .read()
            .expect("page_cache lock poisoned")
            .contains_key(path)
        {
            return Ok(());
        }

        // Slow path: fetch from S3.
        let bundle_key = s3::build_key(&self.config, &format::page_bundle_key(path));
        let bundle: PageBundle = self.runtime.block_on(self.fetch_json(&bundle_key))?;

        // Re-check under write lock to avoid duplicate S3 fetches under concurrency.
        self.page_cache
            .write()
            .expect("page_cache lock poisoned")
            .entry(path.to_owned())
            .or_insert(bundle);

        Ok(())
    }
}

impl Storage for S3Storage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        self.ensure_manifest()?;
        let guard = self.manifest.read().expect("manifest lock poisoned");
        let cached = guard.as_ref().unwrap();
        Ok(cached.documents.clone())
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        self.ensure_page_bundle(path)?;
        let guard = self.page_cache.read().expect("page_cache lock poisoned");
        guard
            .get(path)
            .map(|b| b.content.clone())
            .ok_or_else(|| StorageError::not_found(path).with_backend(BACKEND))
    }

    fn exists(&self, path: &str) -> bool {
        if self.ensure_manifest().is_err() {
            return false;
        }
        let guard = self.manifest.read().expect("manifest lock poisoned");
        guard
            .as_ref()
            .is_some_and(|m| m.content_paths.contains(path))
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        self.ensure_manifest()?;
        let guard = self.manifest.read().expect("manifest lock poisoned");
        if !guard
            .as_ref()
            .is_some_and(|m| m.content_paths.contains(path))
        {
            return Err(StorageError::not_found(path).with_backend(BACKEND));
        }
        Ok(0.0)
    }

    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError> {
        self.ensure_page_bundle(path)?;
        let guard = self.page_cache.read().expect("page_cache lock poisoned");
        Ok(guard.get(path).and_then(|b| b.metadata.clone()))
    }
}
