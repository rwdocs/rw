use clap::Args;
use rw_comments::{CommentStatus, UpdateComment};
use uuid::Uuid;

use super::{Context, OutputArgs, format};
use crate::error::CliError;

#[derive(Args, Debug)]
pub(crate) struct ResolveArgs {
    #[command(flatten)]
    pub output: OutputArgs,

    /// Comment id to resolve.
    pub id: Uuid,
}

pub(crate) async fn run(ctx: &Context, args: ResolveArgs) -> Result<(), CliError> {
    let ResolveArgs { output, id } = args;

    let comment = ctx
        .store
        .update(
            id,
            UpdateComment {
                body: None,
                status: Some(CommentStatus::Resolved),
                selectors: None,
            },
        )
        .await?;

    format::print(
        output.format,
        |out| writeln!(out, "Resolved {}.", comment.id),
        &comment,
    )?;

    Ok(())
}
