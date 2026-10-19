//! S3 storage backend and bundle publisher for RW.
//!
//! Provides a `Storage` implementation for serving docs from S3
//! and a bundle publisher for uploading docs.
//!
//! The publisher inlines `!include` files from `include_dirs`; C4 metadata
//! `!include` lines are left as written and resolve at read time.
//!
//! An older [`S3Storage`] ignores manifest fields it does not know, so upgrade
//! readers before publishers that write new fields.
//!
//! # Features
//!
//! - Default: `S3Storage` reader and `S3Config`
//! - `publish`: Bundle publisher for uploading docs to S3

pub(crate) mod format;
mod s3;
mod storage;

pub use s3::S3Config;
pub use storage::S3Storage;

#[cfg(feature = "publish")]
mod publisher;

#[cfg(feature = "publish")]
pub use publisher::{BundlePublishError, BundlePublisher, PublishReport};
