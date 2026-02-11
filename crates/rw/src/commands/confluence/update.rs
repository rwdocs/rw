//! `rw confluence update` command implementation.

use std::path::{Path, PathBuf};

use clap::Args;
use rw_config::{CliSettings, Config, ConfluenceConfig};
use rw_confluence::{ConfluenceClient, DryRunResult, PageUpdater, UpdateConfig, UpdateResult};

use crate::error::CliError;
use crate::output::Output;

/// Arguments for the confluence update command.
#[derive(Args)]
pub(crate) struct UpdateArgs {
    /// Path to the markdown file.
    markdown_file: PathBuf,

    /// Confluence page ID to update.
    page_id: String,

    /// Version message for the update.
    #[arg(short, long)]
    message: Option<String>,

    /// Kroki server URL for diagram rendering (overrides config).
    #[arg(long)]
    kroki_url: Option<String>,

    /// Extract title from first H1 heading and update page title (default: enabled).
    #[arg(long, default_value = "true")]
    extract_title: Option<bool>,

    /// Do not extract title from first H1 heading.
    #[arg(long, conflicts_with = "extract_title")]
    no_extract_title: bool,

    /// Preview changes without updating Confluence.
    #[arg(long)]
    dry_run: bool,

    /// Path to configuration file (default: auto-discover rw.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Path to OAuth private key file.
    #[arg(short = 'k', long, default_value = "private_key.pem")]
    key_file: PathBuf,
}

impl UpdateArgs {
    /// Execute the update command.
    ///
    /// # Errors
    ///
    /// Returns an error if the update fails.
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let output = Output::new();

        // Load config
        let cli_settings = CliSettings {
            kroki_url: self.kroki_url.clone(),
            ..Default::default()
        };
        let config = Config::load(self.config.as_deref(), Some(&cli_settings))?;

        // Require confluence config
        let conf_config = require_confluence_config(&config, &output)?;

        // Create Confluence client
        let client = create_confluence_client(conf_config, &self.key_file)?;

        // Read markdown file
        let markdown_text = std::fs::read_to_string(&self.markdown_file)?;
        output.info(&format!("Converting {}...", self.markdown_file.display()));

        // Create update config
        let update_config = UpdateConfig {
            diagrams: config.diagrams_resolved.clone(),
            extract_title: self.resolve_extract_title(),
        };
        let updater = PageUpdater::new(&client, update_config);

        if self.dry_run {
            let result = updater.dry_run(&self.page_id, &markdown_text)?;
            print_dry_run_result(&output, &result);
        } else {
            let result = updater.update(&self.page_id, &markdown_text, self.message.as_deref())?;
            print_update_result(&output, &result);
        }

        Ok(())
    }

    fn resolve_extract_title(&self) -> bool {
        !self.no_extract_title && self.extract_title.unwrap_or(true)
    }
}

fn require_confluence_config<'a>(
    config: &'a Config,
    output: &Output,
) -> Result<&'a ConfluenceConfig, CliError> {
    config.confluence.as_ref().ok_or_else(|| {
        output.error("Error: confluence configuration required in rw.toml");
        output.info("\nAdd the following to your rw.toml:");
        output.info("\n[confluence]");
        output.info(r#"base_url = "https://confluence.example.com""#);
        output.info(r#"access_token = "your-token""#);
        output.info(r#"access_secret = "your-secret""#);
        CliError::Validation("confluence configuration required".to_owned())
    })
}

fn create_confluence_client(
    conf_config: &ConfluenceConfig,
    key_file: &Path,
) -> Result<ConfluenceClient, CliError> {
    let client = ConfluenceClient::from_config(
        &conf_config.base_url,
        &conf_config.consumer_key,
        key_file,
        &conf_config.access_token,
        &conf_config.access_secret,
    )?;
    Ok(client)
}

fn print_dry_run_result(output: &Output, result: &DryRunResult) {
    output.highlight("\n[DRY RUN] No changes made.");

    if let Some(title) = &result.title {
        output.info(&format!("Title: {title}"));
    }
    output.info(&format!(
        "Current page: \"{}\" (v{})",
        result.current_title, result.current_version
    ));

    if result.attachment_count > 0 {
        output.info(&format!("\nAttachments ({}):", result.attachment_count));
        for name in &result.attachment_names {
            output.info(&format!("  -> {name}"));
        }
    }

    if result.unmatched_comments.is_empty() {
        output.success("\nNo comments would be resolved.");
    } else {
        output.warning(&format!(
            "\nComments that would be resolved ({}):",
            result.unmatched_comments.len()
        ));
        for comment in &result.unmatched_comments {
            output.info(&format!(r#"  - [{}] "{}""#, comment.ref_id, comment.text));
        }
    }
}

fn print_update_result(output: &Output, result: &UpdateResult) {
    output.success("\nPage updated successfully!");
    output.info(&format!("ID: {}", result.page.id));
    output.info(&format!("Title: {}", result.page.title));
    output.info(&format!("Version: {}", result.page.version.number));
    output.info(&format!("URL: {}", result.url));
    output.info(&format!("\nComments on page: {}", result.comment_count));

    if result.attachments_uploaded > 0 {
        output.info(&format!(
            "Attachments uploaded: {}",
            result.attachments_uploaded
        ));
    }

    if !result.unmatched_comments.is_empty() {
        output.warning(&format!(
            "\nWarning: {} comment(s) could not be placed:",
            result.unmatched_comments.len()
        ));
        for comment in &result.unmatched_comments {
            output.info(&format!(r#"  - [{}] "{}""#, comment.ref_id, comment.text));
        }
    }
}
