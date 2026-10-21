use clap::Args;
use rw_comments::{NewComment, create_comment};

use super::context::{document_key, document_url_path};
use super::{AuthorArgs, Context, OutputArgs, format, identity};
use crate::commands::build_site;
use crate::error::CliError;

#[derive(Args, Debug)]
pub(crate) struct AddArgs {
    #[command(flatten)]
    pub output: OutputArgs,

    #[command(flatten)]
    pub author: AuthorArgs,

    /// Page to comment on: a URL path ("billing/overview") or the markdown source
    /// file, with or without the docs prefix ("docs/billing/overview.md" or "billing/overview.md").
    #[arg(long)]
    pub document: String,

    /// Comment body.
    #[arg(long)]
    pub body: String,

    /// Anchor text for inline comments. Omit for a page-level comment.
    #[arg(long)]
    pub quote: Option<String>,
}

pub(crate) async fn run(ctx: &Context, args: AddArgs) -> Result<(), CliError> {
    let AddArgs {
        output,
        author,
        document,
        body,
        quote,
    } = args;

    let author =
        identity::resolve_author(author.author_id.as_deref(), author.author_name.as_deref())?;
    let site = build_site(&ctx.config);
    // `--document` may be a URL path or a source file path (…/foo.md); resolve
    // to the page's URL path, then key on (sectionRef, subpath).
    let document = document_url_path(&ctx.config, &site, &document)?;
    let document_id = document_key(&site, &document)?;

    let input = NewComment {
        document_id,
        parent_id: None,
        author: Some(author),
        body,
        selectors: None,
        quote,
    };

    let comment = create_comment(&ctx.store, &site, input).await?;

    format::print(
        output.format,
        |out| {
            writeln!(
                out,
                "Created comment {} on document '{}'.",
                comment.id, comment.document_id
            )
        },
        &comment,
    )?;

    Ok(())
}
