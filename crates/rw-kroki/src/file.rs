//! Reading the diagram file a request names.

use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};

use rw_diagrams::{DiagramError, FileStamp};

/// The largest diagram file read: storage's limit on a project file
/// (`rw_storage::MAX_FILE_SIZE`), which this crate does not depend on.
const MAX_FILE_SIZE: u64 = 32 * 1024 * 1024;

/// A diagram file's content, and where it was read from.
pub(crate) struct DiagramFileContent {
    pub(crate) text: String,
    /// Canonical.
    pub(crate) path: PathBuf,
    /// The canonical project directory.
    pub(crate) project: PathBuf,
}

/// Read `path`, relative to `project`, refusing anything that resolves outside
/// it.
///
/// `record` gets the file's stamp even when it is missing, so creating it
/// later is noticed.
pub(crate) fn read_diagram_file(
    project: Option<&Path>,
    path: &str,
    record: Option<&(dyn Fn(FileStamp) + Sync)>,
) -> Result<DiagramFileContent, DiagramError> {
    let project = project.ok_or_else(|| {
        failure(format!(
            "diagram file '{path}' needs a project directory to be read from"
        ))
    })?;
    let joined = project.join(path);
    if let Some(record) = record {
        record(FileStamp::of(joined.clone()));
    }
    let project = project.canonicalize().map_err(|error| {
        failure(format!(
            "cannot read diagram file '{path}': project directory '{}': {error}",
            project.display()
        ))
    })?;
    let canonical = joined
        .canonicalize()
        .map_err(|error| unreadable(path, &error))?;
    if !canonical.starts_with(&project) {
        return Err(failure(format!(
            "diagram file '{path}' is outside the project"
        )));
    }

    // Opened through the path confinement checked, and bounded, since the
    // file can be swapped or grow after that check.
    let file = File::open(&canonical).map_err(|error| unreadable(path, &error))?;
    let metadata = file.metadata().map_err(|error| unreadable(path, &error))?;
    if !metadata.is_file() {
        return Err(failure(format!("diagram file '{path}' is not a file")));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| unreadable(path, &error))?;
    if bytes.len() as u64 > MAX_FILE_SIZE {
        return Err(failure(format!(
            "diagram file '{path}' is larger than {} MiB",
            MAX_FILE_SIZE / 1024 / 1024
        )));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| failure(format!("diagram file '{path}' is not valid UTF-8")))?;

    Ok(DiagramFileContent {
        text,
        path: canonical,
        project,
    })
}

fn unreadable(path: &str, error: &io::Error) -> DiagramError {
    if error.kind() == ErrorKind::NotFound {
        failure(format!("diagram file '{path}' not found"))
    } else {
        failure(format!("cannot read diagram file '{path}': {error}"))
    }
}

/// Retrying reads the same file the same way; a change to it is caught by its
/// stamp instead.
fn failure(message: String) -> DiagramError {
    DiagramError {
        message,
        transient: false,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use parking_lot::Mutex;
    use rw_diagrams::FileStamp;

    use super::{MAX_FILE_SIZE, read_diagram_file};

    /// A temporary directory holding `project/`, so a test can put files
    /// just outside it.
    fn layout() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("project/docs")).unwrap();
        root
    }

    fn error_of(project: &Path, path: &str) -> String {
        match read_diagram_file(Some(project), path, None) {
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

        let read = read_diagram_file(Some(&project), "docs/flow.mmd", None).expect("read");

        assert_eq!(read.text, "graph TD");
        assert_eq!(read.project, project.canonicalize().unwrap());
        assert_eq!(read.path, read.project.join("docs/flow.mmd"));
    }

    #[test]
    fn a_missing_file_fails_and_is_stamped_as_missing() {
        let root = layout();
        let project = root.path().join("project");
        let reads = Mutex::new(Vec::new());

        let error = read_diagram_file(
            Some(&project),
            "docs/flow.puml",
            Some(&|stamp| reads.lock().push(stamp)),
        )
        .err()
        .expect("missing");

        assert_eq!(error.message, "diagram file 'docs/flow.puml' not found");
        assert!(!error.transient);
        let stamp = FileStamp {
            path: project.join("docs/flow.puml"),
            modified: None,
        };
        assert_eq!(reads.into_inner(), [stamp]);
    }

    #[test]
    fn without_a_project_directory_nothing_is_read() {
        let error = read_diagram_file(None, "docs/flow.puml", None)
            .err()
            .expect("no project");
        assert_eq!(
            error.message,
            "diagram file 'docs/flow.puml' needs a project directory to be read from"
        );
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

    /// The project directory itself may be a symlink; confinement is to its
    /// target.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_project_directory_still_reads() {
        let root = layout();
        fs::write(root.path().join("project/docs/flow.mmd"), "graph TD").unwrap();
        std::os::unix::fs::symlink(root.path().join("project"), root.path().join("linked"))
            .unwrap();

        let read = read_diagram_file(Some(&root.path().join("linked")), "docs/flow.mmd", None)
            .expect("read");
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
}
