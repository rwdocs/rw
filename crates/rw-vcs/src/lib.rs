use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use gix::bstr::ByteSlice;
use gix::discover::upwards::Options as DiscoverOptions;
use gix::open::Options as OpenOptions;
use gix::revision::walk::Sorting;
use gix::sec::trust::Mapping as TrustMapping;
use gix::{Repository, ThreadSafeRepository};

/// Git-aware file metadata resolver.
///
/// Discovers a git repository from a directory path and provides
/// metadata about tracked files. Falls back to filesystem metadata
/// when git is unavailable.
///
/// Uses [`ThreadSafeRepository`] internally so that `Vcs` is `Send + Sync`
/// and can be stored in types shared across threads (e.g., `FsStorage`).
pub struct Vcs {
    repo: Option<ThreadSafeRepository>,
}

impl Vcs {
    /// Create a new `Vcs` by discovering the git repo from `path`.
    ///
    /// Always succeeds — stores `None` internally if `path` is not
    /// inside a git repository.
    ///
    /// Only the repository's own configuration is read: system config, the
    /// user's global config and `GIT_*` config variables are ignored, so a
    /// page's modification time does not depend on the environment the process
    /// was launched from. Nothing here needs identity, credentials, remotes or
    /// transports, which is what that gives up. None of the caveats here are
    /// errors; the worst outcome in every case is a page reported with its
    /// filesystem mtime instead of its commit time.
    ///
    /// A repository owned by another user is opened at reduced trust, which
    /// caps gix's allocations at 16 MiB
    /// (`gitoxide.objects.allocLimitIfReducedTrust`). Neither the repository's
    /// own config nor `GIT_ALLOC_LIMIT` can lift it — both are untrusted or
    /// unread here. To raise it, add
    /// `config_overrides(["gitoxide.objects.allocLimit=<bytes>"])` to the
    /// options below; API-source config is trusted at every level. Until then a
    /// large enough index fails to load, every file looks dirty, and every page
    /// falls back to its filesystem mtime while [`Vcs::has_repo`] still reports
    /// `true`.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        // Discovery errors when `path` is not an accessible directory.
        // Discovery already walks *upward* to find `.git`, so handing it the
        // nearest existing ancestor finds the same repository instead of
        // failing and dropping every page to filesystem mtimes.
        let start = Self::nearest_existing_dir(path).unwrap_or(path);
        // Discovery stays on gix's defaults; only the open options are narrowed.
        let repo = ThreadSafeRepository::discover_opts(
            start,
            DiscoverOptions::default(),
            // The same isolated options at either trust level. Two consequences
            // worth knowing wherever a checkout's owner differs from the runtime
            // user — `@rwdocs/core` inside a container, say. No `git config
            // --global --add safe.directory` can grant trust back, because gix
            // honors `safe.directory` only from global and system config, which
            // isolation does not read. And `include.path` and `includeIf` are
            // not expanded even inside `.git/config`.
            TrustMapping {
                full: OpenOptions::isolated(),
                reduced: OpenOptions::isolated(),
            },
        )
        .ok();
        Self { repo }
    }

    /// Returns `true` if a git repository was discovered for the path this
    /// `Vcs` was constructed with.
    ///
    /// When `false`, [`Vcs::mtime`] always falls back to filesystem
    /// modification times. Callers that requested git times can use this to
    /// warn about a misconfiguration (git times asked for, but none available).
    #[must_use]
    pub fn has_repo(&self) -> bool {
        self.repo.is_some()
    }

    /// The nearest ancestor of `path` (including `path` itself) that exists and
    /// is a directory, or `None` if none exists.
    ///
    /// Lets [`Vcs::new`] hand `gix::discover` a real directory even when the
    /// caller's path points at a not-yet-created subdirectory.
    fn nearest_existing_dir(path: &Path) -> Option<&Path> {
        path.ancestors().find(|p| p.is_dir())
    }

    /// Returns the most recent modification time across all given paths
    /// as seconds since Unix epoch.
    ///
    /// For each path:
    /// - Clean tracked file → git commit committer timestamp
    /// - Dirty or untracked file → filesystem mtime
    /// - No git repo → filesystem mtime
    ///
    /// Returns the max across all paths.
    #[must_use]
    pub fn mtime(&self, paths: &[&Path]) -> f64 {
        paths
            .iter()
            .filter_map(|p| self.file_mtime(p))
            .fold(0.0_f64, f64::max)
    }

    /// Resolve mtime for a single file.
    fn file_mtime(&self, path: &Path) -> Option<f64> {
        if let Some(sync_repo) = &self.repo {
            let repo = sync_repo.to_thread_local();
            if let Some(rel_path) = Self::repo_relative_path(&repo, path)
                && !Self::is_dirty(&repo, &rel_path)
                && let Some(git_mtime) = Self::git_commit_mtime(&repo, &rel_path)
            {
                return Some(git_mtime);
            }
        }
        fs_mtime(path)
    }

    /// Convert absolute path to repo-relative path.
    fn repo_relative_path(repo: &Repository, path: &Path) -> Option<PathBuf> {
        let workdir = repo.workdir()?;
        path.strip_prefix(workdir).ok().map(PathBuf::from)
    }

    /// Check if a file has uncommitted changes in the working directory.
    ///
    /// Returns `true` if the file is modified, staged, or untracked.
    /// Also returns `true` on any error (conservative fallback to fs mtime).
    ///
    /// Note: This compares raw file content against the index entry hash.
    /// Repositories using git filters (autocrlf, smudge/clean) may see
    /// clean files reported as dirty because the working tree content
    /// differs from the filtered content stored in the index. This is a
    /// conservative failure — the fallback to fs mtime is safe.
    fn is_dirty(repo: &Repository, rel_path: &Path) -> bool {
        let rel_path_bstr = gix::bstr::BString::from(rel_path.as_os_str().as_encoded_bytes());

        let Ok(index) = repo.index_or_empty() else {
            return true;
        };

        let Some(entry) = index.entry_by_path(rel_path_bstr.as_bstr()) else {
            // Not in the index = untracked
            return true;
        };

        let Some(workdir) = repo.workdir() else {
            return true;
        };
        let abs_path = workdir.join(rel_path);
        let Ok(content) = fs::read(&abs_path) else {
            return true;
        };
        let Ok(hash) =
            gix::objs::compute_hash(repo.object_hash(), gix::object::Kind::Blob, &content)
        else {
            return true;
        };
        hash != entry.id
    }

    /// Walk commit history to find the most recent commit that touched the file.
    ///
    /// Returns the committer timestamp (seconds since epoch) of the most recent
    /// commit that changed the file's content. Returns `None` if the file
    /// is not tracked in git history.
    #[allow(clippy::default_trait_access)] // CommitTimeOrder is not publicly exported by gix
    fn git_commit_mtime(repo: &Repository, rel_path: &Path) -> Option<f64> {
        let head = repo.head_commit().ok()?;

        // Check that the file exists in HEAD's tree
        let head_tree = head.tree().ok()?;
        let head_entry = head_tree.lookup_entry_by_path(rel_path).ok()??;
        let head_blob_oid = head_entry.object_id();

        // The HEAD commit's committer time is the candidate — if the file was
        // only ever touched by one commit, this is the answer. `Commit::time`
        // is the committer's, not the author's.
        let head_time = head.time().ok()?;
        #[allow(clippy::cast_precision_loss)] // git timestamps are well within f64 range
        let mut last_change_time = head_time.seconds as f64;

        // Walk backwards from HEAD through first-parent commits only.
        // First-parent traversal avoids interleaving commits from merged
        // branches, which would break the "chain of same blob OID" logic.
        let walk = repo
            .rev_walk([head.id])
            .sorting(Sorting::ByCommitTime(Default::default()))
            .first_parent_only()
            .all()
            .ok()?;

        for info in walk {
            let info = info.ok()?;
            // Skip HEAD itself — we already processed it
            if info.id == head.id {
                continue;
            }

            let commit = info.object().ok()?;
            let tree = commit.tree().ok()?;

            let blob_oid = tree
                .lookup_entry_by_path(rel_path)
                .ok()
                .flatten()
                .map(|e| e.object_id());

            match blob_oid {
                Some(oid) if oid == head_blob_oid => {
                    // File exists and has the same content as the child commit.
                    // The change happened in an earlier commit — keep walking.
                    let commit_time = commit.time().ok()?;
                    #[allow(clippy::cast_precision_loss)]
                    {
                        last_change_time = commit_time.seconds as f64;
                    }
                }
                _ => {
                    // File either doesn't exist in this commit or has different
                    // content. The child commit is the one that last changed it.
                    // last_change_time already holds the child's timestamp.
                    return Some(last_change_time);
                }
            }
        }

        // Reached the root of history — the file was introduced in the
        // oldest commit we saw.
        Some(last_change_time)
    }
}

/// Read a file's filesystem modification time as seconds since the Unix epoch.
///
/// Returns `None` if the file cannot be stat'd. This is the raw filesystem
/// signal, independent of git — used directly by `FsStorage` in filesystem
/// mtime mode, and as the fallback inside [`Vcs::mtime`] for dirty/untracked
/// files.
pub fn fs_mtime(path: &Path) -> Option<f64> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    Some(
        modified
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64()),
    )
}

#[cfg(test)]
mod tests {
    use gix::config::Source;
    use rw_git_fixture::{
        COMMIT_INTERVAL, FIRST_COMMIT_TIME, GitFixture, assert_commit_time, commit_time,
    };

    use super::*;

    /// Get a thread-local Repository from a Vcs instance (for testing internals).
    fn thread_local_repo(vcs: &Vcs) -> Repository {
        vcs.repo.as_ref().unwrap().to_thread_local()
    }

    /// The config sections a repository read from outside itself.
    fn foreign_config_sources(repo: &Repository) -> Vec<Source> {
        repo.config_snapshot()
            .plumbing()
            .sections()
            .map(|section| section.meta().source)
            .filter(|source| {
                !matches!(
                    source,
                    Source::Local | Source::Worktree | Source::Api | Source::Cli
                )
            })
            .collect()
    }

    #[test]
    fn test_dirty_file_detected() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        fixture.commit_all("initial");

        let vcs = Vcs::new(fixture.path());
        let rel_path = PathBuf::from("doc.md");

        // File is clean after commit
        let repo = thread_local_repo(&vcs);
        assert!(!Vcs::is_dirty(&repo, &rel_path));

        // Modify the file — now dirty
        fixture.write("doc.md", "# Modified");
        let repo = thread_local_repo(&vcs);
        assert!(Vcs::is_dirty(&repo, &rel_path));
    }

    #[test]
    fn test_git_commit_mtime_uses_latest_commit() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Version 1");
        fixture.commit_all("v1");
        fixture.write("doc.md", "# Version 2");
        let v2_time = fixture.commit_all("v2");

        let vcs = Vcs::new(fixture.path());
        let repo = thread_local_repo(&vcs);

        let mtime = Vcs::git_commit_mtime(&repo, &PathBuf::from("doc.md")).unwrap();

        // The v2 commit time exactly — not merely "later than v1".
        assert_commit_time(mtime, v2_time);
        assert_eq!(v2_time, FIRST_COMMIT_TIME + COMMIT_INTERVAL);
    }

    #[test]
    fn test_git_commit_mtime_none_for_untracked() {
        let fixture = GitFixture::init();
        fixture.write("initial.md", "# Init");
        fixture.commit_all("initial");

        fixture.write("new.md", "# New");

        let vcs = Vcs::new(fixture.path());
        let repo = thread_local_repo(&vcs);

        assert!(Vcs::git_commit_mtime(&repo, &PathBuf::from("new.md")).is_none());
    }

    #[test]
    fn test_mtime_clean_file_returns_git_time() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        let committed_at = fixture.commit_all("initial");

        let vcs = Vcs::new(fixture.path());
        let mtime = vcs.mtime(&[&fixture.path().join("doc.md")]);

        // The commit time is 2020; the file was written to disk seconds ago.
        // Asserting the exact commit time is what makes this test discriminate
        // between the two sources.
        assert_commit_time(mtime, committed_at);
    }

    #[test]
    fn test_mtime_uses_the_committer_time_not_the_author_time() {
        // `gix::Commit::time` is the committer's, and the rest of this module
        // documents it as such. Every other fixture commit sets the two to the
        // same second, so only a commit that separates them can hold that
        // documentation to account.
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        let authored_at = FIRST_COMMIT_TIME;
        let committed_at = FIRST_COMMIT_TIME + 10 * COMMIT_INTERVAL;
        fixture.commit_all_at("initial", authored_at, committed_at);

        let vcs = Vcs::new(fixture.path());
        let mtime = vcs.mtime(&[&fixture.path().join("doc.md")]);

        assert_commit_time(mtime, committed_at);
    }

    #[test]
    fn test_mtime_dirty_file_returns_fs_time() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        let committed_at = fixture.commit_all("initial");

        // Modify without committing
        fixture.write("doc.md", "# Modified");

        let path = fixture.path().join("doc.md");
        let vcs = Vcs::new(fixture.path());
        let mtime = vcs.mtime(&[&path]);

        // The file's own mtime exactly, not merely "later than the commit" —
        // every value produced after 2020 satisfies that, including a wrong one.
        // Read straight from the filesystem rather than through `fs_mtime`, the
        // very function the fallback uses: an expectation computed by the code
        // under test moves whenever that code does. Both sides are the same
        // quantity, not the result of arithmetic, so exact equality is correct
        // rather than approximate. Scoped to this one assertion: a bare
        // `#[allow]` above an `assert_eq!` statement is silently ignored by
        // rustc (attributes don't reach into macro-invocation statements), so the
        // block is what actually narrows the suppression.
        let on_disk = fs::metadata(&path)
            .and_then(|m| m.modified())
            .expect("the file exists")
            .duration_since(UNIX_EPOCH)
            .expect("a file written today is after the epoch")
            .as_secs_f64();
        #[allow(clippy::float_cmp)]
        {
            assert_eq!(mtime, on_disk);
        }
        assert!(
            mtime > commit_time(committed_at),
            "the fs mtime should be decades after the 2020 commit time",
        );
    }

    #[test]
    fn test_mtime_multiple_paths_returns_max() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Doc");
        fixture.commit_all("first");

        fixture.write("meta.yaml", "title: Doc");
        let meta_time = fixture.commit_all("second");

        let vcs = Vcs::new(fixture.path());
        let mtime = vcs.mtime(&[
            &fixture.path().join("doc.md"),
            &fixture.path().join("meta.yaml"),
        ]);

        // meta.yaml is the newer of the two.
        assert_commit_time(mtime, meta_time);
    }

    #[test]
    fn test_mtime_no_git_repo_returns_fs_time() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("doc.md");
        fs::write(&file, "# Hello").unwrap();

        let vcs = Vcs::new(dir.path());
        let mtime = vcs.mtime(&[&file]);

        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert!(mtime > now - 60.0);
        assert!(mtime <= now);
    }

    #[test]
    fn test_mtime_empty_paths_returns_zero() {
        let fixture = GitFixture::init();
        let vcs = Vcs::new(fixture.path());
        assert!((vcs.mtime(&[]) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_staged_file_is_not_dirty() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        fixture.commit_all("initial");

        // Modify and stage, but don't commit
        fixture.write("doc.md", "# Staged change");
        fixture.git(&["add", "doc.md"]);

        let vcs = Vcs::new(fixture.path());
        let repo = thread_local_repo(&vcs);

        // Staged file: index hash matches working tree, so is_dirty returns false.
        // This means mtime() returns the git commit time of the last commit,
        // not reflecting the staged change. This is acceptable — staged content
        // is snapshotted in the index but not yet part of history.
        assert!(!Vcs::is_dirty(&repo, &PathBuf::from("doc.md")));
    }

    #[test]
    fn discovers_repo_when_given_a_nonexistent_path() {
        // Public contract: `Vcs::new` accepts a non-existent path and still
        // discovers the enclosing repository by climbing to the nearest
        // existing ancestor. No caller in this workspace passes a non-existent
        // path today — `rw-storage-fs` passes the project directory — but
        // `Vcs::new` is public API and this test guards that contract.
        let fixture = GitFixture::init();
        fixture.write("README.md", "# Hello");
        fixture.commit_all("initial");

        let missing_source_dir = fixture.path().join("docs");
        assert!(!missing_source_dir.exists());

        assert!(
            Vcs::new(&missing_source_dir).has_repo(),
            "Vcs should discover the repo by climbing to the existing parent",
        );
    }

    #[test]
    fn opens_repositories_without_reading_ambient_config() {
        // `Vcs` needs no identity, credentials, remotes or transports — only
        // the repository's own config. Reading the invoking user's global
        // config would make a served page's modification time depend on who
        // started the process.
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        fixture.commit_all("initial");

        // Config that lives outside `.git/config` and is reachable from it. A
        // default open expands `include.path`; an isolated one does not, so
        // this is a probe every machine has — unlike the developer's global
        // config, which a bare container lacks.
        fixture.write(".git/included.config", "[rw]\n\tprobe = leaked\n");
        fixture.git(&["config", "include.path", "included.config"]);

        let vcs = Vcs::new(fixture.path());
        let repo = thread_local_repo(&vcs);

        // The control: the same repository through gix's default permissions.
        // It shows what an un-isolated open picks up, so a regression in
        // `Vcs::new` cannot pass by the probe being unreadable to begin with.
        let control = gix::discover(fixture.path()).expect("discover the fixture repository");
        assert!(
            control.config_snapshot().string("rw.probe").is_some(),
            "the control never read the probe, so this test proves nothing",
        );
        assert!(
            repo.config_snapshot().string("rw.probe").is_none(),
            "config reached Vcs from a file outside `.git/config`",
        );

        // Whatever the control reads from global or system config, `Vcs` must
        // read none of it. gix records each section's origin, so this asserts
        // the property directly. Unlike the probe above it is only as strong as
        // the machine's own config: where there is none, both sides are empty.
        let ambient = foreign_config_sources(&control);
        let leaked = foreign_config_sources(&repo);
        assert!(
            leaked.is_empty(),
            "config reached Vcs from outside the repository: {leaked:?} \
             (a default open of the same repository reads {ambient:?})",
        );
    }

    #[test]
    fn test_has_repo_true_inside_repo() {
        let fixture = GitFixture::init();
        assert!(Vcs::new(fixture.path()).has_repo());
    }

    #[test]
    fn test_has_repo_false_outside_any_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!Vcs::new(dir.path()).has_repo());
    }

    #[test]
    fn test_nearest_existing_dir_climbs_to_existing_parent() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("a/b/c"); // none of a, b, c exist
        assert_eq!(Vcs::nearest_existing_dir(&missing), Some(dir.path()));
    }

    #[test]
    fn test_nearest_existing_dir_returns_self_when_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Vcs::nearest_existing_dir(dir.path()), Some(dir.path()));
    }

    #[test]
    fn test_untracked_file_is_dirty() {
        let fixture = GitFixture::init();
        fixture.write("initial.md", "# Init");
        fixture.commit_all("initial");

        let vcs = Vcs::new(fixture.path());
        let repo = thread_local_repo(&vcs);

        // Untracked file should be treated as dirty
        fixture.write("new.md", "# New");
        assert!(Vcs::is_dirty(&repo, &PathBuf::from("new.md")));
    }
}
