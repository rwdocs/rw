//! Page updater implementation.

use std::path::Path;

use tempfile::TempDir;

use crate::client::ConfluenceClient;
use crate::comment_preservation::preserve_comments;
use crate::renderer::PageRenderer;
use crate::types::Page;

use super::UpdateConfig;
use super::error::UpdateError;
use super::result::{DryRunResult, UpdateResult};

/// Handles updating Confluence pages from markdown.
pub struct PageUpdater<'a> {
    client: &'a ConfluenceClient,
    config: UpdateConfig,
}

impl<'a> PageUpdater<'a> {
    /// Create a new page updater.
    #[must_use]
    pub fn new(client: &'a ConfluenceClient, config: UpdateConfig) -> Self {
        Self { client, config }
    }

    /// Update a Confluence page from markdown content.
    ///
    /// This method:
    /// 1. Converts markdown to Confluence storage format
    /// 2. Fetches current page content
    /// 3. Preserves inline comments from current page
    /// 4. Uploads diagram attachments
    /// 5. Updates the page with new content
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `kroki_url` is not configured
    /// - Confluence API calls fail
    /// - IO operations fail (temp directory, file reading)
    pub fn update(
        &self,
        page_id: &str,
        markdown_text: &str,
        message: Option<&str>,
    ) -> Result<UpdateResult, UpdateError> {
        let kroki_url = self.kroki_url()?;

        // Create temp directory for diagram output
        let tmpdir = TempDir::new()?;

        // Convert markdown
        let renderer = self.create_renderer();
        let render_result = renderer.render(markdown_text, Some(kroki_url), Some(tmpdir.path()));

        // Collect diagram attachments
        let attachments = Self::collect_attachments(tmpdir.path())?;

        // Fetch current page
        let current_page = self
            .client
            .get_page(page_id, &["body.storage", "version"])?;

        // Preserve comments
        let old_html = Self::extract_body_html(&current_page);
        let preserve_result = preserve_comments(old_html, &render_result.html);

        // Determine title
        let title = render_result.title.as_ref().unwrap_or(&current_page.title);

        // Upload attachments
        for (filename, data) in &attachments {
            self.client
                .upload_attachment(page_id, filename, data, "image/png", None)?;
        }

        // Update page
        let updated_page = self.client.update_page(
            page_id,
            title,
            &preserve_result.html,
            current_page.version.number,
            message,
        )?;

        // Get page URL and comment count
        let url = self.client.get_page_url(page_id)?;
        let comments = self.client.get_comments(page_id)?;

        Ok(UpdateResult {
            page: updated_page,
            url,
            comment_count: comments.size,
            unmatched_comments: preserve_result.unmatched_comments,
            attachments_uploaded: attachments.len(),
            warnings: render_result.warnings,
        })
    }

    /// Perform a dry-run update (no changes made).
    ///
    /// Returns information about what would change without
    /// actually updating the page or uploading attachments.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `kroki_url` is not configured
    /// - Confluence API calls fail
    /// - IO operations fail (temp directory, file reading)
    pub fn dry_run(&self, page_id: &str, markdown_text: &str) -> Result<DryRunResult, UpdateError> {
        let kroki_url = self.kroki_url()?;
        let tmpdir = TempDir::new()?;

        let renderer = self.create_renderer();
        let render_result = renderer.render(markdown_text, Some(kroki_url), Some(tmpdir.path()));

        let attachments = Self::collect_attachment_names(tmpdir.path())?;

        let current_page = self
            .client
            .get_page(page_id, &["body.storage", "version"])?;

        let old_html = Self::extract_body_html(&current_page);
        let preserve_result = preserve_comments(old_html, &render_result.html);

        Ok(DryRunResult {
            html: preserve_result.html,
            title: render_result.title,
            current_title: current_page.title,
            current_version: current_page.version.number,
            unmatched_comments: preserve_result.unmatched_comments,
            attachment_count: attachments.len(),
            attachment_names: attachments,
            warnings: render_result.warnings,
        })
    }

    fn kroki_url(&self) -> Result<&str, UpdateError> {
        self.config.diagrams.kroki_url.as_deref().ok_or_else(|| {
            UpdateError::Config("kroki_url required (via --kroki-url or [diagrams] config)".into())
        })
    }

    fn create_renderer(&self) -> PageRenderer {
        let diagrams = &self.config.diagrams;
        PageRenderer::new()
            .prepend_toc(true)
            .extract_title(self.config.extract_title)
            .include_dirs(diagrams.include_dirs.clone())
            .dpi(diagrams.dpi)
    }

    fn collect_attachments(dir: &Path) -> Result<Vec<(String, Vec<u8>)>, UpdateError> {
        let mut attachments = Vec::new();
        for (filename, path) in Self::png_files(dir)? {
            let data = std::fs::read(&path)?;
            attachments.push((filename, data));
        }
        attachments.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(attachments)
    }

    fn collect_attachment_names(dir: &Path) -> Result<Vec<String>, UpdateError> {
        let mut names: Vec<_> = Self::png_files(dir)?
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        names.sort();
        Ok(names)
    }

    /// Returns PNG files as (filename, path) pairs.
    fn png_files(dir: &Path) -> Result<Vec<(String, std::path::PathBuf)>, UpdateError> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "png") {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("diagram.png")
                    .to_owned();
                files.push((filename, path));
            }
        }
        Ok(files)
    }

    fn extract_body_html(page: &Page) -> &str {
        page.body
            .as_ref()
            .and_then(|b| b.storage.as_ref())
            .map_or("", |s| s.value.as_str())
    }
}
