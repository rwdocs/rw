mod add;
mod context;
mod format;
mod identity;
mod list;
mod reply;
mod resolve;
mod show;

use clap::{Args, Subcommand};

use crate::error::CliError;
use context::Context;

/// Output options shared by every `rw comment` subcommand.
#[derive(Args, Clone, Debug)]
pub(crate) struct OutputArgs {
    /// Output format.
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

/// Author claim for subcommands that create new comments.
#[derive(Args, Clone, Debug)]
pub(crate) struct AuthorArgs {
    /// Author id for new comments (omit to use the default `local:human`
    /// identity).
    #[arg(long, env = "RW_COMMENT_AUTHOR_ID")]
    pub author_id: Option<String>,

    /// Author display name for new comments (omit to use the default `You`
    /// identity).
    #[arg(long, env = "RW_COMMENT_AUTHOR_NAME")]
    pub author_name: Option<String>,
}

#[derive(Copy, Clone, Debug, Default, clap::ValueEnum)]
pub(crate) enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Subcommand, Debug)]
pub(crate) enum CommentCommand {
    /// List comment threads.
    List(list::ListArgs),
    /// Show a single comment and its replies.
    Show(show::ShowArgs),
    /// Create a new comment.
    Add(add::AddArgs),
    /// Reply to an existing comment thread.
    Reply(reply::ReplyArgs),
    /// Resolve a comment thread.
    Resolve(resolve::ResolveArgs),
}

impl CommentCommand {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async move {
            let ctx = Context::load().await?;
            match self {
                Self::List(args) => list::run(&ctx, args).await,
                Self::Show(args) => show::run(&ctx, args).await,
                Self::Add(args) => add::run(&ctx, args).await,
                Self::Reply(args) => reply::run(&ctx, args).await,
                Self::Resolve(args) => resolve::run(&ctx, args).await,
            }
        })
    }
}
