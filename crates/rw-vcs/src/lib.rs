use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use gix::discover::upwards::Options as DiscoverOptions;
use gix::object::Kind as ObjectKind;
use gix::objs::compute_hash;
use gix::open::Options as OpenOptions;
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
    /// was launched from. None of the caveats here are errors; the worst
    /// outcome in every case is a page reported with its filesystem mtime
    /// instead of its commit time.
    ///
    /// A repository owned by another user is opened at reduced trust, which
    /// caps each object gix reads at 16 MiB
    /// (`gitoxide.objects.allocLimitIfReducedTrust`); a file under a larger
    /// tree falls back. Neither the repository's own config nor
    /// `GIT_ALLOC_LIMIT` can lift it — both are untrusted or unread here; only
    /// `config_overrides` in the options below can.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        // Discovery stays on gix's defaults; only the open options are narrowed.
        let repo = ThreadSafeRepository::discover_opts(
            absolute(path),
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

    /// A `Vcs` that never looks for a repository: [`Vcs::mtime`] reports
    /// filesystem modification times only.
    #[must_use]
    pub fn without_repo() -> Self {
        Self { repo: None }
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

    /// Returns the most recent modification time across all given paths
    /// as seconds since Unix epoch.
    ///
    /// For each path:
    /// - File identical to HEAD's → git commit committer timestamp
    /// - Untracked file, or one changed since HEAD (staged or not) → filesystem mtime
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
                && let Some(git_mtime) = Self::git_commit_mtime(&repo, &rel_path)
            {
                return Some(git_mtime);
            }
        }
        fs_mtime(path)
    }

    /// Convert a path to a repo-relative path.
    fn repo_relative_path(repo: &Repository, path: &Path) -> Option<PathBuf> {
        let workdir = repo.workdir()?;
        absolute(path).strip_prefix(workdir).ok().map(PathBuf::from)
    }

    /// Walk commit history to find the most recent commit that touched the file.
    ///
    /// Returns the committer timestamp (seconds since epoch) of the most recent
    /// commit that changed the file's content. Returns `None` if the file is
    /// not in HEAD or its worktree content differs from HEAD's, staged or not.
    ///
    /// The worktree bytes are hashed unfiltered, so with `autocrlf` or
    /// clean/smudge filters a clean file can look changed.
    fn git_commit_mtime(repo: &Repository, rel_path: &Path) -> Option<f64> {
        let head = repo.head_commit().ok()?;

        let head_tree = head.tree().ok()?;
        let head_entry = head_tree.lookup_entry_by_path(rel_path).ok()??;
        let head_blob_oid = head_entry.object_id();

        let content = fs::read(repo.workdir()?.join(rel_path)).ok()?;
        let worktree_blob_oid =
            compute_hash(repo.object_hash(), ObjectKind::Blob, &content).ok()?;
        if worktree_blob_oid != head_blob_oid {
            return None;
        }

        // The HEAD commit's committer time is the candidate — if the file was
        // only ever touched by one commit, this is the answer. `Commit::time`
        // is the committer's, not the author's.
        let head_time = head.time().ok()?;
        #[allow(clippy::cast_precision_loss)] // git timestamps are well within f64 range
        let mut last_change_time = head_time.seconds as f64;

        // Walk backwards from HEAD through first-parent commits only.
        // First-parent traversal avoids interleaving commits from merged
        // branches, which would break the "chain of same blob OID" logic.
        let walk = repo.rev_walk([head.id]).first_parent_only().all().ok()?;

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

/// `path` made absolute against the working directory, with `..` folded
/// lexically the way discovery folds it into the workdir, so the two share a
/// prefix. `""` (the parent of a bare `rw.toml`) is the working directory;
/// `std::path::absolute` rejects it.
fn absolute(path: &Path) -> PathBuf {
    let path = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };
    let Ok(path) = std::path::absolute(path) else {
        return path.to_path_buf();
    };
    let mut folded = PathBuf::new();
    for component in path.components() {
        if component == Component::ParentDir {
            folded.pop();
        } else {
            folded.push(component);
        }
    }
    folded
}

/// Read a file's filesystem modification time as seconds since the Unix epoch.
///
/// Returns `None` if the file cannot be stat'd.
fn fs_mtime(path: &Path) -> Option<f64> {
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

    /// Asserts `mtime` is `path`'s filesystem mtime, read without `fs_mtime`
    /// so the expectation does not move with the code under test.
    fn assert_fs_mtime(mtime: f64, path: &Path) {
        let on_disk = fs::metadata(path)
            .and_then(|m| m.modified())
            .expect("the file exists")
            .duration_since(UNIX_EPOCH)
            .expect("a file written today is after the epoch")
            .as_secs_f64();
        // rustc ignores an `#[allow]` on an `assert_eq!` statement; a block takes it.
        #[allow(clippy::float_cmp)]
        {
            assert_eq!(mtime, on_disk);
        }
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

    // Unix only: on Windows the fixture's temp dir can sit on another drive
    // than the working directory, and then no relative path reaches it.
    #[cfg(unix)]
    #[test]
    fn test_mtime_clean_file_returns_git_time_for_relative_paths() {
        // `rw … -c rw.toml` roots the project at "", so both the discovery path
        // and the file paths arrive relative to the working directory.
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        let committed_at = fixture.commit_all("initial");

        // Changing the working directory would race other tests, so reach the
        // fixture from the current one instead.
        let cwd = std::env::current_dir().unwrap();
        let up: PathBuf = cwd.components().skip(1).map(|_| "..").collect();
        let relative = up.join(fixture.path().strip_prefix("/").unwrap());
        assert!(relative.is_relative());

        let vcs = Vcs::new(&relative);
        let mtime = vcs.mtime(&[&relative.join("doc.md")]);

        assert_commit_time(mtime, committed_at);
    }

    #[test]
    fn test_absolute_treats_empty_path_as_the_working_directory() {
        assert_eq!(absolute(Path::new("")), std::env::current_dir().unwrap());
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

        fixture.write("doc.md", "# Modified");

        let path = fixture.path().join("doc.md");
        let mtime = Vcs::new(fixture.path()).mtime(&[&path]);

        assert_fs_mtime(mtime, &path);
        assert!(mtime > commit_time(committed_at));
    }

    #[test]
    fn test_mtime_untracked_file_returns_fs_time() {
        let fixture = GitFixture::init();
        fixture.write("initial.md", "# Init");
        fixture.commit_all("initial");
        fixture.write("new.md", "# New");

        let path = fixture.path().join("new.md");
        let mtime = Vcs::new(fixture.path()).mtime(&[&path]);

        assert_fs_mtime(mtime, &path);
    }

    #[test]
    fn test_mtime_staged_new_file_returns_fs_time() {
        let fixture = GitFixture::init();
        fixture.write("initial.md", "# Init");
        fixture.commit_all("initial");
        fixture.write("new.md", "# New");
        fixture.git(&["add", "new.md"]);

        let path = fixture.path().join("new.md");
        let mtime = Vcs::new(fixture.path()).mtime(&[&path]);

        assert_fs_mtime(mtime, &path);
    }

    #[test]
    fn test_mtime_partly_staged_file_returns_fs_time() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        fixture.commit_all("initial");
        fixture.write("doc.md", "# Staged");
        fixture.git(&["add", "doc.md"]);
        fixture.write("doc.md", "# Staged\n\nand unstaged");

        let path = fixture.path().join("doc.md");
        let mtime = Vcs::new(fixture.path()).mtime(&[&path]);

        assert_fs_mtime(mtime, &path);
    }

    #[test]
    fn test_mtime_staged_change_reverted_in_worktree_returns_git_time() {
        // The served bytes are HEAD's, whatever the index holds.
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        let committed_at = fixture.commit_all("initial");
        fixture.write("doc.md", "# Staged");
        fixture.git(&["add", "doc.md"]);
        fixture.write("doc.md", "# Hello");

        let mtime = Vcs::new(fixture.path()).mtime(&[&fixture.path().join("doc.md")]);

        assert_commit_time(mtime, committed_at);
    }

    #[test]
    fn test_mtime_follows_first_parent_through_a_merge() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Version 1");
        fixture.write("stable.md", "# Stable");
        let base_time = fixture.commit_all("base");
        let main = fixture.git(&["branch", "--show-current"]);

        // Newer than every commit on main, so a walk ordered by commit time
        // across both parents would visit it first.
        fixture.git(&["switch", "-c", "side"]);
        fixture.write("doc.md", "# Version 2");
        let side_time = FIRST_COMMIT_TIME + 100 * COMMIT_INTERVAL;
        fixture.commit_all_at("side", side_time, side_time);

        fixture.git(&["switch", &main]);
        fixture.write("other.md", "# Other");
        let other_time = fixture.commit_all("other");
        fixture.git(&["merge", "--no-ff", "--no-commit", "side"]);
        let merge_time = fixture.commit_all("merge side");

        let vcs = Vcs::new(fixture.path());
        let mtime = |name: &str| vcs.mtime(&[&fixture.path().join(name)]);

        assert_commit_time(mtime("doc.md"), merge_time);
        assert_commit_time(mtime("other.md"), other_time);
        assert_commit_time(mtime("stable.md"), base_time);
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
    fn test_mtime_staged_file_returns_fs_time() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        fixture.commit_all("initial");

        fixture.write("doc.md", "# Staged change");
        fixture.git(&["add", "doc.md"]);

        let path = fixture.path().join("doc.md");
        let mtime = Vcs::new(fixture.path()).mtime(&[&path]);

        assert_fs_mtime(mtime, &path);
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
}
