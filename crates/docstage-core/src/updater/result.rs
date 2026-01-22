//! Result types for page update operations.

use docstage_confluence::{Page, UnmatchedComment};

/// Result of a successful page update.
pub struct UpdateResult {
    /// Updated page information.
    pub page: Page,
    /// URL to view the updated page.
    pub url: String,
    /// Total comment count after update.
    pub comment_count: usize,
    /// Comments that could not be preserved.
    pub unmatched_comments: Vec<UnmatchedComment>,
    /// Number of attachments uploaded.
    pub attachments_uploaded: usize,
    /// Warnings from markdown conversion.
    pub warnings: Vec<String>,
}

/// Result of a dry-run operation (no changes made).
pub struct DryRunResult {
    /// Converted HTML with preserved comments.
    pub html: String,
    /// Extracted title (if any).
    pub title: Option<String>,
    /// Current page title.
    pub current_title: String,
    /// Current page version.
    pub current_version: u32,
    /// Comments that would be lost.
    pub unmatched_comments: Vec<UnmatchedComment>,
    /// Number of attachments that would be uploaded.
    pub attachment_count: usize,
    /// Attachment filenames.
    pub attachment_names: Vec<String>,
    /// Warnings from markdown conversion.
    pub warnings: Vec<String>,
}
