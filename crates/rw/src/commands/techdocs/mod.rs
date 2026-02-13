//! `rw techdocs` subcommand group.

mod build;
mod publish;

use clap::Subcommand;

use build::BuildArgs;
use publish::PublishArgs;

use crate::error::CliError;

/// `TechDocs` commands.
#[derive(Subcommand)]
pub(crate) enum TechdocsCommand {
    /// Build a static documentation site.
    Build(BuildArgs),
    /// Publish a built site to S3.
    Publish(PublishArgs),
}

impl TechdocsCommand {
    /// Execute the techdocs subcommand.
    pub(crate) fn execute(self) -> Result<(), CliError> {
        match self {
            Self::Build(args) => args.execute(),
            Self::Publish(args) => args.execute(),
        }
    }
}
