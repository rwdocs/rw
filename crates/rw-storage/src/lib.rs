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
//! - [`Storage`] trait with `scan()`, `read()`, `exists()`, and `watch()` methods
//! - [`FsStorage`] implementation for filesystem backends with mtime caching
//! - [`MockStorage`] for testing (behind `mock` feature flag)
//!
//! # Example
//!
//! ```ignore
//! use std::path::PathBuf;
//! use rw_storage::{FsStorage, Storage};
//!
//! let storage = FsStorage::new(PathBuf::from("docs"));
//! let documents = storage.scan()?;
//! for doc in documents {
//!     println!("{}: {}", doc.path.display(), doc.title);
//! }
//! ```

pub(crate) mod debouncer;
mod event;
mod fs;
mod metadata;
#[cfg(feature = "mock")]
mod mock;
mod storage;

pub use event::{StorageEvent, StorageEventKind, StorageEventReceiver, WatchHandle};
pub use fs::FsStorage;
pub use metadata::{MetadataError, PageMetadata, merge_metadata};
#[cfg(feature = "mock")]
pub use mock::MockStorage;
pub use storage::{Document, ErrorStatus, ScanResult, Storage, StorageError, StorageErrorKind};
