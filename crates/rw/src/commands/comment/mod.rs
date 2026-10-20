mod add;
mod context;
mod format;
mod identity;
mod list;
mod notify;
mod reply;
mod resolve;
mod show;

use clap::{Args, Subcommand};
use rw_comments::Author;

use crate::commands::ProjectArgs;
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
    #[arg(
        long,
        env = "RW_COMMENT_AUTHOR_ID",
        help = format!(
            "Author id for new comments (default `{}` when both author options are unset)",
            Author::local_ai().id,
        ),
    )]
    pub author_id: Option<String>,

    #[arg(
        long,
        env = "RW_COMMENT_AUTHOR_NAME",
        help = format!(
            "Author display name for new comments (default `{}` when both author options are unset)",
            Author::local_ai().name,
        ),
    )]
    pub author_name: Option<String>,
}

#[derive(Copy, Clone, Debug, Default, clap::ValueEnum)]
pub(crate) enum OutputFormat {
    #[default]
    Text,
    Json,
}

// `ProjectArgs` is flattened into each subcommand rather than made global on
// the group: clap does not check `--config` against `--project-dir` when one
// is given before the subcommand and the other after it.
#[derive(Subcommand, Debug)]
pub(crate) enum CommentCommand {
    /// List comment threads.
    List {
        #[command(flatten)]
        project: ProjectArgs,
        #[command(flatten)]
        args: list::ListArgs,
    },
    /// Show a single comment and its replies.
    Show {
        #[command(flatten)]
        project: ProjectArgs,
        #[command(flatten)]
        args: show::ShowArgs,
    },
    /// Create a new comment.
    Add {
        #[command(flatten)]
        project: ProjectArgs,
        #[command(flatten)]
        args: add::AddArgs,
    },
    /// Reply to an existing comment thread.
    Reply {
        #[command(flatten)]
        project: ProjectArgs,
        #[command(flatten)]
        args: reply::ReplyArgs,
    },
    /// Resolve a comment thread.
    Resolve {
        #[command(flatten)]
        project: ProjectArgs,
        #[command(flatten)]
        args: resolve::ResolveArgs,
    },
}

impl CommentCommand {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async move {
            match self {
                Self::List { project, args } => {
                    list::run(&Context::load(&project, true).await?, args).await
                }
                Self::Show { project, args } => {
                    show::run(&Context::load(&project, true).await?, args).await
                }
                Self::Add { project, args } => {
                    add::run(&Context::load(&project, false).await?, args).await
                }
                Self::Reply { project, args } => {
                    reply::run(&Context::load(&project, false).await?, args).await
                }
                Self::Resolve { project, args } => {
                    resolve::run(&Context::load(&project, false).await?, args).await
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use clap::Command;

    use super::*;

    fn help_for(arg_id: &str) -> String {
        let cmd = AuthorArgs::augment_args(Command::new("test"));
        let arg = cmd
            .get_arguments()
            .find(|arg| arg.get_id() == arg_id)
            .unwrap();
        arg.get_help().unwrap().to_string()
    }

    #[test]
    fn author_help_names_the_actual_default() {
        let default = Author::local_ai();
        assert!(help_for("author_id").contains(&format!("`{}`", default.id)));
        assert!(help_for("author_name").contains(&format!("`{}`", default.name)));
    }
}
