//! Shared S3 client utilities.

use aws_sdk_s3::Client;

/// S3 connection configuration common to both publisher and storage.
pub(crate) struct S3Config {
    pub region: String,
    pub endpoint: Option<String>,
    pub bucket_root_path: Option<String>,
    pub entity: String,
}

/// Build an S3 client from connection configuration.
pub(crate) async fn build_client(config: &S3Config) -> Client {
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

/// Build a full S3 key from a relative path within the bundle.
pub(crate) fn build_key(config: &S3Config, relative_path: &str) -> String {
    let mut parts = Vec::new();
    if let Some(root) = &config.bucket_root_path {
        parts.push(root.as_str());
    }
    parts.push(&config.entity);
    parts.push(relative_path);
    parts.join("/")
}

/// Format an error and its full source chain into a single string.
#[cfg(feature = "publish")]
pub(crate) fn error_chain(err: &dyn std::error::Error) -> String {
    let mut msgs = vec![err.to_string()];
    let mut source = err.source();
    while let Some(s) = source {
        msgs.push(s.to_string());
        source = s.source();
    }
    msgs.join(": ")
}
