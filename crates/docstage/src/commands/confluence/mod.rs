//! Confluence subcommand group.

mod generate_tokens;
mod update;

use clap::Subcommand;

pub use generate_tokens::GenerateTokensArgs;
pub use update::UpdateArgs;

use crate::error::CliError;

/// Confluence publishing commands.
#[derive(Subcommand)]
pub enum ConfluenceCommand {
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
    pub fn execute(self) -> Result<(), CliError> {
        match self {
            Self::Update(args) => args.execute(),
            Self::GenerateTokens(args) => args.execute(),
        }
    }
}
