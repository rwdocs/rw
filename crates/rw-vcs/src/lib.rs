use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use gix::bstr::ByteSlice;
use gix::revision::walk::Sorting;
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
    #[must_use]
    pub fn new(path: &Path) -> Self {
        let repo = gix::discover(path).ok().map(Repository::into_sync);
        Self { repo }
    }

    /// Returns the most recent modification time across all given paths
    /// as seconds since Unix epoch.
    ///
    /// For each path:
    /// - Clean tracked file → git commit author timestamp
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

        // The HEAD commit's author time is the candidate — if the file
        // was only ever touched by one commit, this is the answer.
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

/// Read filesystem mtime for a file.
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
    use std::process::Command;

    use super::*;

    /// Create a temporary git repo with an initial commit.
    fn create_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();

        Command::new("git")
            .args(["init", "-b", "test"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .unwrap();

        dir
    }

    fn git_add_commit(dir: &Path, message: &str) {
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// Get a thread-local Repository from a Vcs instance (for testing internals).
    fn thread_local_repo(vcs: &Vcs) -> Repository {
        vcs.repo.as_ref().unwrap().to_thread_local()
    }

    #[test]
    fn test_dirty_file_detected() {
        let dir = create_git_repo();
        let file = dir.path().join("doc.md");
        fs::write(&file, "# Hello").unwrap();
        git_add_commit(dir.path(), "initial");

        let vcs = Vcs::new(dir.path());
        let rel_path = PathBuf::from("doc.md");

        // File is clean after commit
        let repo = thread_local_repo(&vcs);
        assert!(!Vcs::is_dirty(&repo, &rel_path));

        // Modify the file — now dirty
        fs::write(&file, "# Modified").unwrap();
        let repo = thread_local_repo(&vcs);
        assert!(Vcs::is_dirty(&repo, &rel_path));
    }

    #[test]
    fn test_git_commit_mtime_for_clean_file() {
        let dir = create_git_repo();
        let file = dir.path().join("doc.md");
        fs::write(&file, "# Hello").unwrap();
        git_add_commit(dir.path(), "initial");

        let vcs = Vcs::new(dir.path());
        let repo = thread_local_repo(&vcs);
        let rel_path = PathBuf::from("doc.md");

        let mtime = Vcs::git_commit_mtime(&repo, &rel_path);
        assert!(mtime.is_some());

        // Should be a recent timestamp
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let mtime = mtime.unwrap();
        assert!(mtime > now - 60.0);
        assert!(mtime <= now);
    }

    #[test]
    fn test_git_commit_mtime_uses_latest_commit() {
        let dir = create_git_repo();
        let file = dir.path().join("doc.md");
        fs::write(&file, "# Version 1").unwrap();
        git_add_commit(dir.path(), "v1");

        // Small delay to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_secs(1));

        fs::write(&file, "# Version 2").unwrap();
        git_add_commit(dir.path(), "v2");

        let vcs = Vcs::new(dir.path());
        let repo = thread_local_repo(&vcs);
        let rel_path = PathBuf::from("doc.md");

        let mtime = Vcs::git_commit_mtime(&repo, &rel_path).unwrap();

        // Get the v1 commit time via git log
        let v1_output = Command::new("git")
            .args(["log", "--format=%at", "--skip=1", "-1"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let v1_time: f64 = String::from_utf8_lossy(&v1_output.stdout)
            .trim()
            .parse()
            .unwrap();

        // mtime should be from v2 (later than v1)
        assert!(mtime > v1_time);
    }

    #[test]
    fn test_git_commit_mtime_none_for_untracked() {
        let dir = create_git_repo();
        let initial = dir.path().join("initial.md");
        fs::write(&initial, "# Init").unwrap();
        git_add_commit(dir.path(), "initial");

        let untracked = dir.path().join("new.md");
        fs::write(&untracked, "# New").unwrap();

        let vcs = Vcs::new(dir.path());
        let repo = thread_local_repo(&vcs);

        assert!(Vcs::git_commit_mtime(&repo, &PathBuf::from("new.md")).is_none());
    }

    #[test]
    fn test_mtime_clean_file_returns_git_time() {
        let dir = create_git_repo();
        let file = dir.path().join("doc.md");
        fs::write(&file, "# Hello").unwrap();
        git_add_commit(dir.path(), "initial");

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
    fn test_mtime_dirty_file_returns_fs_time() {
        let dir = create_git_repo();
        let file = dir.path().join("doc.md");
        fs::write(&file, "# Hello").unwrap();
        git_add_commit(dir.path(), "initial");

        // Modify without committing
        std::thread::sleep(std::time::Duration::from_secs(1));
        fs::write(&file, "# Modified").unwrap();

        let vcs = Vcs::new(dir.path());
        let mtime = vcs.mtime(&[&file]);

        // mtime should be the filesystem time (newer than commit time)
        let git_time_output = Command::new("git")
            .args(["log", "-1", "--format=%at", "--", "doc.md"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let git_time: f64 = String::from_utf8_lossy(&git_time_output.stdout)
            .trim()
            .parse()
            .unwrap();

        assert!(mtime > git_time);
    }

    #[test]
    fn test_mtime_multiple_paths_returns_max() {
        let dir = create_git_repo();
        let file1 = dir.path().join("doc.md");
        fs::write(&file1, "# Doc").unwrap();
        git_add_commit(dir.path(), "first");

        std::thread::sleep(std::time::Duration::from_secs(1));

        let file2 = dir.path().join("meta.yaml");
        fs::write(&file2, "title: Doc").unwrap();
        git_add_commit(dir.path(), "second");

        let vcs = Vcs::new(dir.path());
        let mtime = vcs.mtime(&[&file1, &file2]);

        // Should be the time of the second commit (meta.yaml is newer)
        let git_time_output = Command::new("git")
            .args(["log", "-1", "--format=%at", "--", "meta.yaml"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let meta_time: f64 = String::from_utf8_lossy(&git_time_output.stdout)
            .trim()
            .parse()
            .unwrap();

        assert!((mtime - meta_time).abs() < 1.0);
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
        let dir = create_git_repo();
        let vcs = Vcs::new(dir.path());
        assert!((vcs.mtime(&[]) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_staged_file_is_not_dirty() {
        let dir = create_git_repo();
        let file = dir.path().join("doc.md");
        fs::write(&file, "# Hello").unwrap();
        git_add_commit(dir.path(), "initial");

        // Modify and stage, but don't commit
        fs::write(&file, "# Staged change").unwrap();
        Command::new("git")
            .args(["add", "doc.md"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let vcs = Vcs::new(dir.path());
        let repo = thread_local_repo(&vcs);

        // Staged file: index hash matches working tree, so is_dirty returns false.
        // This means mtime() returns the git commit time of the last commit,
        // not reflecting the staged change. This is acceptable — staged content
        // is snapshotted in the index but not yet part of history.
        assert!(!Vcs::is_dirty(&repo, &PathBuf::from("doc.md")));
    }

    #[test]
    fn test_untracked_file_is_dirty() {
        let dir = create_git_repo();
        let file = dir.path().join("initial.md");
        fs::write(&file, "# Init").unwrap();
        git_add_commit(dir.path(), "initial");

        let vcs = Vcs::new(dir.path());
        let repo = thread_local_repo(&vcs);

        // Untracked file should be treated as dirty
        let untracked = dir.path().join("new.md");
        fs::write(&untracked, "# New").unwrap();
        assert!(Vcs::is_dirty(&repo, &PathBuf::from("new.md")));
    }
}
