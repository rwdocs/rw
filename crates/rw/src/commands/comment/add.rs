use clap::Args;
use rw_comments::{NewComment, create_comment, resolve_quote};

use super::context::{build_site, document_key, document_url_path};
use super::{AuthorArgs, Context, OutputArgs, format, identity};
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
    let document = document_url_path(&ctx.config, &document)?;
    let document_id = document_key(&site, &document)?;

    // Resolve the quote against the page URL path (the composite key is not a
    // renderable path), then store under the composite key — mirroring the
    // browser, which sends pre-resolved selectors.
    let selectors = match &quote {
        Some(q) => Some(resolve_quote(&site, &document, q)?),
        None => None,
    };

    let input = NewComment {
        document_id,
        parent_id: None,
        author,
        body,
        selectors,
        quote: None,
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

    super::notify::notify_server(&ctx.config.docs_resolved.project_dir);

    Ok(())
}
