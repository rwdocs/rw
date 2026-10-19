//! Files a provider read beyond the request, for callers that cache output.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

/// A path a provider looked at, and its modification time when it looked.
///
/// `modified` is `None` when nothing could be stat'ed there, which is how a
/// lookup that missed is recorded: creating the file later changes the stamp.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileStamp {
    /// The path as probed.
    pub path: PathBuf,
    /// Modification time at probe time, or `None` when the path did not stat.
    pub modified: Option<SystemTime>,
}

impl FileStamp {
    /// Stamp `path` as it is now.
    #[must_use]
    pub fn of(path: PathBuf) -> Self {
        let modified = fs::metadata(&path).and_then(|m| m.modified()).ok();
        Self { path, modified }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::FileStamp;

    #[test]
    fn a_missing_path_stamps_as_none_and_a_present_one_as_its_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("common.iuml");
        assert_eq!(FileStamp::of(path.clone()).modified, None);

        fs::write(&path, "A -> B").unwrap();
        assert_eq!(
            FileStamp::of(path.clone()).modified,
            Some(fs::metadata(&path).unwrap().modified().unwrap()),
        );
    }
}
