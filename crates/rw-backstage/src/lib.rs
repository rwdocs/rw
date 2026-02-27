//! Backstage integration for RW.
//!
//! Provides S3 bundle publishing and a `Storage` implementation
//! for serving docs from S3.
//!
//! # Features
//!
//! - Default: `S3Storage` reader and format types
//! - `publish`: Bundle publisher for uploading docs to S3

pub mod format;
mod s3;
mod storage;

pub use storage::{S3Storage, S3StorageConfig};

#[cfg(feature = "publish")]
mod publisher;

#[cfg(feature = "publish")]
pub use publisher::{BackstagePublisher, PublishConfig, PublishError};
