//! S3-backed cache implementation.
//!
//! Implements [`rw_cache::Cache`] and [`rw_cache::CacheBucket`] traits using S3
//! as the backing store. Cache entries are stored as S3 objects with etags in
//! object metadata (`x-amz-meta-etag`).

use aws_sdk_s3::Client;
use aws_sdk_s3::operation::get_object::GetObjectError;
use rw_cache::{Cache, CacheBucket};
use tokio::runtime::Handle;

const ETAG_METADATA_KEY: &str = "cache-etag";

/// S3-backed [`Cache`].
///
/// Creates [`S3CacheBucket`]s that store entries in S3 under
/// `{prefix}/cache/{bucket_name}/{key}`.
///
/// Requires an existing S3 [`Client`] and tokio runtime [`Handle`],
/// allowing the caller to share these with other S3-backed components.
pub struct S3Cache {
    client: Client,
    runtime: Handle,
    bucket: String,
    prefix: String,
}

impl S3Cache {
    /// Create a new S3 cache using the given client and runtime handle.
    pub fn new(client: Client, runtime: Handle, bucket: String, prefix: String) -> Self {
        Self {
            client,
            runtime,
            bucket,
            prefix,
        }
    }
}

impl Cache for S3Cache {
    fn bucket(&self, name: &str) -> Box<dyn CacheBucket> {
        Box::new(S3CacheBucket {
            client: self.client.clone(),
            runtime: self.runtime.clone(),
            s3_bucket: self.bucket.clone(),
            prefix: self.prefix.clone(),
            bucket_name: name.to_owned(),
        })
    }
}

/// Build the full S3 key for a cache entry.
///
/// Layout: `{prefix}/cache/{bucket_name}/{key}`
/// Empty prefix omits the leading segment.
fn build_cache_key(prefix: &str, bucket_name: &str, key: &str) -> String {
    if prefix.is_empty() {
        format!("cache/{bucket_name}/{key}")
    } else {
        format!("{prefix}/cache/{bucket_name}/{key}")
    }
}

/// S3-backed [`CacheBucket`].
struct S3CacheBucket {
    client: Client,
    runtime: Handle,
    s3_bucket: String,
    prefix: String,
    bucket_name: String,
}

impl CacheBucket for S3CacheBucket {
    fn get(&self, key: &str, etag: &str) -> Option<Vec<u8>> {
        let s3_key = build_cache_key(&self.prefix, &self.bucket_name, key);
        self.runtime.block_on(async {
            let resp = self
                .client
                .get_object()
                .bucket(&self.s3_bucket)
                .key(&s3_key)
                .send()
                .await
                .map_err(|e| {
                    if !matches!(e.as_service_error(), Some(GetObjectError::NoSuchKey(_))) {
                        tracing::debug!(key = %s3_key, error = %e, "S3 cache get failed");
                    }
                })
                .ok()?;

            // Check etag if caller provided one
            if !etag.is_empty() {
                let stored_etag = resp
                    .metadata()
                    .and_then(|m| m.get(ETAG_METADATA_KEY))
                    .map_or("", String::as_str);
                if stored_etag != etag {
                    return None;
                }
            }

            let bytes = resp
                .body
                .collect()
                .await
                .map_err(|e| {
                    tracing::debug!(key = %s3_key, error = %e, "S3 cache body read failed");
                })
                .ok()?;
            Some(bytes.into_bytes().to_vec())
        })
    }

    fn set(&self, key: &str, etag: &str, value: &[u8]) {
        let s3_key = build_cache_key(&self.prefix, &self.bucket_name, key);
        let _ = self.runtime.block_on(async {
            self.client
                .put_object()
                .bucket(&self.s3_bucket)
                .key(&s3_key)
                .body(value.to_vec().into())
                .metadata(ETAG_METADATA_KEY, etag)
                .content_type("application/octet-stream")
                .send()
                .await
                .map_err(|e| {
                    tracing::debug!(key = %s3_key, error = %e, "S3 cache set failed");
                })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s3_key_is_built_from_prefix_bucket_and_key() {
        let s3_key = build_cache_key("default/Component/arch", "diagrams", "abc123");
        assert_eq!(s3_key, "default/Component/arch/cache/diagrams/abc123");
    }

    #[test]
    fn s3_key_handles_empty_prefix() {
        let s3_key = build_cache_key("", "diagrams", "abc123");
        assert_eq!(s3_key, "cache/diagrams/abc123");
    }
}
