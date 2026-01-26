//! Directive processing context.
//!
//! Provides file system access and source location information to directive handlers.

use std::io;
use std::path::{Path, PathBuf};

/// Context provided to directive handlers for file system access and source location.
///
/// The context is created by [`DirectiveProcessor`](super::DirectiveProcessor) for each
/// directive and provides:
///
/// - Source file information for error messages
/// - Base directory for resolving relative paths
/// - File reading callback for `::include` and similar directives
///
/// # Example
///
/// ```
/// use std::path::Path;
/// use rw_renderer::directive::DirectiveContext;
///
/// let ctx = DirectiveContext {
///     source_path: Some(Path::new("docs/guide.md")),
///     base_dir: Path::new("docs"),
///     line: 42,
///     read_file: &|path| std::fs::read_to_string(path),
/// };
///
/// let resolved = ctx.resolve_path("snippets/example.md");
/// assert_eq!(resolved, Path::new("docs/snippets/example.md"));
/// ```
pub struct DirectiveContext<'a> {
    /// Path to the source file being rendered (if known).
    pub source_path: Option<&'a Path>,
    /// Base directory for resolving relative paths.
    pub base_dir: &'a Path,
    /// Line number where the directive appears (1-indexed).
    pub line: usize,
    /// Callback to read a file from the file system.
    pub read_file: &'a dyn Fn(&Path) -> io::Result<String>,
}

impl<'a> DirectiveContext<'a> {
    /// Resolve a relative path against the base directory.
    ///
    /// Returns the joined path. Use [`resolve_path_safe`](Self::resolve_path_safe)
    /// to validate that the resolved path stays within the base directory.
    ///
    /// # Example
    ///
    /// ```
    /// use std::path::Path;
    /// use rw_renderer::directive::DirectiveContext;
    ///
    /// let ctx = DirectiveContext {
    ///     source_path: None,
    ///     base_dir: Path::new("/docs"),
    ///     line: 1,
    ///     read_file: &|_| Ok(String::new()),
    /// };
    ///
    /// assert_eq!(ctx.resolve_path("guide.md"), Path::new("/docs/guide.md"));
    /// ```
    #[must_use]
    pub fn resolve_path(&self, relative: &str) -> PathBuf {
        self.base_dir.join(relative)
    }

    /// Resolve a relative path with path traversal protection.
    ///
    /// Returns `Some(path)` if the resolved path stays within the base directory,
    /// or `None` if the path attempts to escape (e.g., `../../etc/passwd`).
    ///
    /// This method canonicalizes both paths and validates that the resolved path
    /// starts with the base directory. Returns `None` if the file doesn't exist
    /// (since canonicalization requires the path to exist).
    ///
    /// # Example
    ///
    /// ```
    /// use std::path::Path;
    /// use rw_renderer::directive::DirectiveContext;
    ///
    /// let ctx = DirectiveContext {
    ///     source_path: None,
    ///     base_dir: Path::new("."),
    ///     line: 1,
    ///     read_file: &|_| Ok(String::new()),
    /// };
    ///
    /// // Path traversal attempt returns None
    /// assert!(ctx.resolve_path_safe("../../etc/passwd").is_none());
    ///
    /// // Non-existent file also returns None (can't canonicalize)
    /// assert!(ctx.resolve_path_safe("nonexistent.md").is_none());
    ///
    /// // Existing file within base_dir returns Some(canonical_path)
    /// // (requires actual file to exist for canonicalize to work)
    /// ```
    #[must_use]
    pub fn resolve_path_safe(&self, relative: &str) -> Option<PathBuf> {
        let resolved = self.base_dir.join(relative);

        // Canonicalize and validate path stays within base_dir
        let canonical = resolved.canonicalize().ok()?;
        let canonical_base = self.base_dir.canonicalize().ok()?;

        if canonical.starts_with(&canonical_base) {
            Some(canonical)
        } else {
            None // Path traversal attempt
        }
    }

    /// Read a file using the context's read_file callback.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn read(&self, path: &Path) -> io::Result<String> {
        (self.read_file)(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_path() {
        let ctx = DirectiveContext {
            source_path: Some(Path::new("docs/guide.md")),
            base_dir: Path::new("docs"),
            line: 10,
            read_file: &|_| Ok(String::new()),
        };

        assert_eq!(
            ctx.resolve_path("snippets/code.md"),
            PathBuf::from("docs/snippets/code.md")
        );
    }

    #[test]
    fn test_resolve_absolute_path() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/home/user/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };

        // Joining absolute path replaces the base
        assert_eq!(
            ctx.resolve_path("/etc/config"),
            PathBuf::from("/etc/config")
        );
    }

    #[test]
    fn test_read_file() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("."),
            line: 1,
            read_file: &|_| Ok("file content".to_string()),
        };

        let result = ctx.read(Path::new("test.md"));
        assert_eq!(result.unwrap(), "file content");
    }

    #[test]
    fn test_read_file_error() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("."),
            line: 1,
            read_file: &|_| Err(io::Error::new(io::ErrorKind::NotFound, "not found")),
        };

        let result = ctx.read(Path::new("nonexistent.md"));
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_path_safe_within_base() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();
        std::fs::write(base_dir.join("guide.md"), "# Guide").unwrap();

        let ctx = DirectiveContext {
            source_path: None,
            base_dir,
            line: 1,
            read_file: &|_| Ok(String::new()),
        };

        let result = ctx.resolve_path_safe("guide.md");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("guide.md"));
    }

    #[test]
    fn test_resolve_path_safe_blocks_traversal() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path().join("docs");
        std::fs::create_dir(&base_dir).unwrap();

        // Create a file outside base_dir
        std::fs::write(temp_dir.path().join("secret.txt"), "secret").unwrap();

        let ctx = DirectiveContext {
            source_path: None,
            base_dir: &base_dir,
            line: 1,
            read_file: &|_| Ok(String::new()),
        };

        // Path traversal attempt should return None
        let result = ctx.resolve_path_safe("../secret.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_path_safe_nonexistent_returns_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_dir = temp_dir.path();

        let ctx = DirectiveContext {
            source_path: None,
            base_dir,
            line: 1,
            read_file: &|_| Ok(String::new()),
        };

        // Non-existent file cannot be canonicalized
        let result = ctx.resolve_path_safe("nonexistent.md");
        assert!(result.is_none());
    }
}
