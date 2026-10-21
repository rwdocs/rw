//! Scan warnings as logged and as returned.
//!
//! Its own test binary: next to the unit tests, which scan on other threads,
//! the scoped subscriber here intermittently received no events.

use std::fs;
use std::io::{self, Write};
use std::sync::Arc;

use parking_lot::Mutex;
use rw_storage::Storage;
use rw_storage_fs::FsStorage;

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl Write for Logs {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn scan_logs_a_warning_once_and_keeps_returning_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("a")).unwrap();
    fs::write(root.join("a.md"), "# A").unwrap();
    fs::write(root.join("a/index.md"), "# A").unwrap();
    fs::write(root.join("b.md"), "# B").unwrap();
    let storage = FsStorage::new(root.to_path_buf(), root.to_path_buf());
    let logs = Logs::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        storage.scan().unwrap();
        storage.scan().unwrap();
        fs::write(root.join("b.meta.yaml"), "namespace: bad/value").unwrap();
        storage.scan().unwrap();
    });

    let logs = String::from_utf8(logs.0.lock().clone()).unwrap();
    assert_eq!(logs.matches("conflicts with").count(), 1, "{logs}");
    assert_eq!(logs.matches("bad/value").count(), 1, "{logs}");
    let warnings = storage.warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
}
