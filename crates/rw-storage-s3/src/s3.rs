//! Shared S3 client utilities.

use aws_sdk_s3::Client;

/// S3 bucket configuration shared by storage and publisher.
#[derive(Debug, Clone)]
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
}

/// Build an S3 client from connection configuration.
pub async fn build_client(config: &S3Config) -> Client {
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(config.region.clone()));

    if let Some(endpoint) = &config.endpoint {
        loader = loader.endpoint_url(endpoint);
    }

    let sdk_config = loader.load().await;
    let mut s3_builder = aws_sdk_s3::config::Builder::from(&sdk_config);

    if config.endpoint.is_some() {
        s3_builder = s3_builder.force_path_style(true);
    }

    Client::from_conf(s3_builder.build())
}

/// Build a full S3 key from a relative path within the bundle.
pub fn build_key(config: &S3Config, relative_path: &str) -> String {
    match &config.bucket_root_path {
        Some(root) => format!("{root}/{}/{relative_path}", config.prefix),
        None => format!("{}/{relative_path}", config.prefix),
    }
}

/// Upload a single object to S3.
///
/// Builds the full key from the config and relative path, uploads the body,
/// and logs the result. Returns `Err(String)` with the formatted error chain
/// on failure.
pub async fn upload(
    client: &Client,
    config: &S3Config,
    relative_key: &str,
    body: Vec<u8>,
    content_type: &str,
) -> Result<(), String> {
    let key = build_key(config, relative_key);
    client
        .put_object()
        .bucket(&config.bucket)
        .key(&key)
        .body(body.into())
        .content_type(content_type)
        .send()
        .await
        .map_err(|e| error_chain(&e))?;
    tracing::debug!(key = %key, "Uploaded");
    Ok(())
}

/// Format an error and its full source chain into a single string.
pub(crate) fn error_chain(err: &dyn std::error::Error) -> String {
    let mut msgs = vec![err.to_string()];
    let mut source = err.source();
    while let Some(s) = source {
        msgs.push(s.to_string());
        source = s.source();
    }
    msgs.join(": ")
}
