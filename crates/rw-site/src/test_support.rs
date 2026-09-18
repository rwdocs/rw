//! Provides local tracing capture and real snapshot fixtures for lookup boundary tests.

use std::io::{self, Write};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::site::SiteSnapshot;
use crate::site_state::SiteStateBuilder;
use rw_storage::Document;

#[derive(Clone, Default)]
struct LogWriter(Arc<Mutex<Vec<u8>>>);

impl Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn capture_warnings<T>(f: impl FnOnce() -> T) -> (T, String) {
    capture_warnings_matching(|_| true, f)
}

pub(crate) fn capture_warnings_matching<T>(
    filter: impl Fn(&tracing::Metadata<'_>) -> bool + Send + Sync + 'static,
    f: impl FnOnce() -> T,
) -> (T, String) {
    use tracing_subscriber::prelude::*;

    let writer = LogWriter::default();
    let output = writer.clone();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::filter_fn(move |meta| {
            *meta.level() <= tracing::Level::WARN && filter(meta)
        }))
        .with(
            tracing_subscriber::fmt::layer()
                .without_time()
                .with_ansi(false)
                .with_target(false)
                .with_writer(move || writer.clone()),
        );
    let result = tracing::subscriber::with_default(subscriber, f);
    let logs = String::from_utf8(output.0.lock().clone()).unwrap();
    (result, logs)
}

pub(crate) fn assert_candidate_order(event: &str, expected: &[(&str, &str)]) {
    assert!(
        event.split_whitespace().any(|word| word == "WARN"),
        "{event}"
    );
    // Ignore presentation separators while preserving ref/path pairing and order.
    let tokens: Vec<_> = event
        .split(|c: char| !(c.is_alphanumeric() || "/:._-".contains(c)))
        .filter(|token| !token.is_empty())
        .collect();
    let mut remaining = tokens.as_slice();
    for &(full_ref, path) in expected {
        let position = remaining
            .iter()
            .position(|token| *token == full_ref)
            .expect(event);
        assert_eq!(remaining.get(position + 1), Some(&path), "{event}");
        remaining = &remaining[position + 2..];
    }
}

pub(crate) fn snapshot_from_yaml(pages: &[(&str, &str, bool)]) -> SiteSnapshot {
    let mut builder = SiteStateBuilder::new();
    for &(path, yaml, has_content) in pages {
        builder.add_document(Document {
            path: path.to_owned(),
            has_content,
            meta: Arc::new(rw_meta::Meta::resolve(None, Some(yaml), path)),
            origin: None,
            is_dir: true,
            diagnostics: Vec::new().into(),
        });
    }
    SiteSnapshot {
        state: builder.build(),
    }
}
