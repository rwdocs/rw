//! S3 storage backend and bundle publisher for RW.
//!
//! Provides a `Storage` implementation for serving docs from S3
//! and a bundle publisher for uploading docs.
//!
//! # Explicit-name rollout
//!
//! Manifest version 1 carries optional `name` on flattened documents. Upgrade
//! readers before publishing names: older readers accept but ignore the field,
//! losing its section and diagram identity semantics. Absent-name wire data is
//! unchanged. Filesystem includes expand at publish time; metadata includes
//! remain for reader-time resolution, including those nested in expanded files.
//!
//! # Features
//!
//! - Default: `S3Storage` reader and format types
//! - `publish`: Bundle publisher for uploading docs to S3

pub(crate) mod format;
pub mod s3;
mod storage;

pub use s3::S3Config;
pub use storage::S3Storage;

#[cfg(feature = "publish")]
mod publisher;

#[cfg(feature = "publish")]
pub use publisher::{BundlePublishError, BundlePublisher, PublishReport};
