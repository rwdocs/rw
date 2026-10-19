//! Change events delivered by [`FsStorage::watch`](crate::FsStorage::watch).

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
        /// so it compares equal to the scanned [`Document::meta`](rw_storage::Document::meta)
        /// unless the metadata itself changed.
        meta: Meta,
    },
    /// Document was removed.
    Removed,
}

/// A storage change event.
#[derive(Debug, PartialEq, Eq)]
pub struct StorageEvent {
    /// URL path; see [`Storage`](rw_storage::Storage).
    pub path: String,
    /// Kind of change.
    pub kind: StorageEventKind,
}

/// Receiver for storage events; iteration ends when watching stops.
pub type StorageEventReceiver = mpsc::Receiver<StorageEvent>;

/// Handle to stop watching for changes.
///
/// Dropping the handle stops watching: it drops the shutdown sender, so the
/// watcher's `recv()` on the other end returns `Err`.
pub struct WatchHandle {
    _shutdown: mpsc::Sender<()>,
}

impl WatchHandle {
    pub(crate) fn new(shutdown: mpsc::Sender<()>) -> Self {
        Self {
            _shutdown: shutdown,
        }
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
