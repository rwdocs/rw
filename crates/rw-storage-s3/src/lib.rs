//! S3 storage backend and bundle publisher for RW.
//!
//! Provides a `Storage` implementation for serving docs from S3
//! and a bundle publisher for uploading docs.
//!
//! # Features
//!
//! - Default: `S3Storage` reader and format types
//! - `publish`: Bundle publisher for uploading docs to S3

pub mod format;
pub mod s3;
mod storage;

pub use s3::S3Config;
pub use storage::S3Storage;

#[cfg(feature = "publish")]
mod publisher;

#[cfg(feature = "publish")]
pub use publisher::{BundlePublisher, PublishError};
