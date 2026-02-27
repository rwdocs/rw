//! S3-backed storage implementation.
//!
//! Reads documentation bundles from S3 using the format defined in [`crate::format`].
//! Optimized to minimize S3 requests:
//! - `scan()` loads manifest (1 request), caches it for `exists()` and `mtime()`
//! - `read()` loads page bundle (1 request), caches it so `meta()` is free

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use aws_sdk_s3::Client;
use rw_storage::{Document, Metadata, Storage, StorageError, StorageErrorKind};

use crate::format::{self, FORMAT_VERSION, Manifest, PageBundle};

/// Configuration for S3 storage.
#[derive(Debug, Clone)]
pub struct S3StorageConfig {
    /// S3 bucket name.
    pub bucket: String,
    /// Backstage entity identifier (e.g., `"default/Component/arch"`).
    pub entity: String,
    /// AWS region.
    pub region: String,
    /// Optional S3-compatible endpoint URL.
    pub endpoint: Option<String>,
    /// Optional prefix path within the bucket.
    pub bucket_root_path: Option<String>,
}

/// S3-backed storage that reads pre-built documentation bundles.
///
/// Uses a dedicated tokio runtime for async S3 operations within
/// the synchronous `Storage` trait interface.
pub struct S3Storage {
    client: Client,
    runtime: tokio::runtime::Runtime,
    config: S3StorageConfig,
    /// Cached manifest from `scan()`.
    manifest: RwLock<Option<CachedManifest>>,
    /// Cached page bundles from `read()`/`meta()`.
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
    pub fn new(config: S3StorageConfig) -> Result<Self, StorageError> {
        let runtime = tokio::runtime::Runtime::new().map_err(|e| {
            StorageError::new(StorageErrorKind::Other)
                .with_backend("S3")
                .with_source(e)
        })?;

        let client = runtime.block_on(Self::build_client(&config));

        Ok(Self {
            client,
            runtime,
            config,
            manifest: RwLock::new(None),
            page_cache: RwLock::new(HashMap::new()),
        })
    }

    async fn build_client(config: &S3StorageConfig) -> Client {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(config.region.clone()));

        if let Some(endpoint) = &config.endpoint {
            loader = loader.endpoint_url(endpoint);
        }

        let sdk_config = loader.load().await;

        if config.endpoint.is_some() {
            let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
                .force_path_style(true)
                .build();
            return Client::from_conf(s3_config);
        }

        Client::new(&sdk_config)
    }

    /// Build full S3 key from a relative path within the bundle.
    fn build_key(&self, relative_path: &str) -> String {
        let mut parts = Vec::new();
        if let Some(root) = &self.config.bucket_root_path {
            parts.push(root.as_str());
        }
        parts.push(&self.config.entity);
        parts.push(relative_path);
        parts.join("/")
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
                let kind = if e.to_string().contains("NoSuchKey")
                    || e.to_string().contains("not found")
                {
                    StorageErrorKind::NotFound
                } else {
                    StorageErrorKind::Unavailable
                };
                StorageError::new(kind)
                    .with_backend("S3")
                    .with_path(key)
                    .with_source(std::io::Error::other(format!("{e}")))
            })?;

        let bytes = resp.body.collect().await.map_err(|e| {
            StorageError::new(StorageErrorKind::Other)
                .with_backend("S3")
                .with_path(key)
                .with_source(std::io::Error::other(format!("{e}")))
        })?;

        serde_json::from_slice(&bytes.into_bytes()).map_err(|e| {
            StorageError::new(StorageErrorKind::Other)
                .with_backend("S3")
                .with_path(key)
                .with_source(e)
        })
    }

    /// Ensure manifest is loaded, returning cached documents.
    fn ensure_manifest(&self) -> Result<(), StorageError> {
        if self.manifest.read().unwrap().is_some() {
            return Ok(());
        }

        let key = self.build_key("manifest.json");
        let manifest: Manifest = self.runtime.block_on(self.fetch_json(&key))?;

        if manifest.version != FORMAT_VERSION {
            return Err(StorageError::new(StorageErrorKind::Other)
                .with_backend("S3")
                .with_source(std::io::Error::other(format!(
                    "Unsupported manifest version: {} (expected {FORMAT_VERSION})",
                    manifest.version
                ))));
        }

        let documents: Vec<Document> = manifest
            .documents
            .iter()
            .map(|d| Document {
                path: d.path.clone(),
                title: d.title.clone(),
                has_content: d.has_content,
                page_type: d.page_type.clone(),
                description: d.description.clone(),
            })
            .collect();

        let content_paths: HashSet<String> = documents
            .iter()
            .filter(|d| d.has_content)
            .map(|d| d.path.clone())
            .collect();

        *self.manifest.write().unwrap() = Some(CachedManifest {
            documents,
            content_paths,
        });

        Ok(())
    }

    /// Ensure a page bundle is loaded and cached.
    fn ensure_page_bundle(&self, path: &str) -> Result<(), StorageError> {
        if self.page_cache.read().unwrap().contains_key(path) {
            return Ok(());
        }

        let bundle_key = self.build_key(&format::page_bundle_key(path));
        let bundle: PageBundle = self.runtime.block_on(self.fetch_json(&bundle_key))?;

        self.page_cache
            .write()
            .unwrap()
            .insert(path.to_owned(), bundle);

        Ok(())
    }
}

impl Storage for S3Storage {
    fn scan(&self) -> Result<Vec<Document>, StorageError> {
        self.ensure_manifest()?;
        let guard = self.manifest.read().unwrap();
        let cached = guard.as_ref().unwrap();
        Ok(cached.documents.clone())
    }

    fn read(&self, path: &str) -> Result<String, StorageError> {
        self.ensure_page_bundle(path)?;
        let guard = self.page_cache.read().unwrap();
        guard
            .get(path)
            .map(|b| b.content.clone())
            .ok_or_else(|| StorageError::not_found(path).with_backend("S3"))
    }

    fn exists(&self, path: &str) -> bool {
        if self.ensure_manifest().is_err() {
            return false;
        }
        let guard = self.manifest.read().unwrap();
        guard
            .as_ref()
            .is_some_and(|m| m.content_paths.contains(path))
    }

    fn mtime(&self, path: &str) -> Result<f64, StorageError> {
        if !self.exists(path) {
            return Err(StorageError::not_found(path).with_backend("S3"));
        }
        Ok(0.0)
    }

    fn meta(&self, path: &str) -> Result<Option<Metadata>, StorageError> {
        self.ensure_page_bundle(path)?;
        let guard = self.page_cache.read().unwrap();
        Ok(guard.get(path).and_then(|b| b.metadata.clone()))
    }
}
