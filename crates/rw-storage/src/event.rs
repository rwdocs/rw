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

/// Kind of storage event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEventKind {
    /// Document was created.
    Created,
    /// Document was modified.
    Modified {
        /// Resolved page title (meta.yaml > H1 > filename fallback).
        title: String,
        /// Ordered list of child page slugs (from `pages` metadata).
        pages: Option<Vec<String>>,
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
    fn test_storage_event_kind_variants() {
        assert_ne!(
            StorageEventKind::Created,
            StorageEventKind::Modified {
                title: "test".to_owned(),
                pages: None,
            }
        );
        assert_ne!(
            StorageEventKind::Modified {
                title: "test".to_owned(),
                pages: None,
            },
            StorageEventKind::Removed
        );
        assert_ne!(StorageEventKind::Created, StorageEventKind::Removed);
    }

    #[test]
    fn test_storage_event_creation() {
        let event = StorageEvent {
            path: "guide".to_owned(),
            kind: StorageEventKind::Modified {
                title: "Guide".to_owned(),
                pages: None,
            },
        };

        assert_eq!(event.path, "guide");
        assert_eq!(
            event.kind,
            StorageEventKind::Modified {
                title: "Guide".to_owned(),
                pages: None,
            }
        );
    }

    #[test]
    fn test_watch_handle_drop() {
        let (tx, rx) = mpsc::channel();
        let handle = WatchHandle::new(tx);

        drop(handle);

        // Channel should be closed
        assert!(rx.recv().is_err());
    }

    #[test]
    fn test_watch_handle_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<WatchHandle>();
    }
}
