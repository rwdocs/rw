//! Storage abstraction for RW documentation engine.
//!
//! This crate provides a [`Storage`] trait for abstracting document scanning and content
//! retrieval from the underlying storage backend. This enables:
//!
//! - **Unit testing** without touching the real filesystem
//! - **Backend flexibility** (filesystem, `PostgreSQL`, Redis, S3)
//! - **Clean separation** between site structure logic and I/O operations
//!
//! # Architecture
//!
//! The crate provides:
//! - [`Storage`] trait with `scan()`, `read()`, `exists()`, `mtime()`, `watch()`, and `meta()` methods
//! - [`MockStorage`] for testing (behind `mock` feature flag)
//!
//! For filesystem storage, use the `rw-storage-fs` crate which provides [`FsStorage`](https://docs.rs/rw-storage-fs).
//!
//! # Example
//!
//! ```ignore
//! use std::path::PathBuf;
//! use rw_storage::Storage;
//! use rw_storage_fs::FsStorage;
//!
//! let storage = FsStorage::new(PathBuf::from("docs"));
//! let documents = storage.scan()?;
//! for doc in documents.documents {
//!     println!("{}: {}", doc.path, doc.title);
//! }
//! ```

mod event;
mod metadata;
#[cfg(feature = "mock")]
mod mock;
mod storage;

pub use event::{StorageEvent, StorageEventKind, StorageEventReceiver, WatchHandle};
pub use metadata::{Metadata, MetadataError};
#[cfg(feature = "mock")]
pub use mock::MockStorage;
pub use storage::{Document, ErrorStatus, ScanResult, Storage, StorageError, StorageErrorKind};
