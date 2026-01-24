//! Confluence subcommand group.

mod generate_tokens;
mod update;

use clap::Subcommand;

use generate_tokens::GenerateTokensArgs;
use update::UpdateArgs;

use crate::error::CliError;

/// Confluence publishing commands.
#[derive(Subcommand)]
pub(crate) enum ConfluenceCommand {
    /// Update a Confluence page from a markdown file.
    Update(UpdateArgs),
    /// Generate OAuth access tokens for Confluence.
    GenerateTokens(GenerateTokensArgs),
}

impl ConfluenceCommand {
    /// Execute the confluence subcommand.
    ///
    /// # Errors
    ///
    /// Returns an error if the subcommand fails.
    pub(crate) fn execute(self) -> Result<(), CliError> {
        match self {
            Self::Update(args) => args.execute(),
            Self::GenerateTokens(args) => args.execute(),
        }
    }
}
