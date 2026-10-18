//! Storage abstraction for RW documentation engine.
//!
//! This crate provides a [`Storage`] trait for abstracting document scanning and content
//! retrieval from the underlying storage backend. This enables:
//!
//! - **Unit testing** without touching the real filesystem
//! - **Backend flexibility** (filesystem, S3)
//! - **Clean separation** between site structure logic and I/O operations
//!
//! # Architecture
//!
//! The crate provides:
//! - [`Storage`] trait with `scan()`, `read()`, and `mtime()` methods
//! - `MockStorage` for testing (behind the `mock` feature flag)
//!
//! For filesystem storage, use the `rw-storage-fs` crate which provides [`FsStorage`](https://docs.rs/rw-storage-fs).
//!
//! # Example
//!
//! ```
//! use rw_storage::Storage;
//!
//! fn list_documents(storage: &dyn Storage) {
//!     let documents = storage.scan().unwrap();
//!     for doc in documents {
//!         assert!(!doc.meta.title.is_empty());
//!     }
//! }
//! ```

#[cfg(feature = "mock")]
mod mock;
mod storage;

#[cfg(feature = "mock")]
pub use mock::MockStorage;
pub use storage::{Document, Storage, StorageError, StorageErrorKind, mtime_to_datetime};
