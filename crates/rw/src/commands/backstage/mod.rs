//! `rw backstage` subcommand group.

mod publish;

use clap::Subcommand;

use publish::PublishArgs;

use crate::error::CliError;

/// Backstage commands.
#[derive(Subcommand)]
pub(crate) enum BackstageCommand {
    /// Publish documentation bundles to S3 for Backstage.
    Publish(PublishArgs),
}

impl BackstageCommand {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        match self {
            Self::Publish(args) => args.execute(),
        }
    }
}
