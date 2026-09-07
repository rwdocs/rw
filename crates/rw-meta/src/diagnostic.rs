//! Recoverable problems found while parsing metadata sources, attached to a
//! single field or the whole source.

use std::fmt;

/// Which metadata source produced a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSource {
    /// YAML frontmatter block at the top of a markdown file.
    Frontmatter,
    /// A sidecar file (`meta.yaml` or a named `<name>.meta.yaml`).
    Sidecar,
}

impl fmt::Display for DiagnosticSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Frontmatter => "frontmatter",
            Self::Sidecar => "sidecar",
        })
    }
}

/// How much metadata a problem cost. Both severities are non-fatal: the page
/// always resolves, with the offending data dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// One field of one source was dropped; sibling fields survive.
    Warning,
    /// The whole source was unreadable; every field it declared is dropped.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

/// One recoverable metadata problem, attached to the source and field that
/// failed. Diagnostics describe dropped data; they are never metadata
/// themselves and never inherit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Source that produced the problem.
    pub source: DiagnosticSource,
    /// Field key the problem is attached to; `None` for whole-source problems.
    pub field: Option<String>,
    /// How much data the problem cost.
    pub severity: Severity,
    /// Human-readable description, rendered to logs. Not a stable API.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.field {
            Some(field) => write!(f, "{} `{}`: {}", self.source, field, self.message),
            None => write!(f, "{}: {}", self.source, self.message),
        }
    }
}
