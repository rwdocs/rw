use clap::Args;
use rw_comments::{Comment, CommentFilter};
use serde::Serialize;
use uuid::Uuid;

use super::{Context, OutputArgs, format};
use crate::error::CliError;

#[derive(Args, Debug)]
pub(crate) struct ShowArgs {
    #[command(flatten)]
    pub output: OutputArgs,

    /// Comment id to show.
    pub id: Uuid,
}

#[derive(Debug, Serialize)]
struct ShowPayload<'a> {
    comment: &'a Comment,
    replies: &'a [Comment],
}

pub(crate) async fn run(ctx: &Context, args: ShowArgs) -> Result<(), CliError> {
    let ShowArgs { output, id } = args;

    let parent = ctx.store.get(id).await?;
    let replies = ctx
        .store
        .list(CommentFilter {
            parent_id: Some(parent.id),
            ..CommentFilter::default()
        })
        .await?;

    format::print(
        output.format,
        |out| format::write_show(out, &parent, &replies),
        &ShowPayload {
            comment: &parent,
            replies: &replies,
        },
    )?;

    Ok(())
}
