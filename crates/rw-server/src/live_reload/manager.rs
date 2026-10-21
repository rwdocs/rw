//! Live reload manager.
//!
//! Coordinates file watching and WebSocket broadcasting for live reload.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};

use rw_comments::CommitWatcher;
use rw_error::error_chain;
use rw_site::Site;
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
    /// Watch `storage`, whose source directory is `source_dir`, and
    /// `include_dirs` on background threads, invalidating `site` and
    /// broadcasting a reload event for each change.
    ///
    /// # Errors
    ///
    /// Returns an error if the file watcher on `storage` cannot be created.
    pub(crate) fn start(
        site: Arc<Site>,
        storage: &FsStorage,
        source_dir: &Path,
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
                let source_dir = source_dir.to_path_buf();
                std::thread::spawn(move || {
                    for changed in &include_rx {
                        Self::handle_include_change(&changed, &source_dir, &site, &broadcaster);
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
        let source_dir = source_dir.to_path_buf();
        std::thread::spawn(move || {
            for event in &rx {
                Self::handle_storage_event(&event, &source_dir, &site, &thread_broadcaster);
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
        source_dir: &Path,
        site: &Site,
        broadcaster: &broadcast::Sender<ReloadEvent>,
    ) {
        // The storage watcher reports these too. Only the parent of a removed
        // file still canonicalizes.
        let source_dir = source_dir.canonicalize().ok();
        let changed: Vec<PathBuf> = changed
            .iter()
            .filter(|path| {
                let parent = path.parent().and_then(|parent| parent.canonicalize().ok());
                !parent
                    .zip(source_dir.as_ref())
                    .is_some_and(|(parent, dir)| parent.starts_with(dir))
            })
            .cloned()
            .collect();
        if site.has_read_diagram_source(&changed) {
            let _ = broadcaster.send(ReloadEvent::Files);
        }
    }

    /// Handle a storage event.
    fn handle_storage_event(
        event: &StorageEvent,
        source_dir: &Path,
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
                    } else {
                        site.invalidate_links();
                    }
                    // Unconditional: the viewer only acts on its own page.
                    let _ = broadcaster.send(ReloadEvent::Content {
                        path: format!("/{path}"),
                    });
                    if structure_changed {
                        let _ = broadcaster.send(ReloadEvent::Structure);
                    }
                }
                // Not gated on whether the path is a page: that read reloads an
                // invalidated snapshot, which may already lack a removed page,
                // or, after a failed rescan, still lack a created one.
                StorageEventKind::Created | StorageEventKind::Removed => {
                    site.invalidate();
                    let _ = broadcaster.send(ReloadEvent::Structure);
                    // A moved directory is reported alone, never the files in it.
                    if site.has_used_file_under(&source_dir.join(path)) {
                        let _ = broadcaster.send(ReloadEvent::Files);
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
            Path::new("docs"),
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

        LiveReloadManager::handle_storage_event(
            &modified("guide", "Guide"),
            Path::new("docs"),
            &site,
            &tx,
        );

        assert_eq!(drain(&mut rx), [r#"{"type":"content","path":"/guide"}"#]);
        site.navigation(None).unwrap();
        assert_eq!(storage.scan_count(), scans, "a body edit must not rescan");
    }

    #[test]
    fn modified_title_broadcasts_content_and_structure() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = loaded_site(&storage);
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(
            &modified("guide", "New"),
            Path::new("docs"),
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
    }

    #[test]
    fn modified_kind_invalidates_and_broadcasts_structure() {
        let storage = Arc::new(MockStorage::new().with_document_meta("guide", "title: Guide"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(
            &modified_meta("guide", "title: Guide\nkind: system"),
            Path::new("docs"),
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

        LiveReloadManager::handle_storage_event(
            &modified("guide", "Guide"),
            Path::new("docs"),
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
    }

    fn removed(path: &str) -> StorageEvent {
        StorageEvent::Page {
            path: path.into(),
            kind: StorageEventKind::Removed,
        }
    }

    #[test]
    fn removals_with_no_read_between_each_broadcast_structure() {
        let docs = tempfile::tempdir().unwrap();
        fs::write(docs.path().join("a.md"), "# A").unwrap();
        fs::write(docs.path().join("b.md"), "# B").unwrap();
        let site = Arc::new(Site::new(
            Arc::new(FsStorage::new(
                docs.path().to_path_buf(),
                docs.path().to_path_buf(),
            )),
            Arc::new(rw_cache::NullCache),
            PageRendererConfig::default(),
        ));
        site.navigation(None).unwrap();
        let (tx, mut rx) = broadcast::channel(8);

        fs::remove_file(docs.path().join("a.md")).unwrap();
        fs::remove_file(docs.path().join("b.md")).unwrap();
        LiveReloadManager::handle_storage_event(&removed("a"), docs.path(), &site, &tx);
        LiveReloadManager::handle_storage_event(&removed("b"), docs.path(), &site, &tx);

        assert_eq!(
            drain(&mut rx),
            [r#"{"type":"structure"}"#, r#"{"type":"structure"}"#]
        );
    }

    #[test]
    fn removed_path_that_was_no_page_still_invalidates_and_broadcasts_structure() {
        let storage = Arc::new(MockStorage::new().with_document("guide", "Guide"));
        let site = loaded_site(&storage);
        let scans = storage.scan_count();
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_storage_event(
            &removed("notes.txt"),
            Path::new("docs"),
            &site,
            &tx,
        );

        assert_eq!(drain(&mut rx), [r#"{"type":"structure"}"#]);
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
            Path::new("docs"),
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
        let storage = FsStorage::new(project.path().to_path_buf(), docs.clone());
        let manager =
            LiveReloadManager::start(site, &storage, &docs, slice::from_ref(&includes)).unwrap();
        let mut rx = manager.subscribe();

        fs::write(includes.join("common.iuml"), "Carol -> Dave").unwrap();

        recv_files(&mut rx);
    }

    #[test]
    fn a_change_to_a_file_no_page_includes_reloads_nothing() {
        let docs = tempfile::tempdir().unwrap();
        let includes = tempfile::tempdir().unwrap();
        let kroki = KrokiStub::start();
        let site = site_including(includes.path(), &kroki);
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_include_change(
            &[includes.path().join("unrelated.iuml")],
            docs.path(),
            &site,
            &tx,
        );
        assert_eq!(drain(&mut rx), [] as [String; 0]);

        LiveReloadManager::handle_include_change(
            &[includes.path().join("common.iuml")],
            docs.path(),
            &site,
            &tx,
        );
        assert_eq!(drain(&mut rx), [r#"{"type":"files"}"#]);
    }

    #[test]
    fn an_include_dir_inside_the_docs_leaves_its_files_to_the_storage_watcher() {
        let docs = tempfile::tempdir().unwrap();
        let includes = docs.path().join("diagrams");
        fs::create_dir(&includes).unwrap();
        let kroki = KrokiStub::start();
        let site = site_including(&includes, &kroki);
        let (tx, mut rx) = broadcast::channel(8);

        LiveReloadManager::handle_include_change(
            &[includes.join("common.iuml")],
            docs.path(),
            &site,
            &tx,
        );

        assert_eq!(drain(&mut rx), [] as [String; 0]);
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

        LiveReloadManager::handle_storage_event(
            &file_changed("docs/guide/img/b.png"),
            Path::new("docs"),
            &site,
            &tx,
        );
        assert_eq!(drain(&mut rx), [] as [String; 0]);

        LiveReloadManager::handle_storage_event(
            &file_changed("docs/guide/img/a.png"),
            Path::new("docs"),
            &site,
            &tx,
        );
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
        let manager = LiveReloadManager::start(site, &storage, &docs, &[]).unwrap();
        let mut rx = manager.subscribe();

        fs::write(docs.join("img/a.png"), "new").unwrap();

        recv_files(&mut rx);
    }

    #[test]
    fn renaming_a_directory_of_shown_images_reloads() {
        let project = tempfile::tempdir().unwrap();
        let docs = project.path().join("docs");
        fs::create_dir_all(docs.join("guide/img")).unwrap();
        fs::create_dir_all(docs.join("guide/img2")).unwrap();
        fs::write(docs.join("guide/index.md"), "# Guide\n\n![a](img2/a.png)\n").unwrap();
        fs::write(docs.join("guide/img2/a.png"), "png").unwrap();
        let site = Arc::new(Site::new(
            Arc::new(FsStorage::new(project.path().to_path_buf(), docs.clone())),
            Arc::new(rw_cache::NullCache),
            PageRendererConfig {
                project_dir: Some(project.path().to_path_buf()),
                ..PageRendererConfig::default()
            },
        ));
        site.render("guide").unwrap();
        let (tx, mut rx) = broadcast::channel(8);

        fs::rename(docs.join("guide/img"), docs.join("guide/other")).unwrap();
        LiveReloadManager::handle_storage_event(&removed("guide/img"), &docs, &site, &tx);
        assert_eq!(drain(&mut rx), [r#"{"type":"structure"}"#]);

        fs::rename(docs.join("guide/img2"), docs.join("guide/pics")).unwrap();
        LiveReloadManager::handle_storage_event(&removed("guide/img2"), &docs, &site, &tx);
        assert_eq!(
            drain(&mut rx),
            [r#"{"type":"structure"}"#, r#"{"type":"files"}"#]
        );
    }

    #[test]
    fn a_body_edit_adds_and_removes_a_linked_file() {
        let project = tempfile::tempdir().unwrap();
        let docs = project.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("guide.md"), "# Guide\n").unwrap();
        fs::write(docs.join("spec.pdf"), "pdf").unwrap();
        let site = Arc::new(Site::new(
            Arc::new(FsStorage::new(project.path().to_path_buf(), docs.clone())),
            Arc::new(rw_cache::NullCache),
            PageRendererConfig::default(),
        ));
        assert!(site.read_file("docs/spec.pdf").is_err());
        let body_edit = StorageEvent::Page {
            path: "guide".into(),
            kind: StorageEventKind::Modified {
                meta: (*site.page_meta("guide").unwrap()).clone(),
            },
        };
        let (tx, _rx) = broadcast::channel(8);

        fs::write(docs.join("guide.md"), "# Guide\n\n[spec](spec.pdf)\n").unwrap();
        LiveReloadManager::handle_storage_event(&body_edit, &docs, &site, &tx);
        assert_eq!(site.read_file("docs/spec.pdf").unwrap(), b"pdf");

        fs::write(docs.join("guide.md"), "# Guide\n").unwrap();
        LiveReloadManager::handle_storage_event(&body_edit, &docs, &site, &tx);
        assert!(site.read_file("docs/spec.pdf").is_err());
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

        LiveReloadManager::handle_storage_event(
            &file_changed("docs/other.puml"),
            Path::new("docs"),
            &site,
            &tx,
        );
        assert_eq!(drain(&mut rx), [] as [String; 0]);

        for used in ["docs/flow.puml", "docs/parts.iuml", "docs/new.puml"] {
            LiveReloadManager::handle_storage_event(
                &file_changed(used),
                Path::new("docs"),
                &site,
                &tx,
            );
            assert_eq!(drain(&mut rx), [r#"{"type":"files"}"#], "{used}");
        }
    }

    #[test]
    fn renaming_a_directory_a_page_read_a_diagram_file_from_reloads() {
        let project = tempfile::tempdir().unwrap();
        let docs = project.path().join("docs");
        fs::create_dir_all(docs.join("diagrams")).unwrap();
        fs::write(
            docs.join("guide.md"),
            "# Guide\n\n![Flow](diagrams/flow.puml)\n",
        )
        .unwrap();
        fs::write(
            docs.join("diagrams/flow.puml"),
            "@startuml\nA -> B\n@enduml\n",
        )
        .unwrap();
        let kroki = KrokiStub::start();
        let site = Arc::new(Site::new(
            Arc::new(FsStorage::new(project.path().to_path_buf(), docs.clone())),
            Arc::new(rw_cache::NullCache),
            PageRendererConfig {
                kroki_url: Some(kroki.url.clone()),
                include_dirs: Vec::new(),
                project_dir: Some(project.path().to_path_buf()),
            },
        ));
        site.render("guide").unwrap();
        let (tx, mut rx) = broadcast::channel(8);

        fs::rename(docs.join("diagrams"), docs.join("moved")).unwrap();
        LiveReloadManager::handle_storage_event(&removed("diagrams"), &docs, &site, &tx);

        assert_eq!(
            drain(&mut rx),
            [r#"{"type":"structure"}"#, r#"{"type":"files"}"#]
        );
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
        let manager = LiveReloadManager::start(site, &storage, &docs, &[]).unwrap();
        let mut rx = manager.subscribe();

        fs::write(docs.join(name), "@startuml\nCarol -> Dave\n@enduml\n").unwrap();

        recv_files(&mut rx);
    }
}
