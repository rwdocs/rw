//! Cache version validation.
//!
//! Ensures the cache directory matches the current build version.
//! When the version changes, all cached data is wiped and rebuilt.

use std::fs;
use std::path::Path;

/// Validate that the cache directory matches the expected version.
///
/// Reads `{cache_dir}/VERSION` and compares it to `version`. If they match,
/// the cache is kept intact. Otherwise, the entire cache directory is wiped
/// and recreated with a new `VERSION` file.
///
/// If the cache directory doesn't exist, it is created with a `VERSION` file.
///
/// Errors are logged but never fatal — cache is optional.
#[allow(dead_code)] // Called from Site::new() in a follow-up change
pub(crate) fn validate_cache_version(cache_dir: &Path, version: &str) {
    let version_path = cache_dir.join("VERSION");

    // If cache dir exists, check the version file
    if cache_dir.exists() {
        if let Ok(contents) = fs::read_to_string(&version_path)
            && contents == version
        {
            return;
        }

        // Version mismatch or missing VERSION file — wipe everything
        if let Err(e) = fs::remove_dir_all(cache_dir) {
            eprintln!("Warning: Failed to remove cache directory: {e}");
            return;
        }
    }

    // Create cache dir and write VERSION
    if let Err(e) = fs::create_dir_all(cache_dir) {
        eprintln!("Warning: Failed to create cache directory: {e}");
        return;
    }

    if let Err(e) = fs::write(&version_path, version) {
        eprintln!("Warning: Failed to write cache VERSION file: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matching_version_keeps_cache() {
        let temp = tempfile::tempdir().unwrap();
        let cache_dir = temp.path().join("cache");

        // Set up cache with matching version and an extra file
        fs::create_dir_all(&cache_dir).unwrap();
        fs::write(cache_dir.join("VERSION"), "1.0.0").unwrap();
        fs::write(cache_dir.join("data.json"), "cached data").unwrap();

        validate_cache_version(&cache_dir, "1.0.0");

        // Both files should still exist
        assert_eq!(fs::read_to_string(cache_dir.join("VERSION")).unwrap(), "1.0.0");
        assert_eq!(fs::read_to_string(cache_dir.join("data.json")).unwrap(), "cached data");
    }

    #[test]
    fn test_mismatched_version_wipes_cache() {
        let temp = tempfile::tempdir().unwrap();
        let cache_dir = temp.path().join("cache");

        // Set up cache with old version, a file, and a subdirectory
        fs::create_dir_all(cache_dir.join("subdir")).unwrap();
        fs::write(cache_dir.join("VERSION"), "0.9.0").unwrap();
        fs::write(cache_dir.join("old_data.json"), "stale").unwrap();
        fs::write(cache_dir.join("subdir/nested.txt"), "nested stale").unwrap();

        validate_cache_version(&cache_dir, "1.0.0");

        // Old files and subdirectory should be gone
        assert!(!cache_dir.join("old_data.json").exists());
        assert!(!cache_dir.join("subdir").exists());

        // VERSION should be updated
        assert_eq!(fs::read_to_string(cache_dir.join("VERSION")).unwrap(), "1.0.0");
    }

    #[test]
    fn test_missing_version_file_wipes_cache() {
        let temp = tempfile::tempdir().unwrap();
        let cache_dir = temp.path().join("cache");

        // Set up cache dir without a VERSION file
        fs::create_dir_all(&cache_dir).unwrap();
        fs::write(cache_dir.join("orphan.txt"), "no version").unwrap();

        validate_cache_version(&cache_dir, "1.0.0");

        // Orphan file should be gone
        assert!(!cache_dir.join("orphan.txt").exists());

        // VERSION should be written
        assert_eq!(fs::read_to_string(cache_dir.join("VERSION")).unwrap(), "1.0.0");
    }

    #[test]
    fn test_nonexistent_cache_dir_creates_version() {
        let temp = tempfile::tempdir().unwrap();
        let cache_dir = temp.path().join("nonexistent_cache");

        assert!(!cache_dir.exists());

        validate_cache_version(&cache_dir, "1.0.0");

        assert!(cache_dir.exists());
        assert_eq!(fs::read_to_string(cache_dir.join("VERSION")).unwrap(), "1.0.0");
    }
}
