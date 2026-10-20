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

mod file;
#[cfg(feature = "mock")]
mod mock;
mod storage;

pub use file::{MAX_FILE_SIZE, file_content_type};

#[cfg(feature = "mock")]
pub use mock::MockStorage;
pub use storage::{Document, Storage, StorageError, StorageErrorKind, mtime_to_datetime};
