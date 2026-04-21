//! CLI command implementations.

pub(crate) mod backstage;
pub(crate) mod comment;
pub(crate) mod confluence;
pub(crate) mod serve;
pub(crate) use backstage::BackstageCommand;
pub(crate) use comment::CommentCommand;
pub(crate) use confluence::ConfluenceCommand;
pub(crate) use serve::ServeArgs;

use clap::Args;
use rw_storage_s3::S3Config;

/// Shared S3 CLI arguments used by backstage publish commands.
#[derive(Args)]
pub(crate) struct S3Args {
    /// Backstage entity (e.g. "default/Component/arch").
    #[arg(long)]
    pub entity: String,

    /// S3 bucket name.
    #[arg(long)]
    pub bucket: String,

    /// S3-compatible endpoint URL (for non-AWS, e.g. Yandex Cloud).
    #[arg(long)]
    pub endpoint: Option<String>,

    /// AWS region.
    #[arg(long, default_value = "us-east-1")]
    pub region: String,

    /// Optional prefix path within the bucket.
    #[arg(long)]
    pub bucket_root_path: Option<String>,
}

impl S3Args {
    pub(crate) fn into_config(self) -> S3Config {
        S3Config {
            bucket: self.bucket,
            prefix: self.entity,
            region: self.region,
            endpoint: self.endpoint,
            bucket_root_path: self.bucket_root_path,
            access_key_id: None,
            secret_access_key: None,
        }
    }
}
