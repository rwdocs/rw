//! Storage event types for change notification.
//!
//! Provides types for subscribing to storage changes through the [`Storage::watch`](crate::Storage::watch) method.
//!
//! # URL Paths
//!
//! Events contain URL paths, not file paths:
//! - `""` - root page changed
//! - `"guide"` - guide page changed
//! - `"domain/billing"` - nested page changed

use std::sync::mpsc;

use rw_meta::Meta;

/// Kind of storage event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEventKind {
    /// Document was created.
    Created,
    /// Document was modified.
    Modified {
        /// The document's metadata after the change, resolved as `scan` would,
        /// so it compares equal to the scanned [`Document::meta`](crate::Document::meta)
        /// unless the metadata itself changed.
        meta: Meta,
    },
    /// Document was removed.
    Removed,
}

/// A storage change event.
#[derive(Debug, PartialEq, Eq)]
pub struct StorageEvent {
    /// URL path (e.g., "", "guide", "domain/billing").
    pub path: String,
    /// Kind of change.
    pub kind: StorageEventKind,
}

/// Receiver for storage events; iteration ends when the backend stops watching.
pub type StorageEventReceiver = mpsc::Receiver<StorageEvent>;

/// Handle to stop watching for changes.
///
/// Dropping the handle stops watching: it drops the shutdown sender, so the
/// watcher's `recv()` on the other end returns `Err`.
pub struct WatchHandle {
    _shutdown: Option<mpsc::Sender<()>>,
}

impl WatchHandle {
    /// Create a watch handle that signals shutdown through `shutdown` on drop.
    #[must_use]
    pub fn new(shutdown: mpsc::Sender<()>) -> Self {
        Self {
            _shutdown: Some(shutdown),
        }
    }

    /// Create a no-op handle that does nothing on drop.
    ///
    /// Used by the default `Storage::watch()` implementation for backends
    /// that don't support change notification.
    #[must_use]
    pub fn no_op() -> Self {
        Self { _shutdown: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_handle_drop() {
        let (tx, rx) = mpsc::channel();
        let handle = WatchHandle::new(tx);

        drop(handle);

        // Channel should be closed
        assert!(rx.recv().is_err());
    }
}
