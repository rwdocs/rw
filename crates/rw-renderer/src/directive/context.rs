//! Directive processing context.
//!
//! Provides file system access and source location information to directive handlers.

use std::io;
use std::path::{Component, Path, PathBuf};

/// Reason a relative path could not be safely resolved against the base directory.
///
/// Returned by [`DirectiveContext::resolve_path`].
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolveError {
    /// The input is an absolute path.
    ///
    /// Includes Unix absolute paths (`/etc/passwd`), Windows drive-absolute paths
    /// (`C:\Windows`), UNC paths (`\\server\share`), and current-drive-rooted
    /// paths (`\foo`).
    #[error("path is absolute; only paths relative to the base directory are allowed")]
    Absolute,

    /// The input uses a Windows-specific prefix that is not a plain relative path.
    ///
    /// Covers drive-relative paths (`C:foo`), bare drive letters (`C:`), and the
    /// verbatim/device namespaces (`\\?\`, `\\.\`). Returned regardless of the host
    /// OS so directive handlers behave identically on Linux CI and Windows developer
    /// machines.
    #[error("path uses a Windows-specific prefix that is not allowed")]
    WindowsPrefix,

    /// The input would resolve to a location outside the base directory.
    ///
    /// Returned when lexical normalization of the path would pop above
    /// `base_dir` (e.g. `../sibling.md`, `a/../../b`).
    #[error("path escapes the base directory")]
    Traversal,

    /// The input contains a NUL or other control byte (< 0x20).
    #[error("path contains an invalid character")]
    InvalidChar,
}

/// Context provided to directive handlers for file system access and source location.
///
/// The context is created by [`DirectiveProcessor`](super::DirectiveProcessor) for each
/// directive and provides:
///
/// - Source file information for error messages
/// - Base directory for resolving relative paths
/// - File reading callback for `::include` and similar directives
///
/// # Methods
///
/// Use the getter methods [`source_path()`](Self::source_path),
/// [`base_dir()`](Self::base_dir), and [`line()`](Self::line) to access
/// context information. Use [`read()`](Self::read) to read files and
/// [`resolve_path()`](Self::resolve_path) to safely resolve relative paths.
pub struct DirectiveContext<'a> {
    /// Path to the source file being rendered (if known).
    pub(crate) source_path: Option<&'a Path>,
    /// Base directory for resolving relative paths.
    pub(crate) base_dir: &'a Path,
    /// Line number where the directive appears (1-indexed).
    pub(crate) line: usize,
    /// Callback to read a file from the file system.
    pub(crate) read_file: &'a dyn Fn(&Path) -> io::Result<String>,
}

impl DirectiveContext<'_> {
    /// Path to the source file being rendered (if known).
    #[must_use]
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path
    }

    /// Base directory for resolving relative paths.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        self.base_dir
    }

    /// Line number where the directive appears (1-indexed).
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// Resolve a path relative to the base directory.
    ///
    /// Performs lexical normalization only — the filesystem is not consulted,
    /// symlinks are not resolved, and the target file does not need to exist.
    /// Backslashes inside the input are treated as path separators (after the
    /// Windows-prefix checks below) so the same input behaves identically on
    /// Linux and Windows.
    ///
    /// Empty input and `"."` both resolve to `base_dir` itself, which is
    /// usually a typo on the caller's side — handlers that want to reject
    /// these should check `args.content().trim().is_empty()` before calling
    /// `resolve_path`.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] if the input is absolute, uses a Windows-specific
    /// prefix, contains an invalid character, or would escape the base directory.
    ///
    /// # Security
    ///
    /// This is a lexical sandbox, not a filesystem sandbox. In particular:
    ///
    /// - **Symlinks are not resolved.** A symlink placed inside `base_dir` that
    ///   points outside it will be silently followed by [`read`](Self::read).
    ///   Callers that need symlink containment must add their own check (e.g.
    ///   `std::fs::canonicalize` + `starts_with`) after resolving.
    /// - **`base_dir` is trusted as-is.** If the caller constructs the context
    ///   with a `base_dir` that itself contains `..` segments, is un-normalized,
    ///   or is relative, the resolved path inherits that shape (a relative
    ///   `base_dir` produces a relative result, which is then sensitive to the
    ///   process's working directory at [`read`](Self::read) time). Pass a
    ///   canonicalized absolute `base_dir` to get the strongest containment.
    /// - **Inputs are not trimmed.** A leading space or other non-control
    ///   whitespace becomes a literal segment of the result (e.g.
    ///   `" /etc/passwd"` resolves to `base_dir/ /etc/passwd`, which is still
    ///   contained but will fail at read time). Handlers that accept
    ///   user-pasted paths should `trim()` themselves before calling.
    pub fn resolve_path(&self, relative: &str) -> Result<PathBuf, ResolveError> {
        // 1. Reject NUL and other control bytes.
        if relative.bytes().any(|b| b < 0x20) {
            return Err(ResolveError::InvalidChar);
        }

        // 2. Reject Windows-specific prefixes regardless of host OS.
        //    The string-level check runs before Path::components because
        //    `Path::new("C:\\foo").components()` on Unix yields a single
        //    Normal component, which would otherwise sneak through.
        let bytes = relative.as_bytes();
        // Verbatim / device namespaces: \\?\, \\.\
        if relative.starts_with(r"\\?\") || relative.starts_with(r"\\.\") {
            return Err(ResolveError::WindowsPrefix);
        }
        // UNC: \\server\share — treat as absolute
        if relative.starts_with(r"\\") {
            return Err(ResolveError::Absolute);
        }
        // Unix absolute or current-drive-rooted (Windows): leading / or \
        if matches!(bytes.first(), Some(b'/' | b'\\')) {
            return Err(ResolveError::Absolute);
        }
        // Drive letter: C: ...
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            match bytes.get(2) {
                Some(b'/' | b'\\') => return Err(ResolveError::Absolute), // C:\foo
                _ => return Err(ResolveError::WindowsPrefix),             // C: or C:foo
            }
        }

        // 3. Normalize backslash separators to forward slashes so the lexical
        //    walk below treats `dir\file.md` the same on Unix and Windows.
        //    All known Windows path-prefix attacks were rejected above, so any
        //    remaining backslash is a separator inside a relative path.
        let normalized = if relative.contains('\\') {
            relative.replace('\\', "/")
        } else {
            relative.to_owned()
        };

        // 4. Lexical normalization. Walk components, maintain a stack,
        //    refuse to pop above the root.
        let mut stack: Vec<&std::ffi::OsStr> = Vec::new();
        for component in Path::new(&normalized).components() {
            match component {
                Component::Normal(s) => stack.push(s),
                Component::CurDir => {}
                Component::ParentDir => {
                    if stack.pop().is_none() {
                        return Err(ResolveError::Traversal);
                    }
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(ResolveError::Absolute);
                }
            }
        }

        // 5. Re-join onto base_dir.
        let mut result = self.base_dir.to_path_buf();
        for segment in stack {
            result.push(segment);
        }
        Ok(result)
    }

    /// Read a file using the context's `read_file` callback.
    ///
    /// # Security
    ///
    /// This method passes `path` straight to the underlying callback and does
    /// **not** validate that the path stays within [`base_dir`](Self::base_dir).
    /// Directive handlers that derive `path` from user-controlled input (e.g.
    /// the content of an `::include` directive) must first sanitize it via
    /// [`resolve_path`](Self::resolve_path); otherwise the callback is reachable
    /// with arbitrary file paths and the sandbox is bypassed.
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
    fn test_read_file() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("."),
            line: 1,
            read_file: &|_| Ok("file content".to_owned()),
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
    fn test_resolve_path_happy_path() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };

        assert_eq!(
            ctx.resolve_path("snippets/code.md"),
            Ok(PathBuf::from("/docs/snippets/code.md"))
        );
    }

    #[test]
    fn test_resolve_error_display_messages() {
        assert_eq!(
            ResolveError::Absolute.to_string(),
            "path is absolute; only paths relative to the base directory are allowed"
        );
        assert_eq!(
            ResolveError::WindowsPrefix.to_string(),
            "path uses a Windows-specific prefix that is not allowed"
        );
        assert_eq!(
            ResolveError::Traversal.to_string(),
            "path escapes the base directory"
        );
        assert_eq!(
            ResolveError::InvalidChar.to_string(),
            "path contains an invalid character"
        );
    }

    #[test]
    fn test_resolve_path_current_dir_segments() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(
            ctx.resolve_path("./snippets/code.md"),
            Ok(PathBuf::from("/docs/snippets/code.md"))
        );
        assert_eq!(
            ctx.resolve_path("a/./b/../c.md"),
            Ok(PathBuf::from("/docs/a/c.md"))
        );
    }

    #[test]
    fn test_resolve_path_internal_parent_dirs_allowed() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        // Steps into subdir then back out, lands in base
        assert_eq!(
            ctx.resolve_path("a/b/../../c.md"),
            Ok(PathBuf::from("/docs/c.md"))
        );
    }

    #[test]
    fn test_resolve_path_traversal_above_root() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(
            ctx.resolve_path("a/b/../../../c.md"),
            Err(ResolveError::Traversal)
        );
        assert_eq!(
            ctx.resolve_path("../etc/passwd"),
            Err(ResolveError::Traversal)
        );
    }

    #[test]
    fn test_resolve_path_unix_absolute() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(ctx.resolve_path("/etc/passwd"), Err(ResolveError::Absolute));
        // Still Absolute even if it would happen to "stay inside"
        let ctx_etc = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/etc"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(
            ctx_etc.resolve_path("/etc/passwd"),
            Err(ResolveError::Absolute)
        );
    }

    #[test]
    fn test_resolve_path_windows_current_drive_rooted() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(ctx.resolve_path(r"\foo"), Err(ResolveError::Absolute));
    }

    #[test]
    fn test_resolve_path_windows_unc() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(
            ctx.resolve_path(r"\\server\share\file"),
            Err(ResolveError::Absolute)
        );
    }

    #[test]
    fn test_resolve_path_windows_verbatim_and_device() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(
            ctx.resolve_path(r"\\?\C:\foo"),
            Err(ResolveError::WindowsPrefix)
        );
        assert_eq!(
            ctx.resolve_path(r"\\?\UNC\server\share"),
            Err(ResolveError::WindowsPrefix)
        );
        assert_eq!(
            ctx.resolve_path(r"\\.\PhysicalDrive0"),
            Err(ResolveError::WindowsPrefix)
        );
    }

    #[test]
    fn test_resolve_path_windows_drive_absolute() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(
            ctx.resolve_path(r"C:\Windows\System32"),
            Err(ResolveError::Absolute)
        );
        assert_eq!(
            ctx.resolve_path("c:/Windows/System32"),
            Err(ResolveError::Absolute)
        );
    }

    #[test]
    fn test_resolve_path_windows_drive_relative() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(ctx.resolve_path("C:foo"), Err(ResolveError::WindowsPrefix));
        assert_eq!(ctx.resolve_path("c:foo"), Err(ResolveError::WindowsPrefix));
        // Bare drive letter
        assert_eq!(ctx.resolve_path("C:"), Err(ResolveError::WindowsPrefix));
    }

    #[test]
    fn test_resolve_path_control_chars() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(ctx.resolve_path("a\0b"), Err(ResolveError::InvalidChar));
        assert_eq!(ctx.resolve_path("a\tb"), Err(ResolveError::InvalidChar));
        assert_eq!(ctx.resolve_path("a\nb"), Err(ResolveError::InvalidChar));
    }

    #[test]
    fn test_resolve_path_normalizes_embedded_backslashes() {
        // Embedded backslashes in a relative path are treated as separators
        // so the same markdown source resolves identically on Unix and Windows.
        // The Windows-prefix attacks (leading \, drive letter, UNC, \\?\, \\.\)
        // were rejected before reaching this point.
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(
            ctx.resolve_path(r"subdir\file.md"),
            Ok(PathBuf::from("/docs/subdir/file.md"))
        );
        // Traversal via backslash is also caught.
        assert_eq!(
            ctx.resolve_path(r"..\secret.md"),
            Err(ResolveError::Traversal)
        );
        // Mixed separators are normalized.
        assert_eq!(
            ctx.resolve_path(r"a/b\c.md"),
            Ok(PathBuf::from("/docs/a/b/c.md"))
        );
    }

    #[test]
    fn test_resolve_path_empty_and_dot() {
        let ctx = DirectiveContext {
            source_path: None,
            base_dir: Path::new("/docs"),
            line: 1,
            read_file: &|_| Ok(String::new()),
        };
        assert_eq!(ctx.resolve_path(""), Ok(PathBuf::from("/docs")));
        assert_eq!(ctx.resolve_path("."), Ok(PathBuf::from("/docs")));
    }
}
