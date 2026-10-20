//! Watching directories that hold no pages, such as diagram include
//! directories.

use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;

use notify::{Event, RecursiveMode, Watcher};
use rw_storage::StorageError;

use crate::debouncer::EventDebouncer;
use crate::event::WatchHandle;
use crate::{DEBOUNCE, DRAIN_POLL, storage_event_kind, watch_error};

/// Watch `dirs` recursively, sending the paths that changed in each burst of
/// changes, in whatever form the platform reports them.
///
/// A directory that does not exist yet is not watched. Watching stops when the
/// returned handle is dropped.
///
/// # Errors
///
/// Returns [`StorageError`] if the file watcher cannot be started.
pub fn watch_dirs(
    dirs: &[PathBuf],
) -> Result<(mpsc::Receiver<Vec<PathBuf>>, WatchHandle), StorageError> {
    let (change_tx, change_rx) = mpsc::channel();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let debouncer = Arc::new(EventDebouncer::new(DEBOUNCE));

    let recorder = Arc::clone(&debouncer);
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res
            && let Some(kind) = storage_event_kind(event.kind)
        {
            for path in event.paths {
                recorder.record(path, kind);
            }
        }
    })
    .map_err(watch_error)?;
    for dir in dirs.iter().filter(|dir| dir.is_dir()) {
        watcher
            .watch(dir, RecursiveMode::Recursive)
            .map_err(watch_error)?;
    }

    thread::spawn(move || {
        let _watcher = watcher;
        loop {
            match shutdown_rx.recv_timeout(DRAIN_POLL) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            let changed: Vec<PathBuf> = debouncer
                .drain_ready()
                .into_iter()
                .map(|event| event.path)
                .collect();
            if !changed.is_empty() && change_tx.send(changed).is_err() {
                break;
            }
        }
    });

    Ok((change_rx, WatchHandle::new(shutdown_tx)))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn an_edit_in_a_watched_dir_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("common.iuml");
        fs::write(&file, "A -> B").unwrap();
        let (rx, _handle) = watch_dirs(&[dir.path().to_path_buf()]).unwrap();

        fs::write(&file, "C -> D").unwrap();

        // `FSEvents` may replay the directory's recent history, and reports
        // the canonical path.
        let wanted = file.canonicalize().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(changed)
                    if changed
                        .iter()
                        .any(|path| path.canonicalize().ok().as_ref() == Some(&wanted)) =>
                {
                    return;
                }
                Ok(changed) => seen.extend(changed),
                Err(_) => break,
            }
        }
        panic!("the edit was not reported, got: {seen:?}");
    }

    #[test]
    fn a_missing_dir_is_skipped() {
        let dir = tempfile::tempdir().unwrap();

        assert!(watch_dirs(&[dir.path().join("missing")]).is_ok());
    }
}
