//! Confluence subcommand group.

mod render;

use clap::Subcommand;

use render::RenderArgs;

use crate::error::CliError;

/// Confluence rendering commands.
#[derive(Subcommand)]
pub(crate) enum ConfluenceCommand {
    /// Render a markdown file into a Confluence-publishable bundle
    /// (XHTML body and diagram PNGs).
    Render(RenderArgs),
}

impl ConfluenceCommand {
    /// Execute the confluence subcommand.
    ///
    /// # Errors
    ///
    /// Returns an error if the subcommand fails.
    pub(crate) fn execute(self) -> Result<(), CliError> {
        match self {
            Self::Render(args) => args.execute(),
        }
    }
}
