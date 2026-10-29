//! Whether a path is written the way the filesystem stores it.
//!
//! macOS and Windows open `img/Logo.PNG` when the file is `img/logo.png`;
//! Linux, Git and S3 do not. Refusing such a path on every OS makes a preview
//! on a Mac fail where the Linux publish would.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

/// Why [`spelled_as_on_disk`] refused a path.
#[derive(Debug)]
pub enum SpellingError {
    /// The path resolved only because the filesystem matched a name stored
    /// differently: in another case or, on Windows, as an 8.3 short name or
    /// without a trailing dot. `on_disk` is the written path relative to the
    /// root with each name as stored, a symlink by its own name rather than
    /// its target's. It is the canonical path instead when the written path
    /// holds `..` or two links in one directory lead to the same place.
    Misspelled { on_disk: PathBuf },
    /// A directory on the path could not be listed, so its spelling is
    /// unknown.
    Unlisted(io::Error),
}

/// Checks that `written`, a relative path that resolved to `canonical_target`
/// under `canonical_root`, names each component as its directory stores it.
///
/// `written` must be relative and `canonical_target` must lie inside
/// `canonical_root`: callers confine paths first. A `..` in `written` is
/// not checked; the walk follows it as `canonicalize` did.
///
/// Case is never folded here, because filesystems disagree on what folds
/// (APFS opens `straße` for `STRASSE`, HFS+ does not): the filesystem decides
/// what exists, and an exact comparison decides the spelling. Names compare in
/// NFC, because HFS+ stores NFD and APFS keeps whichever form a file was
/// created with.
///
/// # Errors
///
/// [`SpellingError::Misspelled`] when a component of `written` is not in its
/// directory's listing, [`SpellingError::Unlisted`] when a listing cannot be
/// read.
pub fn spelled_as_on_disk(
    canonical_root: &Path,
    written: &Path,
    canonical_target: &Path,
) -> Result<(), SpellingError> {
    debug_assert!(
        !written
            .components()
            .any(|component| matches!(component, Component::RootDir | Component::Prefix(_))),
        "not a relative path: {}",
        written.display()
    );
    debug_assert!(
        canonical_target.starts_with(canonical_root),
        "{} is outside {}",
        canonical_target.display(),
        canonical_root.display()
    );
    let stored = canonical_target
        .strip_prefix(canonical_root)
        .unwrap_or(canonical_target);
    // `canonicalize` returns the stored spelling on macOS and Windows, and on
    // Linux a misspelled path does not resolve, so a match is exact. A
    // mismatch is a symlink, a `..` or an alias, which only the listings tell
    // apart.
    if same_path(stored, written) {
        return Ok(());
    }
    let mut dir = canonical_root.to_path_buf();
    let mut components = written.components();
    while let Some(component) = components.next() {
        if let Component::Normal(name) = component
            && !listed(&dir, name).map_err(SpellingError::Unlisted)?
        {
            let rest = Path::new(name).join(components.as_path());
            let on_disk = hint(canonical_root, &dir, &rest).unwrap_or_else(|| stored.to_path_buf());
            return Err(SpellingError::Misspelled { on_disk });
        }
        dir.push(component);
    }
    Ok(())
}

/// `dir`, relative to the root as written, joined with `rest` as stored, so a
/// hint through a symlink names the link rather than its target.
fn hint(root: &Path, dir: &Path, rest: &Path) -> Option<PathBuf> {
    let mut on_disk = dir.strip_prefix(root).ok()?.to_path_buf();
    if on_disk
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return None;
    }
    let mut dir = dir.to_path_buf();
    for component in rest.components() {
        let Component::Normal(name) = component else {
            return None;
        };
        let stored = stored_name(&dir, name)?;
        dir.push(&stored);
        on_disk.push(stored);
    }
    Some(on_disk)
}

/// The entry of `dir` that `name` opens. `canonicalize` spells it unless it
/// is a link, which is found as the only link in `dir` resolving to the same
/// place, so it is not mistaken for its target sitting next to it.
fn stored_name(dir: &Path, name: &OsStr) -> Option<OsString> {
    if listed(dir, name).ok()? {
        return Some(name.to_owned());
    }
    let path = dir.join(name);
    let target = fs::canonicalize(&path).ok()?;
    if !fs::symlink_metadata(&path).ok()?.is_symlink() {
        return target.file_name().map(OsStr::to_owned);
    }
    let mut found = None;
    for entry in fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        if entry.file_type().is_ok_and(|kind| kind.is_symlink())
            && fs::canonicalize(entry.path()).is_ok_and(|entry_target| entry_target == target)
            && found.replace(entry.file_name()).is_some()
        {
            return None;
        }
    }
    found
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.iter().count() == b.iter().count() && a.iter().zip(b).all(|(a, b)| same_name(a, b))
}

fn listed(dir: &Path, name: &OsStr) -> io::Result<bool> {
    for entry in fs::read_dir(dir)? {
        if same_name(&entry?.file_name(), name) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn same_name(a: &OsStr, b: &OsStr) -> bool {
    a == b || matches!((a.to_str(), b.to_str()), (Some(a), Some(b)) if a.nfc().eq(b.nfc()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use super::{SpellingError, spelled_as_on_disk};

    /// A temporary project and its canonical path.
    fn project() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical tempdir");
        (dir, root)
    }

    fn write(root: &Path, path: &str) {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, "").unwrap();
    }

    /// Checks `written` the way a caller does: resolve it, then check it.
    fn check(root: &Path, written: &str) -> Result<(), SpellingError> {
        let target = root.join(written).canonicalize().expect("resolves");
        spelled_as_on_disk(root, Path::new(written), &target)
    }

    /// Whether `root` ignores case. Linux CI does not, macOS and Windows do.
    fn case_insensitive(root: &Path) -> bool {
        fs::write(root.join("probe"), "").unwrap();
        root.join("PROBE").exists()
    }

    fn misspelled(result: &Result<(), SpellingError>, stored: &str) -> bool {
        matches!(result, Err(SpellingError::Misspelled { on_disk }) if on_disk == Path::new(stored))
    }

    #[test]
    fn exact_spelling_needs_no_listing() {
        // Nothing exists here, so a listing would fail.
        let root = Path::new("/nonexistent-rw-project");
        let target = root.join("img/logo.png");

        assert!(spelled_as_on_disk(root, Path::new("img/logo.png"), &target).is_ok());
    }

    #[test]
    fn case_alias_is_refused_with_the_stored_spelling() {
        let (_dir, root) = project();
        write(&root, "img/Logo.png");

        assert!(check(&root, "img/Logo.png").is_ok());
        let insensitive = case_insensitive(&root);
        // Windows CI is the evidence that its canonicalize returns the stored
        // case, so it must not quietly take the other branch.
        assert!(insensitive || !cfg!(windows));
        if insensitive {
            let target = root.join("IMG/logo.PNG").canonicalize().unwrap();
            let result = spelled_as_on_disk(&root, Path::new("IMG/logo.PNG"), &target);
            assert!(
                misspelled(&result, "img/Logo.png"),
                "{result:?}, canonicalize returned {}",
                target.display()
            );
        } else {
            assert!(!root.join("IMG/logo.PNG").exists());
        }
    }

    #[test]
    fn a_case_alias_is_refused_on_any_filesystem() {
        let (_dir, root) = project();
        write(&root, "img/logo.png");
        let target = root.join("img/logo.png");

        let result = spelled_as_on_disk(&root, Path::new("IMG/logo.png"), &target);
        assert!(misspelled(&result, "img/logo.png"), "{result:?}");
    }

    #[test]
    fn parent_steps_are_left_to_the_filesystem() {
        let (_dir, root) = project();
        write(&root, "img/logo.png");
        let up = Path::new("..").join(root.file_name().unwrap());

        assert!(check(&root, "img/../img/logo.png").is_ok());
        assert!(check(&root, up.join("img/logo.png").to_str().unwrap()).is_ok());
        let target = root.join("img/logo.png");
        let result = spelled_as_on_disk(&root, Path::new("img/../IMG/logo.png"), &target);
        assert!(misspelled(&result, "img/logo.png"), "{result:?}");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_spelled_by_their_own_names() {
        use std::os::unix::fs::symlink;

        let (_dir, root) = project();
        write(&root, "real/a.md");
        symlink("real", root.join("linkdir")).unwrap();
        symlink("real/a.md", root.join("link.md")).unwrap();

        write(&root, "up/x.md");
        fs::create_dir(root.join("up/down")).unwrap();
        symlink("up/down", root.join("down")).unwrap();

        assert!(check(&root, "linkdir/a.md").is_ok());
        assert!(check(&root, "link.md").is_ok());
        // `..` leaves the link's target, not the link.
        assert!(check(&root, "down/../x.md").is_ok());
        if case_insensitive(&root) {
            assert!(misspelled(&check(&root, "LinkDir/a.md"), "linkdir/a.md"));
            assert!(misspelled(&check(&root, "Link.md"), "link.md"));
            assert!(misspelled(&check(&root, "linkdir/A.md"), "linkdir/a.md"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_misspelled_link_out_of_its_folder_keeps_the_links_name() {
        use std::os::unix::fs::symlink;

        let (_dir, root) = project();
        write(&root, ".github/assets/badge.svg");
        write(&root, "real/a.md");
        fs::create_dir(root.join("docs")).unwrap();
        symlink("../.github/assets", root.join("docs/assets")).unwrap();
        symlink("../real", root.join("docs/one")).unwrap();
        symlink("../real", root.join("docs/two")).unwrap();

        if case_insensitive(&root) {
            assert!(misspelled(
                &check(&root, "docs/Assets/badge.svg"),
                "docs/assets/badge.svg"
            ));
            assert!(misspelled(
                &check(&root, "docs/assets/Badge.svg"),
                "docs/assets/badge.svg"
            ));
            // Two links to one folder: rather the target than the wrong link.
            assert!(misspelled(&check(&root, "docs/One/a.md"), "real/a.md"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn hints_keep_the_written_directories() {
        use std::os::unix::fs::symlink;

        use super::hint;

        let (_dir, root) = project();
        write(&root, "real/a.md");
        write(&root, "real/foo/b.md");
        write(&root, "other.md");
        symlink("real", root.join("linkdir")).unwrap();
        symlink("foo", root.join("real/inlink")).unwrap();
        let linkdir = root.join("linkdir");

        let hint_for = |rest: &str| hint(&root, &linkdir, Path::new(rest));
        assert_eq!(hint_for("a.md"), Some(PathBuf::from("linkdir/a.md")));
        assert_eq!(
            hint_for("inlink/b.md"),
            Some(PathBuf::from("linkdir/inlink/b.md"))
        );
        assert_eq!(hint_for("../other.md"), None);
        assert_eq!(
            hint(&root, &root, Path::new("linkdir/inlink/b.md")),
            Some(PathBuf::from("linkdir/inlink/b.md"))
        );
    }

    #[test]
    fn names_compare_in_nfc() {
        let (_dir, root) = project();
        let nfc = "caf\u{e9}.md";
        let nfd = "cafe\u{301}.md";
        write(&root, nfc);

        // HFS+ stores NFD, so canonicalize answers an NFC spelling with NFD.
        assert!(spelled_as_on_disk(&root, Path::new(nfc), &root.join(nfd)).is_ok());
        // A target that differs forces the listing, which holds the NFC name.
        let elsewhere = root.join("elsewhere.md");
        assert!(spelled_as_on_disk(&root, Path::new(nfd), &elsewhere).is_ok());
    }

    #[test]
    fn a_component_missing_from_its_listing_is_misspelled() {
        let (_dir, root) = project();
        write(&root, "img/logo.png");
        let target = root.join("img/logo.png");

        for written in ["img/other.png", "pics/logo.png"] {
            let result = spelled_as_on_disk(&root, Path::new(written), &target);
            assert!(misspelled(&result, "img/logo.png"), "{written}: {result:?}");
        }
    }

    #[test]
    fn an_unlistable_directory_is_reported() {
        let (_dir, root) = project();
        write(&root, "img/logo.png");
        let target = root.join("img/logo.png");

        let result = spelled_as_on_disk(&root, Path::new("img/logo.png/x"), &target);
        assert!(
            matches!(result, Err(SpellingError::Unlisted(_))),
            "{result:?}"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "not a relative path")]
    fn written_paths_must_be_relative() {
        let root = Path::new("/project");
        let _ = spelled_as_on_disk(root, Path::new("/logo.png"), &root.join("logo.png"));
    }
}
