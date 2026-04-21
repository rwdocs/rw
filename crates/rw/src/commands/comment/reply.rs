use clap::Args;
use rw_comments::CreateComment;
use uuid::Uuid;

use super::{AuthorArgs, Context, OutputArgs, format, identity};
use crate::error::CliError;

#[derive(Args, Debug)]
pub(crate) struct ReplyArgs {
    #[command(flatten)]
    pub output: OutputArgs,

    #[command(flatten)]
    pub author: AuthorArgs,

    /// Id of the comment to reply to.
    pub parent_id: Uuid,

    /// Reply body.
    #[arg(long)]
    pub body: String,
}

pub(crate) async fn run(ctx: &Context, args: ReplyArgs) -> Result<(), CliError> {
    let ReplyArgs {
        output,
        author,
        parent_id,
        body,
    } = args;

    let author =
        identity::resolve_author(author.author_id.as_deref(), author.author_name.as_deref())?;
    let parent = ctx.store.get(parent_id).await?;

    let reply = ctx
        .store
        .create(CreateComment {
            document_id: parent.document_id.clone(),
            parent_id: Some(parent.id),
            author,
            body,
            selectors: Vec::new(),
        })
        .await?;

    format::print(
        output.format,
        |out| writeln!(out, "Replied to {} on '{}'.", parent.id, parent.document_id),
        &reply,
    )?;

    Ok(())
}
