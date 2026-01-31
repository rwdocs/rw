//! Event debouncing for storage change notification.
//!
//! Coalesces multiple storage events into single events per path,
//! reducing unnecessary processing when editors emit multiple events per save.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::event::StorageEventKind;

/// A debounced event with file path (not URL path).
///
/// The debouncer works with file system paths internally. Conversion to
/// URL paths happens at the `FsStorage::watch()` boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DebouncedEvent {
    /// File system path.
    pub path: PathBuf,
    /// Kind of change.
    pub kind: StorageEventKind,
}

/// Pending event waiting to be emitted.
struct PendingEvent {
    kind: StorageEventKind,
    deadline: Instant,
}

/// Thread-safe event debouncer.
///
/// Coalesces raw storage events into single events per path using the
/// coalescing rules defined in RD-031.
pub(crate) struct EventDebouncer {
    pending: Mutex<HashMap<PathBuf, PendingEvent>>,
    debounce_duration: Duration,
}

impl EventDebouncer {
    /// Create a new debouncer with the specified debounce duration.
    pub fn new(debounce_duration: Duration) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            debounce_duration,
        }
    }

    /// Record an event.
    ///
    /// Thread-safe, can be called from file system watcher callbacks.
    /// Events are coalesced according to the rules in RD-031.
    pub fn record(&self, path: PathBuf, kind: StorageEventKind) {
        use std::collections::hash_map::Entry;

        let mut pending = self.pending.lock().unwrap();
        let deadline = Instant::now() + self.debounce_duration;

        match pending.entry(path) {
            Entry::Vacant(entry) => {
                entry.insert(PendingEvent { kind, deadline });
            }
            Entry::Occupied(mut entry) => {
                let existing_kind = entry.get().kind;
                if let Some(coalesced_kind) = Self::coalesce(existing_kind, kind) {
                    entry.get_mut().kind = coalesced_kind;
                    entry.get_mut().deadline = deadline;
                } else {
                    // Discard both (Created + Removed = file never existed for us)
                    entry.remove();
                }
            }
        }
    }

    /// Coalesce two event kinds.
    ///
    /// Returns `None` if both events should be discarded (Created + Removed).
    ///
    /// Each arm is documented separately per RD-031 coalescing matrix.
    #[allow(clippy::match_same_arms)]
    fn coalesce(existing: StorageEventKind, new: StorageEventKind) -> Option<StorageEventKind> {
        use StorageEventKind::{Created, Modified, Removed};

        match (existing, new) {
            // Created + anything
            (Created, Created) => Some(Created),  // Duplicate
            (Created, Modified) => Some(Created), // Content included in create
            (Created, Removed) => None,           // File never existed for us

            // Modified + anything
            (Modified, Created) => Some(Created), // File was recreated
            (Modified, Modified) => Some(Modified), // Normal debounce
            (Modified, Removed) => Some(Removed), // File is gone

            // Removed + anything
            (Removed, Created) => Some(Modified), // File was replaced
            (Removed, Modified) => Some(Removed), // Invalid state, ignore new
            (Removed, Removed) => Some(Removed),  // Duplicate
        }
    }

    /// Drain events that have passed their debounce deadline.
    ///
    /// Thread-safe, called from watcher thread.
    /// Returns events with file system paths (not URL paths).
    pub fn drain_ready(&self) -> Vec<DebouncedEvent> {
        let mut pending = self.pending.lock().unwrap();
        let now = Instant::now();

        // Use extract_if when stabilized; for now, collect keys then remove
        let ready_paths: Vec<PathBuf> = pending
            .iter()
            .filter(|(_, event)| event.deadline <= now)
            .map(|(path, _)| path.clone())
            .collect();

        ready_paths
            .into_iter()
            .map(|path| {
                let event = pending.remove(&path).expect("path was just found");
                DebouncedEvent {
                    path,
                    kind: event.kind,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_single_event_emitted_after_deadline() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));
        let path = PathBuf::from("/test/file.md");

        debouncer.record(path.clone(), StorageEventKind::Modified);

        // Before deadline
        let events = debouncer.drain_ready();
        assert!(events.is_empty());

        // Wait for deadline
        thread::sleep(Duration::from_millis(15));

        let events = debouncer.drain_ready();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].path, path);
        assert_eq!(events[0].kind, StorageEventKind::Modified);

        // Should be empty after drain
        let events = debouncer.drain_ready();
        assert!(events.is_empty());
    }

    #[test]
    fn test_multiple_modified_events_coalesce() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));
        let path = PathBuf::from("/test/file.md");

        // Simulate editor saving: multiple modify events
        debouncer.record(path.clone(), StorageEventKind::Modified);
        debouncer.record(path.clone(), StorageEventKind::Modified);
        debouncer.record(path.clone(), StorageEventKind::Modified);

        thread::sleep(Duration::from_millis(15));

        let events = debouncer.drain_ready();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, StorageEventKind::Modified);
    }

    #[test]
    fn test_created_then_modified_stays_created() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));
        let path = PathBuf::from("/test/file.md");

        debouncer.record(path.clone(), StorageEventKind::Created);
        debouncer.record(path.clone(), StorageEventKind::Modified);

        thread::sleep(Duration::from_millis(15));

        let events = debouncer.drain_ready();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, StorageEventKind::Created);
    }

    #[test]
    fn test_created_then_removed_discards_both() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));
        let path = PathBuf::from("/test/file.md");

        debouncer.record(path.clone(), StorageEventKind::Created);
        debouncer.record(path.clone(), StorageEventKind::Removed);

        thread::sleep(Duration::from_millis(15));

        let events = debouncer.drain_ready();
        assert!(events.is_empty());
    }

    #[test]
    fn test_modified_then_removed_keeps_removed() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));
        let path = PathBuf::from("/test/file.md");

        debouncer.record(path.clone(), StorageEventKind::Modified);
        debouncer.record(path.clone(), StorageEventKind::Removed);

        thread::sleep(Duration::from_millis(15));

        let events = debouncer.drain_ready();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, StorageEventKind::Removed);
    }

    #[test]
    fn test_removed_then_created_becomes_modified() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));
        let path = PathBuf::from("/test/file.md");

        debouncer.record(path.clone(), StorageEventKind::Removed);
        debouncer.record(path.clone(), StorageEventKind::Created);

        thread::sleep(Duration::from_millis(15));

        let events = debouncer.drain_ready();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, StorageEventKind::Modified);
    }

    #[test]
    fn test_modified_then_created_keeps_created() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));
        let path = PathBuf::from("/test/file.md");

        debouncer.record(path.clone(), StorageEventKind::Modified);
        debouncer.record(path.clone(), StorageEventKind::Created);

        thread::sleep(Duration::from_millis(15));

        let events = debouncer.drain_ready();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, StorageEventKind::Created);
    }

    #[test]
    fn test_multiple_paths_independent() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));
        let path1 = PathBuf::from("/test/file1.md");
        let path2 = PathBuf::from("/test/file2.md");

        debouncer.record(path1.clone(), StorageEventKind::Modified);
        debouncer.record(path2.clone(), StorageEventKind::Created);

        thread::sleep(Duration::from_millis(15));

        let events = debouncer.drain_ready();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_coalesce_all_combinations() {
        use StorageEventKind::{Created, Modified, Removed};

        // Created + *
        assert_eq!(EventDebouncer::coalesce(Created, Created), Some(Created));
        assert_eq!(EventDebouncer::coalesce(Created, Modified), Some(Created));
        assert_eq!(EventDebouncer::coalesce(Created, Removed), None);

        // Modified + *
        assert_eq!(EventDebouncer::coalesce(Modified, Created), Some(Created));
        assert_eq!(EventDebouncer::coalesce(Modified, Modified), Some(Modified));
        assert_eq!(EventDebouncer::coalesce(Modified, Removed), Some(Removed));

        // Removed + *
        assert_eq!(EventDebouncer::coalesce(Removed, Created), Some(Modified));
        assert_eq!(EventDebouncer::coalesce(Removed, Modified), Some(Removed));
        assert_eq!(EventDebouncer::coalesce(Removed, Removed), Some(Removed));
    }
}
