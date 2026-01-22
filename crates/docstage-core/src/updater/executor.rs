//! Page updater implementation.

use std::path::Path;

use docstage_confluence::{ConfluenceClient, Page, preserve_comments};
use tempfile::TempDir;

use crate::MarkdownConverter;

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
        let converter = self.create_converter();
        let convert_result = converter.convert(markdown_text, kroki_url, tmpdir.path());

        // Collect diagram attachments
        let attachments = Self::collect_attachments(tmpdir.path())?;

        // Fetch current page
        let current_page = self
            .client
            .get_page(page_id, &["body.storage", "version"])?;

        // Preserve comments
        let old_html = Self::extract_body_html(&current_page);
        let preserve_result = preserve_comments(old_html, &convert_result.html);

        // Determine title
        let title = convert_result.title.as_ref().unwrap_or(&current_page.title);

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
            warnings: convert_result.warnings,
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

        let converter = self.create_converter();
        let convert_result = converter.convert(markdown_text, kroki_url, tmpdir.path());

        let attachments = Self::collect_attachment_names(tmpdir.path())?;

        let current_page = self
            .client
            .get_page(page_id, &["body.storage", "version"])?;

        let old_html = Self::extract_body_html(&current_page);
        let preserve_result = preserve_comments(old_html, &convert_result.html);

        Ok(DryRunResult {
            html: preserve_result.html,
            title: convert_result.title,
            current_title: current_page.title,
            current_version: current_page.version.number,
            unmatched_comments: preserve_result.unmatched_comments,
            attachment_count: attachments.len(),
            attachment_names: attachments,
            warnings: convert_result.warnings,
        })
    }

    fn kroki_url(&self) -> Result<&str, UpdateError> {
        self.config.diagrams.kroki_url.as_deref().ok_or_else(|| {
            UpdateError::Config("kroki_url required (via --kroki-url or [diagrams] config)".into())
        })
    }

    fn create_converter(&self) -> MarkdownConverter {
        let diagrams = &self.config.diagrams;
        MarkdownConverter::new()
            .prepend_toc(true)
            .extract_title(self.config.extract_title)
            .include_dirs(diagrams.include_dirs.clone())
            .config_file(diagrams.config_file.as_deref())
            .dpi(diagrams.dpi)
    }

    fn collect_attachments(dir: &Path) -> Result<Vec<(String, Vec<u8>)>, UpdateError> {
        let mut attachments: Vec<_> = Self::png_files(dir)?
            .into_iter()
            .map(|path| {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("diagram.png")
                    .to_string();
                std::fs::read(&path).map(|data| (filename, data))
            })
            .collect::<Result<_, _>>()?;
        attachments.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(attachments)
    }

    fn collect_attachment_names(dir: &Path) -> Result<Vec<String>, UpdateError> {
        let mut names: Vec<_> = Self::png_files(dir)?
            .iter()
            .filter_map(|path| path.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect();
        names.sort();
        Ok(names)
    }

    fn png_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, UpdateError> {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "png") {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    fn extract_body_html(page: &Page) -> &str {
        page.body
            .as_ref()
            .and_then(|b| b.storage.as_ref())
            .map_or("", |s| s.value.as_str())
    }
}
