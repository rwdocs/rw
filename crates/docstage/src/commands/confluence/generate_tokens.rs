//! `docstage confluence generate-tokens` command implementation.

use std::io::{self, Write};
use std::path::PathBuf;

use clap::Args;
use docstage_config::Config;
use docstage_confluence::{OAuthTokenGenerator, oauth};

use crate::error::CliError;
use crate::output::Output;

/// Arguments for the confluence generate-tokens command.
#[derive(Args)]
pub struct GenerateTokensArgs {
    /// Path to RSA private key file.
    #[arg(short = 'k', long = "private-key", default_value = "private_key.pem")]
    private_key: PathBuf,

    /// OAuth consumer key (default: from config or "docstage").
    #[arg(long)]
    consumer_key: Option<String>,

    /// Confluence base URL (default: from config).
    #[arg(short = 'u', long)]
    base_url: Option<String>,

    /// Path to configuration file (default: auto-discover docstage.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,
}

impl GenerateTokensArgs {
    /// Execute the generate-tokens command.
    ///
    /// # Errors
    ///
    /// Returns an error if token generation fails.
    pub fn execute(self) -> Result<(), CliError> {
        let output = Output::new();

        // Load config
        let config = Config::load(self.config.as_deref(), None)?;

        // Resolve effective values
        let effective_consumer_key = self
            .consumer_key
            .or_else(|| config.confluence.as_ref().map(|c| c.consumer_key.clone()))
            .unwrap_or_else(|| "docstage".to_string());

        let effective_base_url = self
            .base_url
            .or_else(|| config.confluence.as_ref().map(|c| c.base_url.clone()));

        let Some(effective_base_url) = effective_base_url else {
            output.error("Error: base_url required (via --base-url or config)");
            return Err(CliError::Validation("base_url required".to_string()));
        };

        // Read private key
        output.info(&format!(
            "Reading private key from {}...",
            self.private_key.display()
        ));
        let key_bytes = oauth::read_private_key(&self.private_key)?;

        // Create generator
        let generator =
            OAuthTokenGenerator::new(&effective_base_url, &effective_consumer_key, &key_bytes)?;

        // Step 1: Get request token
        output.info("\nStep 1: Requesting temporary credentials...");
        let (request_token, auth_url) = generator.request_token()?;
        output.success("Temporary token received");

        // Step 2: User authorization
        output.separator();
        output.highlight("Step 2: Authorization Required");
        output.separator();
        output.info("\nPlease open this URL in your browser:");
        output.highlight(&format!("\n{auth_url}\n"));

        // Read verifier from stdin
        print!("Enter the verification code: ");
        io::stdout().flush()?;
        let mut verifier = String::new();
        io::stdin().read_line(&mut verifier)?;
        let verifier = verifier.trim();

        // Step 3: Exchange for access token
        output.info("\nStep 3: Exchanging for access token...");
        let access_token = generator.exchange_verifier(&request_token, verifier)?;

        // Output results
        output.separator();
        output.success("OAuth Authorization Successful!");
        output.separator();
        output.info("\nAdd these credentials to your docstage.toml:");
        output.info("\n[confluence]");
        output.info(&format!(r#"base_url = "{effective_base_url}""#));
        output.info(&format!(r#"access_token = "{}""#, access_token.oauth_token));
        output.info(&format!(
            r#"access_secret = "{}""#,
            access_token.oauth_token_secret
        ));
        output.info(&format!(r#"consumer_key = "{effective_consumer_key}""#));

        Ok(())
    }
}
