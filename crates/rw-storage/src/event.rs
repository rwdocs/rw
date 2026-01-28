//! Storage event types for change notification.
//!
//! Provides types for subscribing to storage changes through the [`Storage::watch`](crate::Storage::watch) method.

use std::path::PathBuf;
use std::sync::mpsc;

/// Kind of storage event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageEventKind {
    /// Document was created.
    Created,
    /// Document was modified.
    Modified,
    /// Document was removed.
    Removed,
}

/// A storage change event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageEvent {
    /// Relative path to the document (e.g., "guide.md", "domain/index.md").
    pub path: PathBuf,
    /// Kind of change.
    pub kind: StorageEventKind,
}

/// Receiver for storage events.
///
/// Wraps a [`std::sync::mpsc::Receiver`] for synchronous event delivery.
/// Can be iterated with [`iter()`](Self::iter) or polled with [`recv()`](Self::recv)/[`try_recv()`](Self::try_recv).
pub struct StorageEventReceiver {
    rx: mpsc::Receiver<StorageEvent>,
}

impl StorageEventReceiver {
    /// Create a new receiver from a channel receiver.
    pub(crate) fn new(rx: mpsc::Receiver<StorageEvent>) -> Self {
        Self { rx }
    }

    /// Wait for the next event (blocking).
    ///
    /// Returns `None` when the sender is dropped.
    #[must_use]
    pub fn recv(&self) -> Option<StorageEvent> {
        self.rx.recv().ok()
    }

    /// Try to receive an event without blocking.
    ///
    /// Returns `None` if no event is available or the sender is dropped.
    #[must_use]
    pub fn try_recv(&self) -> Option<StorageEvent> {
        self.rx.try_recv().ok()
    }

    /// Returns an iterator over events.
    ///
    /// Blocks until an event is available. Stops when the sender is dropped.
    pub fn iter(&self) -> impl Iterator<Item = StorageEvent> + '_ {
        self.rx.iter()
    }

    /// Create a no-op receiver that never yields events.
    ///
    /// Used by the default `Storage::watch()` implementation for backends
    /// that don't support change notification.
    pub(crate) fn no_op() -> Self {
        let (_tx, rx) = mpsc::channel();
        Self { rx }
    }
}

/// Handle to stop watching for changes.
///
/// Uses RAII pattern - dropping the handle stops watching automatically.
/// Signals shutdown by dropping the internal channel sender.
pub struct WatchHandle {
    _shutdown: Option<mpsc::Sender<()>>,
}

impl WatchHandle {
    /// Create a new watch handle with a shutdown signal sender.
    ///
    /// When the handle is dropped, the sender is dropped, causing the
    /// receiver to return `Err(RecvError)` which signals shutdown.
    pub(crate) fn new(shutdown: mpsc::Sender<()>) -> Self {
        Self {
            _shutdown: Some(shutdown),
        }
    }

    /// Stop watching immediately (consumes the handle).
    pub fn stop(mut self) {
        self._shutdown.take();
    }

    /// Create a no-op handle that does nothing on drop.
    ///
    /// Used by the default `Storage::watch()` implementation for backends
    /// that don't support change notification.
    pub(crate) fn no_op() -> Self {
        Self { _shutdown: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_event_kind_variants() {
        assert_ne!(StorageEventKind::Created, StorageEventKind::Modified);
        assert_ne!(StorageEventKind::Modified, StorageEventKind::Removed);
        assert_ne!(StorageEventKind::Created, StorageEventKind::Removed);
    }

    #[test]
    fn test_storage_event_creation() {
        let event = StorageEvent {
            path: PathBuf::from("guide.md"),
            kind: StorageEventKind::Modified,
        };

        assert_eq!(event.path, PathBuf::from("guide.md"));
        assert_eq!(event.kind, StorageEventKind::Modified);
    }

    #[test]
    fn test_receiver_recv_blocking() {
        let (tx, rx) = mpsc::channel();
        let receiver = StorageEventReceiver::new(rx);

        let event = StorageEvent {
            path: PathBuf::from("test.md"),
            kind: StorageEventKind::Created,
        };

        tx.send(event.clone()).unwrap();

        let received = receiver.recv();
        assert!(received.is_some());
        assert_eq!(received.unwrap(), event);
    }

    #[test]
    fn test_receiver_recv_on_closed_channel() {
        let (tx, rx) = mpsc::channel();
        let receiver = StorageEventReceiver::new(rx);

        drop(tx);

        let received = receiver.recv();
        assert!(received.is_none());
    }

    #[test]
    fn test_receiver_try_recv_non_blocking() {
        let (_tx, rx) = mpsc::channel();
        let receiver = StorageEventReceiver::new(rx);

        let received = receiver.try_recv();
        assert!(received.is_none());
    }

    #[test]
    fn test_receiver_try_recv_available() {
        let (tx, rx) = mpsc::channel();
        let receiver = StorageEventReceiver::new(rx);

        let event = StorageEvent {
            path: PathBuf::from("test.md"),
            kind: StorageEventKind::Modified,
        };

        tx.send(event.clone()).unwrap();

        let received = receiver.try_recv();
        assert!(received.is_some());
        assert_eq!(received.unwrap(), event);
    }

    #[test]
    fn test_receiver_iter() {
        let (tx, rx) = mpsc::channel();
        let receiver = StorageEventReceiver::new(rx);

        let events = vec![
            StorageEvent {
                path: PathBuf::from("a.md"),
                kind: StorageEventKind::Created,
            },
            StorageEvent {
                path: PathBuf::from("b.md"),
                kind: StorageEventKind::Modified,
            },
        ];

        for event in &events {
            tx.send(event.clone()).unwrap();
        }
        drop(tx);

        let received: Vec<_> = receiver.iter().collect();
        assert_eq!(received, events);
    }

    #[test]
    fn test_receiver_no_op() {
        let receiver = StorageEventReceiver::no_op();

        let received = receiver.try_recv();
        assert!(received.is_none());
    }

    #[test]
    fn test_watch_handle_stop() {
        let (tx, rx) = mpsc::channel();
        let handle = WatchHandle::new(tx);

        handle.stop();

        // Channel should be closed
        assert!(rx.recv().is_err());
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
    fn test_watch_handle_no_op() {
        let handle = WatchHandle::no_op();
        handle.stop(); // Should not panic
    }

    #[test]
    fn test_watch_handle_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<WatchHandle>();
    }

    #[test]
    fn test_receiver_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<StorageEventReceiver>();
    }
}
