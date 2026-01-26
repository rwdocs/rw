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
}
