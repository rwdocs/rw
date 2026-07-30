//! Throwaway git repositories for tests, isolated from the developer's git
//! configuration.
//!
//! Tests that shell out to `git` inherit whatever the developer has configured.
//! A `commit.gpgsign = true` whose signing program is unavailable makes
//! `git commit` fail; a global `core.hooksPath` runs hooks that reject the
//! commit; a global `core.excludesFile` makes `git add .` skip files. The
//! resulting failure surfaces far from its cause, or — worse — as a test that
//! passes because the code under test fell back to a filesystem path.
//!
//! [`GitFixture`] applies the isolation recipe that gitoxide uses for its own
//! tests — `configure_command` in `tests/tools/src/lib.rs` of
//! <https://github.com/GitoxideLabs/gitoxide> — to every `git` invocation, and
//! panics with git's own stderr when a command fails.

use std::cell::Cell;
use std::fs;
use std::path::{Component, Path};
use std::process::{Command, Output};

use tempfile::TempDir;

/// Author and committer timestamp of a fixture's first commit:
/// 2020-01-01T00:00:00Z. Comfortably in the past, so a test can tell a commit
/// time from a filesystem mtime by magnitude alone.
pub const FIRST_COMMIT_TIME: i64 = 1_577_836_800;

/// Seconds added to the commit timestamp for each subsequent commit.
pub const COMMIT_INTERVAL: i64 = 60;

/// A commit timestamp in the `f64` seconds that a git-aware mtime resolver
/// reports.
///
/// [`GitFixture::commit_all`] returns commit times as `i64`; callers resolving
/// an mtime get back `f64`. Converting here keeps the cast — and its lint
/// suppression — out of every test.
#[must_use]
#[allow(clippy::cast_precision_loss)] // git timestamps are well within f64 range
pub fn commit_time(seconds: i64) -> f64 {
    seconds as f64
}

/// Assert that a resolved mtime is exactly the commit time it should have come
/// from.
///
/// Both sides are the same value produced the same way — a whole second from
/// a commit — so exact equality is the point, not an approximation that a
/// tolerance would blur. `#[track_caller]` keeps a failure pointing at the
/// test that called this, not at this helper.
#[track_caller]
#[allow(clippy::float_cmp)]
pub fn assert_commit_time(actual: f64, expected: i64) {
    assert_eq!(actual, commit_time(expected));
}

/// Config forced on every invocation, at a precedence that beats the
/// repository's own `.git/config`.
///
/// These are the keys a fixture needs pinned to a known value, not a copy of
/// everything `GIT_CONFIG_GLOBAL` already suppresses: signing off, so a broken
/// signing program cannot fail a commit; `main`, so a global
/// `init.defaultBranch` cannot rename the branch tests assert on; an empty
/// `init.templateDir`, so no template installs hooks into a fresh `.git`; and
/// background maintenance off, so no test races a `gc`. `protocol.file.allow`
/// is inert today — nothing here clones — and is kept only so a fixture that
/// grows a local clone is not broken by a hardened default.
///
/// Deliberately absent: `core.hooksPath` and `core.excludesFile`, the two keys
/// whose global values broke these tests in the first place. Forcing them would
/// make the tests that prove isolation pass by construction. They are dropped
/// by `GIT_CONFIG_NOSYSTEM` and `GIT_CONFIG_GLOBAL` instead, which is the part
/// worth testing.
const ISOLATED_GIT_CONFIG: &[(&str, &str)] = &[
    ("commit.gpgsign", "false"),
    ("tag.gpgsign", "false"),
    ("init.defaultBranch", "main"),
    ("init.templateDir", ""),
    ("protocol.file.allow", "always"),
    ("maintenance.auto", "false"),
    ("gc.auto", "0"),
];

/// Environment variables that would redirect git away from the fixture.
///
/// `GIT_TEMPLATE_DIR` is here rather than in [`ISOLATED_GIT_CONFIG`] because it
/// outranks `init.templateDir`. An exported one installs hooks into
/// `.git/hooks` while `git init` runs — the failure this crate exists to
/// prevent, arriving by environment variable instead of by config key.
///
/// Add a variable here when git would read it to find a repository, a template
/// or a helper program outside the fixture. Do not switch to
/// `Command::env_clear`: the keep-list it needs — `PATH`, `TMPDIR`, and
/// `SYSTEMROOT`/`PATHEXT`/`COMSPEC` on Windows — is harder to get right
/// per-platform than this list.
const INHERITED_VARS_TO_DROP: &[&str] = &[
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_TEMPLATE_DIR",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
];

#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";

/// A git repository in a temporary directory, isolated from ambient config.
///
/// The repository is deleted when the fixture is dropped, so bind it to a
/// variable that outlives every use of [`GitFixture::path`].
pub struct GitFixture {
    dir: TempDir,
    /// `XDG_CONFIG_HOME` for every invocation. It lives outside the working
    /// tree, so anything that ends up creating it cannot be swept into a commit
    /// by `commit_all`'s `git add .`.
    xdg_config_home: TempDir,
    commits: Cell<i64>,
}

impl GitFixture {
    /// Create a temporary directory and `git init` it on branch `main`.
    ///
    /// # Panics
    ///
    /// If the temporary directory cannot be created or `git init` fails.
    pub fn init() -> Self {
        let fixture = Self {
            dir: tempfile::tempdir().expect("temp dir"),
            xdg_config_home: tempfile::tempdir().expect("temp dir for XDG_CONFIG_HOME"),
            commits: Cell::new(0),
        };
        fixture.git(&["init"]);
        fixture
    }

    /// The repository's working directory.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Write a file relative to the repository root, creating parent
    /// directories as needed.
    ///
    /// # Panics
    ///
    /// If `rel_path` resolves outside the fixture, or if the file cannot be
    /// written. A file written outside the temporary directory survives
    /// `TempDir::drop`, so it is refused rather than leaked.
    pub fn write(&self, rel_path: &str, contents: &str) {
        let path = self.path().join(rel_path);
        assert!(
            path.starts_with(self.path()) && !path.components().any(|c| c == Component::ParentDir),
            "{rel_path:?} escapes the fixture directory {}",
            self.path().display(),
        );
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(&path, contents).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }

    /// Stage everything and commit it, returning the commit's Unix timestamp.
    ///
    /// Timestamps are deterministic: the first commit is [`FIRST_COMMIT_TIME`]
    /// and each later one is [`COMMIT_INTERVAL`] seconds after its predecessor.
    /// Both the author and committer times are set, so `%at` and `%ct` agree.
    ///
    /// # Panics
    ///
    /// If `git add` or `git commit` fails.
    pub fn commit_all(&self, message: &str) -> i64 {
        let seconds = FIRST_COMMIT_TIME + self.commits.get() * COMMIT_INTERVAL;
        self.commits.set(self.commits.get() + 1);
        self.commit_all_at(message, seconds, seconds);
        seconds
    }

    /// Stage everything and commit it with the author and committer times set
    /// to different values.
    ///
    /// [`GitFixture::commit_all`] makes the two equal, which cannot tell a
    /// resolver reading the author time apart from one reading the committer
    /// time; only a caller that sets them separately can. This does not advance
    /// the deterministic sequence `commit_all` hands out, so a test mixing the
    /// two picks its own timestamps.
    ///
    /// # Panics
    ///
    /// If `git add` or `git commit` fails.
    pub fn commit_all_at(&self, message: &str, author_seconds: i64, committer_seconds: i64) {
        self.git(&["add", "."]);
        let author_date = format!("@{author_seconds} +0000");
        let committer_date = format!("@{committer_seconds} +0000");
        self.run(
            &["commit", "-m", message],
            &[
                ("GIT_AUTHOR_DATE", author_date.as_str()),
                ("GIT_COMMITTER_DATE", committer_date.as_str()),
            ],
        );
    }

    /// Run a git command in the repository and return its trimmed stdout.
    ///
    /// # Panics
    ///
    /// If git cannot be spawned, or exits non-zero — the panic message carries
    /// the arguments, the exit status, and both captured streams.
    pub fn git(&self, args: &[&str]) -> String {
        self.run(args, &[])
    }

    /// Run one isolated git command with `extra_env` layered on top.
    ///
    /// Every invocation goes through here, so the exit-status check sits on a
    /// single path and each caller names its arguments once.
    fn run(&self, args: &[&str], extra_env: &[(&str, &str)]) -> String {
        let mut cmd = self.command(args);
        for (key, value) in extra_env {
            cmd.env(key, value);
        }
        let output = cmd.output().expect("git should be on PATH");
        assert_success(args, &output);
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// Build a `git` invocation isolated from ambient configuration.
    ///
    /// This is gitoxide's own recipe for its test fixtures (see the module
    /// doc). `git` honors `GIT_CONFIG_NOSYSTEM`, `GIT_CONFIG_GLOBAL` and the
    /// `GIT_CONFIG_COUNT` family, and the last outranks the repository's own
    /// `.git/config`. This env reaches the spawned `git` only — a library
    /// reading git config in-process is unaffected.
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(self.path());

        for var in INHERITED_VARS_TO_DROP {
            cmd.env_remove(var);
        }

        cmd.env("XDG_CONFIG_HOME", self.xdg_config_home.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", NULL_DEVICE)
            .env("GIT_TERMINAL_PROMPT", "false")
            .env("GIT_AUTHOR_NAME", "RW Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@rw.test")
            .env("GIT_COMMITTER_NAME", "RW Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@rw.test");

        cmd.env("GIT_CONFIG_COUNT", ISOLATED_GIT_CONFIG.len().to_string());
        for (index, (key, value)) in ISOLATED_GIT_CONFIG.iter().enumerate() {
            cmd.env(format!("GIT_CONFIG_KEY_{index}"), key)
                .env(format!("GIT_CONFIG_VALUE_{index}"), value);
        }

        cmd
    }
}

/// Panic with everything needed to diagnose a failed git command.
///
/// `Command::output` only errors on a failure to *spawn*, so the exit status
/// must be checked separately or a failed git command surfaces as an unrelated
/// assertion failure later.
fn assert_success(args: &[&str], output: &Output) {
    assert!(
        output.status.success(),
        "git {} failed with {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keys a leak would carry, planted where git looks for global and system
    /// config. `core.hooksPath` and `core.excludesFile` are the two whose
    /// global values broke these tests in the first place; `rw.probe` is a name
    /// no real config sets, so an empty read cannot be a coincidence.
    const PROBE_CONFIG: &str = "[rw]\n\tprobe = leaked\n\
         [core]\n\thooksPath = /nonexistent/hooks\n\texcludesFile = /nonexistent/excludes\n";

    #[test]
    fn ambient_git_config_does_not_reach_the_fixture() {
        // Probe with config this test plants itself rather than the developer's
        // own: whether `~/.config/git/config` happens to set these keys on this
        // machine must not decide whether the isolation is exercised.
        let fixture = GitFixture::init();

        // git reads `$XDG_CONFIG_HOME/git/config` as global config, and the
        // fixture points `XDG_CONFIG_HOME` at a directory of its own — so this
        // is a global config that exists everywhere, including a bare container.
        let global_dir = fixture.xdg_config_home.path().join("git");
        fs::create_dir_all(&global_dir).expect("create the global probe's directory");
        fs::write(global_dir.join("config"), PROBE_CONFIG).expect("write the global probe");

        // System config lives at a build-time path no test may write, so stand
        // in for it with `GIT_CONFIG_SYSTEM`, which `GIT_CONFIG_NOSYSTEM`
        // suppresses exactly as it suppresses the real file.
        let system = fixture.xdg_config_home.path().join("system-config");
        fs::write(&system, PROBE_CONFIG).expect("write the system probe");

        for key in ["rw.probe", "core.hooksPath", "core.excludesFile"] {
            // `git config --get` exits 1 for an unset key, so use the raw
            // command instead of `GitFixture::git`, which panics on non-zero
            // exit.
            let out = fixture
                .command(&["config", "--get", key])
                .env("GIT_CONFIG_SYSTEM", &system)
                .output()
                .expect("git should be on PATH");
            let value = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            assert!(value.is_empty(), "ambient config leaked: {key} = {value:?}");
        }
    }

    #[test]
    #[should_panic(expected = "escapes the fixture directory")]
    fn write_refuses_a_path_outside_the_fixture() {
        // A file written above the temporary directory outlives `TempDir::drop`
        // and is never cleaned up.
        let fixture = GitFixture::init();
        fixture.write("../escape.md", "# Escaped");
    }

    #[test]
    fn git_init_installs_no_hooks() {
        // A template directory — `init.templateDir` or `GIT_TEMPLATE_DIR` —
        // seeds `.git/hooks` while the repository is being created, before any
        // config could switch the hooks off again. git's own default templates
        // install `*.sample` files, so an empty `.git/hooks` is what proves the
        // forced empty `init.templateDir` reached this invocation.
        let fixture = GitFixture::init();
        let hooks = fixture.path().join(".git").join("hooks");

        let installed: Vec<_> = fs::read_dir(&hooks)
            .into_iter()
            .flatten()
            .map(|entry| entry.expect("read .git/hooks entry").file_name())
            .collect();

        assert!(
            installed.is_empty(),
            "git init installed hooks from a template: {installed:?}",
        );
    }

    #[test]
    fn fixture_commits_are_never_signed() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        fixture.commit_all("initial");

        let commit = fixture.git(&["cat-file", "commit", "HEAD"]);
        assert!(
            !commit.contains("gpgsig"),
            "fixture commit was signed:\n{commit}",
        );
    }

    #[test]
    fn commit_timestamps_are_deterministic_and_increasing() {
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# v1");
        let first = fixture.commit_all("v1");
        fixture.write("doc.md", "# v2");
        let second = fixture.commit_all("v2");

        assert_eq!(first, FIRST_COMMIT_TIME);
        assert_eq!(second, FIRST_COMMIT_TIME + COMMIT_INTERVAL);
        assert_eq!(
            fixture.git(&["log", "-1", "--format=%at"]),
            second.to_string()
        );
        assert_eq!(
            fixture.git(&["log", "-1", "--format=%ct"]),
            second.to_string()
        );
    }

    #[test]
    fn commits_land_on_branch_main() {
        // Fixtures commit to `main` whatever the developer's
        // `init.defaultBranch` says and whatever a global hook does with the
        // name. Tests here and in the crates using this fixture assert on that
        // branch, so it is part of the fixture's contract, not an accident of
        // git's default.
        let fixture = GitFixture::init();
        fixture.write("doc.md", "# Hello");
        fixture.commit_all("initial");

        assert_eq!(fixture.git(&["rev-parse", "--abbrev-ref", "HEAD"]), "main");
    }

    #[test]
    // git's own words for this failure, which only its stderr carries — the
    // arguments and the exit status alone would leave the caller guessing.
    #[should_panic(expected = "Needed a single revision")]
    fn a_failing_git_command_panics_with_stderr() {
        let fixture = GitFixture::init();
        fixture.git(&["rev-parse", "--verify", "unknown-revision"]);
    }
}
