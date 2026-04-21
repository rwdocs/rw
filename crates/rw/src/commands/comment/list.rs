use clap::Args;
use rw_comments::{CommentFilter, CommentStatus};
use uuid::Uuid;

use super::{Context, OutputArgs, format};
use crate::error::CliError;

#[derive(Args, Debug)]
pub(crate) struct ListArgs {
    #[command(flatten)]
    pub output: OutputArgs,

    /// Filter by document id.
    #[arg(long)]
    pub document: Option<String>,

    /// Status filter. Default: `open`. Use `all` for no filter.
    #[arg(long, default_value = "open")]
    pub status: StatusFilter,

    /// Filter to replies of a specific parent (pass a comment UUID).
    /// Use `all` to include every comment regardless of depth. Omit the
    /// flag to keep only top-level threads (no replies).
    #[arg(long)]
    pub parent: Option<String>,
}

#[derive(Copy, Clone, Debug, clap::ValueEnum)]
pub(crate) enum StatusFilter {
    Open,
    Resolved,
    All,
}

impl StatusFilter {
    fn as_store_filter(self) -> Option<CommentStatus> {
        match self {
            StatusFilter::Open => Some(CommentStatus::Open),
            StatusFilter::Resolved => Some(CommentStatus::Resolved),
            StatusFilter::All => None,
        }
    }
}

pub(crate) async fn run(ctx: &Context, args: ListArgs) -> Result<(), CliError> {
    let ListArgs {
        output,
        document,
        status,
        parent,
    } = args;

    let mut filter = CommentFilter {
        document_id: document,
        status: status.as_store_filter(),
        ..CommentFilter::default()
    };
    match parent.as_deref() {
        None => filter.top_level_only = true,
        Some("all") => {}
        Some(raw) => {
            filter.parent_id = Some(
                Uuid::parse_str(raw)
                    .map_err(|_| CliError::Validation(format!("invalid --parent uuid: {raw}")))?,
            );
        }
    }

    let comments = ctx.store.list(filter).await?;

    format::print(
        output.format,
        |out| format::write_list(out, &comments),
        &comments,
    )?;

    Ok(())
}
