//! Live reload manager.
//!
//! Coordinates file watching and WebSocket broadcasting for live reload.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};

use rw_comments::CommitWatcher;
use rw_error::error_chain;
use rw_site::{Site, to_url_path};
use rw_storage::StorageError;
use rw_storage_fs::{FsStorage, StorageEvent, StorageEventKind, WatchHandle, watch_dirs};

/// Event sent to connected WebSocket clients.
///
/// Clone is required by `tokio::sync::broadcast` which delivers a copy to
/// each subscriber.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum ReloadEvent {
    /// Only page content changed (no navigation impact).
    Content { path: String },
    /// Site structure changed (new/removed/renamed pages).
    Structure,
    /// A comment on some page changed; every viewer refetches its own
    /// current page's comments. Carries no path — it is a generic signal.
    Comments,
    /// A diagram file or include some page reads, or an image some page
    /// shows, changed; every viewer refetches its own current page. Carries no
    /// path: which pages read the file is not tracked.
    Files,
}

const CHANNEL_CAPACITY: usize = 100;

/// The longest a comment written elsewhere (the `rw comment` CLI, another
/// server, a browser) waits before viewers refetch.
pub(crate) const COMMENT_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Turns storage changes and comment commits into reload events for WebSocket
/// clients.
pub(crate) struct LiveReloadManager {
    broadcaster: broadcast::Sender<ReloadEvent>,
    // Dropping them stops the watchers. Empty only in tests: a `WatchHandle`
    // can't be built outside `rw-storage-fs`.
    _watch_handles: Vec<WatchHandle>,
    _comment_poll: Option<CommentPoll>,
}

impl LiveReloadManager {
    /// Watch `storage` and `include_dirs` on background threads, invalidating
    /// `site` and broadcasting a reload event for each change.
    ///
    /// # Errors
    ///
    /// Returns an error if the file watcher on `storage` cannot be created.
    pub(crate) fn start(
        site: Arc<Site>,
        storage: &FsStorage,
        include_dirs: &[PathBuf],
    ) -> Result<Self, StorageError> {
        let (rx, handle) = storage.watch()?;
        let (broadcaster, _) = broadcast::channel(CHANNEL_CAPACITY);
        let mut watch_handles = vec![handle];

        // Not fatal: an include dir can be the whole project, which may not
        // fit in the platform's watch limit.
        match watch_dirs(include_dirs) {
            Ok((include_rx, include_handle)) => {
                let site = Arc::clone(&site);
                let broadcaster = broadcaster.clone();
                std::thread::spawn(move || {
                    for changed in &include_rx {
                        Self::handle_include_change(&changed, &site, &broadcaster);
                    }
                });
                watch_handles.push(include_handle);
            }
            Err(e) => tracing::warn!(
                error = %error_chain(&e),
                "Failed to watch diagram include directories; live reload will miss edits to included files"
            ),
        }

        let thread_broadcaster = broadcaster.clone();
        std::thread::spawn(move || {
            for event in &rx {
                Self::handle_storage_event(&event, &site, &thread_broadcaster);
            }
        });

        Ok(Self {
            broadcaster,
            _watch_handles: watch_handles,
            _comment_poll: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn without_watcher() -> Self {
        Self {
            broadcaster: broadcast::channel(CHANNEL_CAPACITY).0,
            _watch_handles: Vec::new(),
            _comment_poll: None,
        }
    }

    /// Also broadcast [`ReloadEvent::Comments`] when `comments` sees a commit.
    pub(crate) fn polling_comments(self, comments: CommitWatcher) -> Self {
        Self {
            _comment_poll: Some(CommentPoll::start(comments, self.broadcaster.clone())),
            ..self
        }
    }

    fn handle_include_change(
        changed: &[PathBuf],
        site: &Site,
        broadcaster: &broadcast::Sender<ReloadEvent>,
    ) {
        if site.has_read_include(changed) {
            let _ = broadcaster.send(ReloadEvent::Files);
        }
    }

    /// Handle a storage event.
    fn handle_storage_event(
        event: &StorageEvent,
        site: &Arc<Site>,
        broadcaster: &broadcast::Sender<ReloadEvent>,
    ) {
        match event {
            StorageEvent::File { path } => {
                if site.has_used_file(path) {
                    let _ = broadcaster.send(ReloadEvent::Files);
                }
            }
            StorageEvent::Page { path, kind } => match kind {
                StorageEventKind::Modified { meta } => {
                    // Any metadata field can reach nav, sections, breadcrumbs,
                    // other pages' link resolution, or the page's own meta, and
                    // all of them are read from the snapshot. `None` also
                    // covers a site that has not loaded since startup, while a
                    // viewer from before a restart still shows this page.
                    let structure_changed = site.page_meta(path).is_none_or(|old| *old != *meta);

                    // Invalidate before broadcasting: a viewer refetches on
                    // receipt and must not read the old metadata.
                    if structure_changed {
                        site.invalidate();
                    }
                    // Unconditional: the viewer only acts on its own page.
                    let _ = broadcaster.send(ReloadEvent::Content {
                        path: to_url_path(path),
                    });
                    if structure_changed {
                        let _ = broadcaster.send(ReloadEvent::Structure);
                    }
                }
                StorageEventKind::Created => {
                    // Broadcast unconditionally. Do NOT gate this on a
                    // snapshot read (e.g., `site.has_page_under(path)`):
                    // any read here triggers a reload that races the
                    // watcher event we just received, and on transient
                    // `storage.scan()` failure returns the pre-event
                    // snapshot — silently dropping the broadcast. See the
                    // `created_broadcasts_*` tests below.
                    site.invalidate();
                    let _ = broadcaster.send(ReloadEvent::Structure);
                }
                StorageEventKind::Removed => {
                    // A removed directory need not be a page itself.
                    let known = site.has_page_under(path).unwrap_or(false);
                    site.invalidate();
                    if known {
                        let _ = broadcaster.send(ReloadEvent::Structure);
                    }
                }
            },
        }
    }

    /// Get a receiver for reload events.
    #[must_use]
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<ReloadEvent> {
        self.broadcaster.subscribe()
    }
}

/// Broadcasts [`ReloadEvent::Comments`] after each poll that saw a commit,
/// until dropped.
struct CommentPoll(JoinHandle<()>);

impl CommentPoll {
    fn start(mut watcher: CommitWatcher, broadcaster: broadcast::Sender<ReloadEvent>) -> Self {
        Self(tokio::spawn(async move {
            let mut ticks = time::interval(COMMENT_POLL_INTERVAL);
            ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
            let mut failing = false;
            loop {
                ticks.tick().await;
                match watcher.changed().await {
                    Ok(changed) => {
                        failing = false;
                        if changed {
                            let _ = broadcaster.send(ReloadEvent::Comments);
                        }
                    }
                    // Not reconnected: a fresh connection would take a new
                    // baseline and swallow the commit this poll missed.
                    Err(e) => {
                        if !failing {
                            tracing::warn!(error = %error_chain(&e), "Failed to poll the comments database; retrying");
                        }
                        failing = true;
                    }
                }
            }
        }))
    }
}

impl Drop for CommentPoll {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;
    use std::fs;
    use std::path::Path;
    use std::slice;
    use std::thread;
    use std::time::{Duration, Instant};

    use rw_kroki::test_support::KrokiStub;
    use rw_site::{Meta, PageRendererConfig};
    use rw_storage::{MockStorage, Storage, StorageErrorKind};

    #[test]
    fn test_content_event_serialization() {
        let event = ReloadEvent::Content {
            path: "/guide".to_owned(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content");
        assert_eq!(json["path"], "/guide");
    }

    #[test]
    fn pathless_events_serialize_as_their_type_alone() {
        for (event, json) in [
            (ReloadEvent::Structure, r#"{"type":"structure"}"#),
            (ReloadEvent::Comments, r#"{"type":"comments"}"#),
        ] {
            assert_eq!(serde_json::to_string(&event).unwrap(), json);
        }
    }

    // Returns a `Site` past its initial load, so any subsequent
    // `reload_if_needed` failure is swallowed and the stale snapshot is
    // returned — the production behavior the Created handler must cope with.
    fn loaded_site(storage: &Arc<MockStorage>) -> Arc<Site> {
        let site = Arc::new(Site::new(
            Arc::clone(storage) as Arc<dyn Storage>,
            Arc::new(rw_cache::NullCache),
            PageRendererConfig::default(),
        ));
        site.navigation(None).expect("initial load");
        site
    }

    #[test]
    fn created_broadcasts_structure_even_when_post_event_reload_would_fail() {
        // The Created handler must broadcast Structure without depending on
        // any post-invalidate snapshot read. Setup arranges a state where any
        // such read would silently swallow a scan error and return the
        // (pre-event) stale snapshot — exactly the race that drops the
        // broadcast.
        let storage = Arc::new(MockStorage::new());
        let site = loaded_site(&storage);

        let (tx, mut rx) = broadcast::channel(8);

        storage.set_scan_error(Some(StorageErrorKind::Unavailable));

        LiveReloadManager::handle_storage_event(
            &StorageEvent::Page {
                path: "foo".into(),
                kind: StorageEventKind::Created,
            },
            &site,
            &tx,
        );

        let event = rx
            .try_recv()
            .expect("Created should broadcast Structure even when reload would fail");
        assert_matches!(event, ReloadEvent::Structure);
        assert!(
            rx.try_recv().is_err(),
            "Created should produce exactly one Structure broadcast",
        );

        // Confirm the broadcast did not depend on a successful scan: a
        // direct read still surfaces the stale snapshot (the scan error
        // is swallowed and `foo` is absent), so any future re-introduction
        // of a post-invalidate `has_page_under` re-check would re-trigger the bug.
        assert!(
            !site
                .has_page_under("foo")
                .expect("scan error is swallowed after initial load")
        );
    }

    fn modified(path: &str, title: &str) -> StorageEvent {
        modified_meta(path, &format!("title: {title}"))
    }

    fn modified_meta(path: &str, meta_yaml: &str) -> StorageEvent {
        StorageEvent::Page {
            path: path.into(),
            kind: StorageEventKind::Modified {
                meta: Meta::resolve(None, Some(meta_yaml), path),
            },
        }
    }

    fn drain(rx: &mut broadcast::Receiver<ReloadEvent>) -> Vec<String> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .map(|event| serde_json::to_string(&event).unwrap())
            .collect()
    }

    #[test]
    fn modified_body_broadcasts_content_only() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&modified("guide", "Guide"), &site, &tx);

        assert_eq!(drain(&mut rx), [r#"{"type":"content","path":"/guide"}"#]);
        site.navigation(None).unwrap();
        assert_eq!(storage.scan_count(), scans, "a body edit must not rescan");
    }

    #[test]
    fn modified_title_broadcasts_content_and_structure() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = loaded_site(&storage);
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&modified("guide", "New"), &site, &tx);

        assert_eq!(
            drain(&mut rx),
            [
                r#"{"type":"content","path":"/guide"}"#,
                r#"{"type":"structure"}"#,
            ]
        );
    }

    #[test]
    fn modified_kind_invalidates_and_broadcasts_structure() {
        let storage = Arc::new(MockStorage::new().with_document_meta("guide", "title: Guide"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(
            &modified_meta("guide", "title: Guide\nkind: system"),
            &site,
            &tx,
        );

        assert_eq!(
            drain(&mut rx),
            [
                r#"{"type":"content","path":"/guide"}"#,
                r#"{"type":"structure"}"#,
            ]
        );
        site.navigation(None).unwrap();
        assert_eq!(storage.scan_count(), scans + 1, "site must be invalidated");
    }

    #[test]
    fn modified_before_first_load_still_reloads_the_page() {
        // A viewer left open across a server restart reconnects its socket
        // without refetching, so the site may never have loaded. Its page
        // must still reload on the next edit.
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = Arc::new(Site::new(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::new(rw_cache::NullCache),
            PageRendererConfig::default(),
        ));
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&modified("guide", "Guide"), &site, &tx);

        assert_eq!(
            drain(&mut rx),
            [
                r#"{"type":"content","path":"/guide"}"#,
                r#"{"type":"structure"}"#,
            ]
        );
    }

    fn removed(path: &str) -> StorageEvent {
        StorageEvent::Page {
            path: path.into(),
            kind: StorageEventKind::Removed,
        }
    }

    #[test]
    fn removed_directory_without_its_own_page_broadcasts_structure() {
        let storage = Arc::new(MockStorage::new().with_document("notes/a", "A"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&removed("notes"), &site, &tx);

        assert_eq!(drain(&mut rx), [r#"{"type":"structure"}"#]);
        site.navigation(None).unwrap();
        assert_eq!(storage.scan_count(), scans + 1, "site must be invalidated");
    }

    #[test]
    fn removed_unknown_path_invalidates_without_broadcast() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&removed("image.png"), &site, &tx);

        assert_eq!(drain(&mut rx), [] as [String; 0]);
        site.navigation(None).unwrap();
        assert_eq!(storage.scan_count(), scans + 1, "site must be invalidated");
    }

    #[test]
    fn created_broadcasts_structure_on_successful_path() {
        // Locks in the simple-path behavior: a Created event with no
        // backend failure still produces a single Structure broadcast.
        let storage = Arc::new(MockStorage::new());
        let site = loaded_site(&storage);

        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(
            &StorageEvent::Page {
                path: "foo".into(),
                kind: StorageEventKind::Created,
            },
            &site,
            &tx,
        );

        let event = rx.try_recv().expect("Created should broadcast Structure");
        assert_matches!(event, ReloadEvent::Structure);
        assert!(
            rx.try_recv().is_err(),
            "Created should produce exactly one Structure broadcast",
        );
    }

    /// A site whose `diag` page includes `common.iuml` from `includes`, and
    /// has been rendered once.
    fn site_including(includes: &Path, kroki: &KrokiStub) -> Arc<Site> {
        fs::write(includes.join("common.iuml"), "Alice -> Bob").unwrap();
        let storage = MockStorage::new()
            .with_file(
                "diag",
                "Diagram",
                "# Diagram\n\n```plantuml\n!include common.iuml\n```\n",
            )
            .with_mtime("diag", 1000.0);
        let site = Arc::new(Site::new(
            Arc::new(storage),
            Arc::new(rw_cache::NullCache),
            PageRendererConfig {
                kroki_url: Some(kroki.url.clone()),
                include_dirs: vec![includes.to_path_buf()],
                project_dir: None,
            },
        ));
        site.render("diag").unwrap();
        site
    }

    #[test]
    fn editing_an_included_file_reloads_open_pages() {
        let project = tempfile::tempdir().unwrap();
        let docs = project.path().join("docs");
        let includes = project.path().join("includes");
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir_all(&includes).unwrap();
        let kroki = KrokiStub::start();
        let site = site_including(&includes, &kroki);
        let storage = FsStorage::new(project.path().to_path_buf(), docs);
        let manager = LiveReloadManager::start(site, &storage, slice::from_ref(&includes)).unwrap();
        let mut rx = manager.subscribe();

        fs::write(includes.join("common.iuml"), "Carol -> Dave").unwrap();

        recv_files(&mut rx);
    }

    #[test]
    fn a_change_to_a_file_no_page_includes_reloads_nothing() {
        let includes = tempfile::tempdir().unwrap();
        let kroki = KrokiStub::start();
        let site = site_including(includes.path(), &kroki);
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_include_change(
            &[includes.path().join("unrelated.iuml")],
            &site,
            &tx,
        );
        assert_eq!(drain(&mut rx), [] as [String; 0]);

        LiveReloadManager::handle_include_change(
            &[includes.path().join("common.iuml")],
            &site,
            &tx,
        );
        assert_eq!(drain(&mut rx), [r#"{"type":"files"}"#]);
    }

    fn file_changed(path: &str) -> StorageEvent {
        StorageEvent::File { path: path.into() }
    }

    #[test]
    fn only_an_image_a_page_shows_reloads_and_none_rescans() {
        let storage = Arc::new(
            MockStorage::new()
                .with_file("guide", "Guide", "# Guide\n\n![a](img/a.png)\n")
                .with_mtime("guide", 1000.0)
                .with_source_dir_in_project("docs"),
        );
        let site = loaded_site(&storage);
        site.render("guide").unwrap();
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&file_changed("docs/guide/img/b.png"), &site, &tx);
        assert_eq!(drain(&mut rx), [] as [String; 0]);

        LiveReloadManager::handle_storage_event(&file_changed("docs/guide/img/a.png"), &site, &tx);
        assert_eq!(drain(&mut rx), [r#"{"type":"files"}"#]);

        site.navigation(None).unwrap();
        assert_eq!(
            storage.scan_count(),
            scans,
            "an image change must not rescan"
        );
    }

    #[test]
    fn editing_a_shown_image_reloads_open_pages() {
        let project = tempfile::tempdir().unwrap();
        let docs = project.path().join("docs");
        fs::create_dir_all(docs.join("img")).unwrap();
        fs::write(docs.join("guide.md"), "# Guide\n\n![a](img/a.png)\n").unwrap();
        fs::write(docs.join("img/a.png"), "old").unwrap();
        let storage = FsStorage::new(project.path().to_path_buf(), docs.clone());
        let site = Arc::new(Site::new(
            Arc::new(FsStorage::new(project.path().to_path_buf(), docs.clone())),
            Arc::new(rw_cache::NullCache),
            PageRendererConfig::default(),
        ));
        site.render("guide").unwrap();
        let manager = LiveReloadManager::start(site, &storage, &[]).unwrap();
        let mut rx = manager.subscribe();

        fs::write(docs.join("img/a.png"), "new").unwrap();

        recv_files(&mut rx);
    }

    fn recv_files(rx: &mut broadcast::Receiver<ReloadEvent>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(ReloadEvent::Files) => return,
                Ok(event) => seen.push(event),
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("no files event, got: {seen:?}");
    }

    /// A site over `project/docs` whose `guide` page shows `flow.puml`, which
    /// includes `parts.iuml`, and `new.puml`, which does not exist yet, and
    /// has been rendered once.
    fn site_with_diagram_files(project: &Path, kroki: &KrokiStub) -> Arc<Site> {
        let docs = project.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(
            docs.join("guide.md"),
            "# Guide\n\n![Flow](flow.puml)\n\n![New](new.puml)\n",
        )
        .unwrap();
        fs::write(
            docs.join("flow.puml"),
            "@startuml\n!include parts.iuml\n@enduml\n",
        )
        .unwrap();
        fs::write(docs.join("parts.iuml"), "Alice -> Bob\n").unwrap();
        fs::write(docs.join("other.puml"), "@startuml\nA -> B\n@enduml\n").unwrap();
        let site = Arc::new(Site::new(
            Arc::new(FsStorage::new(project.to_path_buf(), docs)),
            Arc::new(rw_cache::NullCache),
            PageRendererConfig {
                kroki_url: Some(kroki.url.clone()),
                include_dirs: Vec::new(),
                project_dir: Some(project.to_path_buf()),
            },
        ));
        site.render("guide").unwrap();
        site
    }

    #[test]
    fn only_a_diagram_file_or_include_a_page_read_reloads() {
        let project = tempfile::tempdir().unwrap();
        let kroki = KrokiStub::start();
        let site = site_with_diagram_files(project.path(), &kroki);
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(&file_changed("docs/other.puml"), &site, &tx);
        assert_eq!(drain(&mut rx), [] as [String; 0]);

        for used in ["docs/flow.puml", "docs/parts.iuml", "docs/new.puml"] {
            LiveReloadManager::handle_storage_event(&file_changed(used), &site, &tx);
            assert_eq!(drain(&mut rx), [r#"{"type":"files"}"#], "{used}");
        }
    }

    #[test]
    fn editing_a_diagram_file_reloads_open_pages() {
        writing_reloads_open_pages("flow.puml");
    }

    #[test]
    fn creating_a_missing_diagram_file_reloads_open_pages() {
        writing_reloads_open_pages("new.puml");
    }

    /// Writes `name` in the docs folder of a watched
    /// [`site_with_diagram_files`] and waits for a `files` event.
    fn writing_reloads_open_pages(name: &str) {
        let project = tempfile::tempdir().unwrap();
        let docs = project.path().join("docs");
        let kroki = KrokiStub::start();
        let site = site_with_diagram_files(project.path(), &kroki);
        let storage = FsStorage::new(project.path().to_path_buf(), docs.clone());
        let manager = LiveReloadManager::start(site, &storage, &[]).unwrap();
        let mut rx = manager.subscribe();

        fs::write(docs.join(name), "@startuml\nCarol -> Dave\n@enduml\n").unwrap();

        recv_files(&mut rx);
    }
}
