//! Storage abstraction for RW documentation engine.
//!
//! The [`Storage`] trait abstracts document scanning and content retrieval.
//! Backends live in `rw-storage-fs` and `rw-storage-s3`; `MockStorage` (behind
//! the `mock` feature) serves tests.
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
