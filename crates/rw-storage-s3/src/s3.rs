//! Shared S3 client utilities.

use std::fmt;

use aws_sdk_s3::Client;

use crate::BundlePublishError;

/// S3 bucket configuration shared by storage and publisher.
#[derive(Clone)]
pub struct S3Config {
    /// S3 bucket name.
    pub bucket: String,
    /// S3 key prefix (e.g., `"default/Component/arch"`).
    pub prefix: String,
    /// AWS region.
    pub region: String,
    /// Optional S3-compatible endpoint URL.
    pub endpoint: Option<String>,
    /// Optional prefix path within the bucket.
    pub bucket_root_path: Option<String>,
    /// Optional AWS access key ID for explicit credentials.
    pub access_key_id: Option<String>,
    /// Optional AWS secret access key for explicit credentials.
    pub secret_access_key: Option<String>,
}

impl fmt::Debug for S3Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Config")
            .field("bucket", &self.bucket)
            .field("prefix", &self.prefix)
            .field("region", &self.region)
            .field("endpoint", &self.endpoint)
            .field("bucket_root_path", &self.bucket_root_path)
            .field("access_key_id", &self.access_key_id)
            .field(
                "secret_access_key",
                &self.secret_access_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// Build an S3 client from connection configuration.
pub(crate) async fn build_client(config: &S3Config) -> Client {
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(config.region.clone()));

    if let Some(endpoint) = &config.endpoint {
        loader = loader.endpoint_url(endpoint);
    }

    if let (Some(access_key_id), Some(secret_access_key)) =
        (&config.access_key_id, &config.secret_access_key)
    {
        let credentials = aws_sdk_s3::config::Credentials::new(
            access_key_id,
            secret_access_key,
            None, // session token
            None, // expiry
            "rw", // provider name
        );
        loader = loader.credentials_provider(credentials);
    }

    let sdk_config = loader.load().await;
    let mut s3_builder = aws_sdk_s3::config::Builder::from(&sdk_config);

    if config.endpoint.is_some() {
        s3_builder = s3_builder.force_path_style(true);
    }

    Client::from_conf(s3_builder.build())
}

impl S3Config {
    /// Return the base prefix combining `bucket_root_path` and `prefix`.
    ///
    /// Layout: `{bucket_root_path}/{prefix}` or just `{prefix}`.
    pub fn base_prefix(&self) -> String {
        let root = self
            .bucket_root_path
            .as_deref()
            .unwrap_or("")
            .trim_matches('/');
        if root.is_empty() {
            self.prefix.clone()
        } else {
            format!("{root}/{}", self.prefix)
        }
    }
}

/// Build a full S3 key from a relative path within the bundle.
pub(crate) fn build_key(config: &S3Config, relative_path: &str) -> String {
    format!("{}/{relative_path}", config.base_prefix())
}

/// Upload a single object to S3.
///
/// Builds the full key from the config and relative path, uploads the body,
/// and logs the result.
pub(crate) async fn upload(
    client: &Client,
    config: &S3Config,
    relative_key: &str,
    body: Vec<u8>,
    content_type: &str,
) -> Result<(), BundlePublishError> {
    let key = build_key(config, relative_key);
    client
        .put_object()
        .bucket(&config.bucket)
        .key(&key)
        .body(body.into())
        .content_type(content_type)
        .send()
        .await
        .map_err(|e| BundlePublishError::S3(Box::new(e)))?;
    tracing::debug!(key = %key, "Uploaded");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(bucket_root_path: Option<&str>) -> S3Config {
        S3Config {
            bucket: "bucket".to_owned(),
            prefix: "default/component/x".to_owned(),
            region: "us-east-1".to_owned(),
            endpoint: None,
            bucket_root_path: bucket_root_path.map(str::to_owned),
            access_key_id: None,
            secret_access_key: None,
        }
    }

    #[test]
    fn bucket_root_path_slashes_do_not_change_keys() {
        for root in ["techdocs", "/techdocs", "techdocs/", "/techdocs/"] {
            assert_eq!(
                build_key(&config(Some(root)), "manifest.json"),
                "techdocs/default/component/x/manifest.json",
                "{root:?}"
            );
        }
        for root in [None, Some(""), Some("/")] {
            assert_eq!(
                build_key(&config(root), "manifest.json"),
                "default/component/x/manifest.json",
                "{root:?}"
            );
        }
    }
}
