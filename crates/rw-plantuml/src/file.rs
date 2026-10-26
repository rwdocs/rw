//! Bounded reads of diagram files and the includes they pull in.

use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};

use rw_diagrams::{DiagramError, FileStamp, diagram_language};
use rw_path_spelling::{SpellingError, spelled_as_on_disk};

use crate::includes::DiagramFile;

/// The largest diagram or include file read: storage's limit on a project
/// file (`rw_storage::MAX_FILE_SIZE`), which this crate does not depend on
/// (`rw-site` asserts they match).
pub const MAX_FILE_SIZE: u64 = 32 * 1024 * 1024;

/// Why a file could not be read.
#[derive(Debug)]
pub(crate) enum ReadError {
    NotFound,
    /// Found only because the filesystem ignores case; `on_disk` is the
    /// stored spelling, relative to the same directory as the written one.
    Misspelled {
        on_disk: String,
    },
    NotAFile,
    TooLarge,
    NotUtf8,
    /// Anything else the filesystem reported, such as a permission error.
    Io(io::Error),
}

impl ReadError {
    /// Whether reading again could succeed with the file unchanged. A
    /// `chmod`, or a rename to the written spelling, leaves the mtime, and
    /// so the file's stamp, as it was.
    fn is_transient(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Misspelled { .. })
    }
}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        // A path through a file stays unreadable until the path changes.
        if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) {
            Self::NotFound
        } else {
            Self::Io(error)
        }
    }
}

/// Completes a sentence that starts with the file's name.
impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::Misspelled { on_disk } => write!(f, "not found; the file on disk is '{on_disk}'"),
            Self::NotAFile => f.write_str("is not a file"),
            Self::TooLarge => write!(f, "is larger than {} MiB", MAX_FILE_SIZE / 1024 / 1024),
            Self::NotUtf8 => f.write_str("is not valid UTF-8"),
            Self::Io(error) => write!(f, "cannot be read ({error})"),
        }
    }
}

/// Checks that `written`, relative to the canonical `root`, spells each name
/// of `canonical` as stored.
pub(crate) fn check_spelling(
    root: &Path,
    written: &Path,
    canonical: &Path,
) -> Result<(), ReadError> {
    spelled_as_on_disk(root, written, canonical).map_err(|error| match error {
        SpellingError::Misspelled { on_disk } => ReadError::Misspelled {
            // `\` would work on Windows only, and the hint is to be pasted.
            on_disk: on_disk
                .iter()
                .map(OsStr::to_string_lossy)
                .collect::<Vec<_>>()
                .join("/"),
        },
        SpellingError::Unlisted(error) => ReadError::Io(error),
    })
}

/// Read a regular file of at most [`MAX_FILE_SIZE`] bytes as UTF-8.
pub(crate) fn read_bounded(path: &Path) -> Result<String, ReadError> {
    // Opening a FIFO blocks until something writes to it.
    if !fs::metadata(path)?.is_file() {
        return Err(ReadError::NotAFile);
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(ReadError::NotAFile);
    }
    // `take` bounds a file that grows after opening, and hides its length from `read_to_end`.
    let len = usize::try_from(metadata.len().min(MAX_FILE_SIZE)).unwrap_or_default();
    let mut bytes = Vec::with_capacity(len);
    file.take(MAX_FILE_SIZE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_SIZE {
        return Err(ReadError::TooLarge);
    }
    String::from_utf8(bytes).map_err(|_| ReadError::NotUtf8)
}

/// A diagram file's content, and where it was read from.
#[derive(Debug)]
pub struct DiagramFileContent {
    pub text: String,
    /// Canonical.
    pub path: PathBuf,
    /// The canonical project directory.
    pub project: PathBuf,
}

impl DiagramFileContent {
    /// Where the content came from, for resolving its includes.
    #[must_use]
    pub fn file(&self) -> DiagramFile<'_> {
        DiagramFile {
            path: &self.path,
            project: &self.project,
        }
    }
}

/// Read `path`, relative to `project`, refusing anything that resolves outside
/// it or to a file without a diagram extension.
///
/// `record` gets the file's stamp even when it is missing, so creating it
/// later is noticed.
///
/// # Errors
///
/// A [`DiagramError`] naming `path`. It is transient only when the
/// filesystem refused for a reason other than what the file holds or where
/// it leads, or when `path` is misspelled.
pub fn read_diagram_file(
    project: &Path,
    path: &str,
    record: Option<&(dyn Fn(FileStamp) + Sync)>,
) -> Result<DiagramFileContent, DiagramError> {
    let joined = project.join(path);
    // `None` for an absolute path that does not start with `project`: there
    // is no written spelling under it to check.
    let written = joined.strip_prefix(project).ok();
    if let Some(record) = record {
        record(FileStamp::of(joined.clone()));
    }
    let failure = |error: ReadError| DiagramError {
        transient: error.is_transient(),
        message: format!("diagram file '{path}' {error}"),
    };
    let project = project.canonicalize().map_err(|error| DiagramError {
        transient: error.kind() != ErrorKind::NotFound,
        message: format!(
            "diagram file '{path}' cannot be read: project directory '{}': {error}",
            project.display()
        ),
    })?;
    let canonical = joined
        .canonicalize()
        .map_err(|error| failure(error.into()))?;
    if !canonical.starts_with(&project) {
        return Err(DiagramError {
            message: format!("diagram file '{path}' is outside the project"),
            transient: false,
        });
    }
    if let Some(written) = written {
        check_spelling(&project, written, &canonical).map_err(failure)?;
    }
    // The text goes to Kroki, so a symlinked `.puml` must not reach `.env`.
    if canonical.to_str().and_then(diagram_language).is_none() {
        return Err(DiagramError {
            message: format!("diagram file '{path}' is not a diagram file"),
            transient: false,
        });
    }
    let text = read_bounded(&canonical).map_err(failure)?;
    Ok(DiagramFileContent {
        text,
        path: canonical,
        project,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use parking_lot::Mutex;
    use rw_diagrams::FileStamp;

    use super::{MAX_FILE_SIZE, check_spelling, read_diagram_file};

    /// A temporary directory holding `project/`, so a test can put files
    /// just outside it.
    fn layout() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("project/docs")).unwrap();
        root
    }

    fn error_of(project: &Path, path: &str) -> String {
        match read_diagram_file(project, path, None) {
            Ok(read) => panic!("'{path}' should fail, read {:?}", read.text),
            Err(error) => {
                assert!(!error.transient, "{}", error.message);
                error.message
            }
        }
    }

    #[test]
    fn reads_a_file_relative_to_the_project() {
        let root = layout();
        let project = root.path().join("project");
        fs::write(project.join("docs/flow.mmd"), "graph TD").unwrap();

        let read = read_diagram_file(&project, "docs/flow.mmd", None).expect("read");

        assert_eq!(read.text, "graph TD");
        assert_eq!(read.project, project.canonicalize().unwrap());
        assert_eq!(read.path, read.project.join("docs/flow.mmd"));
        let read = read_diagram_file(&project, "docs/../docs/flow.mmd", None).expect("read");
        assert_eq!(read.path, read.project.join("docs/flow.mmd"));
    }

    /// Whether `dir` ignores case. Linux CI does not, macOS and Windows do.
    fn case_insensitive(dir: &Path) -> bool {
        fs::write(dir.join("probe"), "").unwrap();
        dir.join("PROBE").exists()
    }

    /// So a preview on a Mac fails where the Linux publish would.
    #[test]
    fn a_wrong_case_path_is_refused_with_the_spelling_on_disk() {
        let root = layout();
        let project = root.path().join("project");
        fs::write(project.join("docs/flow.puml"), "A -> B").unwrap();

        let insensitive = case_insensitive(root.path());

        let error = read_diagram_file(&project, "Docs/Flow.puml", None).expect_err("misspelled");
        let hint = if insensitive {
            "; the file on disk is 'docs/flow.puml'"
        } else {
            ""
        };
        assert_eq!(
            error.message,
            format!("diagram file 'Docs/Flow.puml' not found{hint}")
        );
        // Renaming the file to match leaves its stamp as it was.
        assert_eq!(error.transient, insensitive);
    }

    #[test]
    fn a_case_alias_is_refused_on_any_filesystem() {
        let root = layout();
        let project = root.path().join("project").canonicalize().unwrap();
        fs::write(project.join("docs/flow.puml"), "A -> B").unwrap();

        let error = check_spelling(
            &project,
            Path::new("docs/Flow.puml"),
            &project.join("docs/flow.puml"),
        )
        .expect_err("misspelled");

        assert!(error.is_transient());
        assert_eq!(
            error.to_string(),
            "not found; the file on disk is 'docs/flow.puml'"
        );
    }

    #[test]
    fn a_missing_file_fails_and_is_stamped_as_missing() {
        let root = layout();
        let project = root.path().join("project");
        let reads = Mutex::new(Vec::new());

        let error = read_diagram_file(
            &project,
            "docs/flow.puml",
            Some(&|stamp| reads.lock().push(stamp)),
        )
        .expect_err("missing");

        assert_eq!(error.message, "diagram file 'docs/flow.puml' not found");
        assert!(!error.transient);
        let stamp = FileStamp {
            path: project.join("docs/flow.puml"),
            modified: None,
        };
        assert_eq!(reads.into_inner(), [stamp]);
    }

    #[test]
    fn a_path_leaving_the_project_is_refused() {
        let root = layout();
        fs::write(root.path().join("outside.puml"), "Secret").unwrap();
        let project = root.path().join("project");

        assert_eq!(
            error_of(&project, "docs/../../outside.puml"),
            "diagram file 'docs/../../outside.puml' is outside the project"
        );
        let absolute = root.path().join("outside.puml").display().to_string();
        assert_eq!(
            error_of(&project, &absolute),
            format!("diagram file '{absolute}' is outside the project")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_leading_out_of_the_project_is_refused() {
        let root = layout();
        fs::write(root.path().join("outside.puml"), "Secret").unwrap();
        let project = root.path().join("project");
        std::os::unix::fs::symlink(
            root.path().join("outside.puml"),
            project.join("docs/flow.puml"),
        )
        .unwrap();

        assert_eq!(
            error_of(&project, "docs/flow.puml"),
            "diagram file 'docs/flow.puml' is outside the project"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_file_that_is_no_diagram_is_refused() {
        let root = layout();
        let project = root.path().join("project");
        fs::write(project.join(".env"), "SECRET=1").unwrap();
        std::os::unix::fs::symlink("../.env", project.join("docs/leak.puml")).unwrap();

        assert_eq!(
            error_of(&project, "docs/leak.puml"),
            "diagram file 'docs/leak.puml' is not a diagram file"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_another_diagram_file_reads_it() {
        let root = layout();
        let project = root.path().join("project");
        fs::write(project.join("shared.mmd"), "graph TD").unwrap();
        std::os::unix::fs::symlink("../shared.mmd", project.join("docs/flow.puml")).unwrap();

        let read = read_diagram_file(&project, "docs/flow.puml", None).expect("read");

        assert_eq!(read.text, "graph TD");
        assert_eq!(read.path, read.project.join("shared.mmd"));
    }

    /// The project directory itself may be a symlink; confinement is to its
    /// target.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_project_directory_still_reads() {
        let root = layout();
        fs::write(root.path().join("project/docs/flow.mmd"), "graph TD").unwrap();
        std::os::unix::fs::symlink(root.path().join("project"), root.path().join("linked"))
            .unwrap();

        let read =
            read_diagram_file(&root.path().join("linked"), "docs/flow.mmd", None).expect("read");
        assert_eq!(read.text, "graph TD");
    }

    #[test]
    fn a_file_over_the_size_limit_is_refused() {
        let root = layout();
        let project = root.path().join("project");
        fs::File::create(project.join("docs/huge.puml"))
            .unwrap()
            .set_len(MAX_FILE_SIZE + 1)
            .unwrap();

        assert_eq!(
            error_of(&project, "docs/huge.puml"),
            "diagram file 'docs/huge.puml' is larger than 32 MiB"
        );
    }

    #[test]
    fn a_file_that_is_not_utf8_is_refused() {
        let root = layout();
        let project = root.path().join("project");
        fs::write(project.join("docs/flow.puml"), [0xFF, 0xFE, 0x00]).unwrap();

        assert_eq!(
            error_of(&project, "docs/flow.puml"),
            "diagram file 'docs/flow.puml' is not valid UTF-8"
        );
    }

    #[test]
    fn a_directory_is_refused() {
        let root = layout();
        let project = root.path().join("project");
        fs::create_dir(project.join("docs/dir.puml")).unwrap();

        assert_eq!(
            error_of(&project, "docs/dir.puml"),
            "diagram file 'docs/dir.puml' is not a file"
        );
    }

    /// Stamped like any miss, so it must not be retried on every request.
    #[test]
    fn a_path_through_a_file_is_not_found() {
        let root = layout();
        let project = root.path().join("project");
        fs::write(project.join("docs/secret.txt"), "Secret").unwrap();

        assert_eq!(
            error_of(&project, "docs/secret.txt/x.puml"),
            "diagram file 'docs/secret.txt/x.puml' not found"
        );
    }

    /// Opening one would block until something wrote to it.
    #[cfg(unix)]
    #[test]
    fn a_fifo_is_refused_without_blocking() {
        let root = layout();
        let project = root.path().join("project");
        let fifo = project.join("docs/flow.puml");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("run mkfifo");
        assert!(made.success());

        assert_eq!(
            error_of(&project, "docs/flow.puml"),
            "diagram file 'docs/flow.puml' is not a file"
        );
    }

    /// A permission error can clear without the file's stamp changing, so it
    /// must not be cached as the file's answer.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_file_fails_transiently() {
        use std::os::unix::fs::PermissionsExt;

        let root = layout();
        let project = root.path().join("project");
        let file = project.join("docs/flow.puml");
        fs::write(&file, "A -> B").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&file).is_ok() {
            // Running as root: permissions do not apply.
            return;
        }

        let error = read_diagram_file(&project, "docs/flow.puml", None).expect_err("unreadable");

        assert!(error.transient, "{}", error.message);
        assert!(
            error
                .message
                .starts_with("diagram file 'docs/flow.puml' cannot be read ("),
            "{}",
            error.message,
        );
    }
}
