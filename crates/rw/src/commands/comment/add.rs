use std::sync::Arc;

use clap::Args;
use rw_cache::NullCache;
use rw_comments::{NewComment, create_comment};
use rw_config::Config;
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

use super::{AuthorArgs, Context, OutputArgs, format, identity};
use crate::error::CliError;

#[derive(Args, Debug)]
pub(crate) struct AddArgs {
    #[command(flatten)]
    pub output: OutputArgs,

    #[command(flatten)]
    pub author: AuthorArgs,

    /// Document id (URL path without leading slash, e.g. "guide" or
    /// "domain/billing/overview").
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

    let input = NewComment {
        document_id: document,
        parent_id: None,
        author,
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

fn build_site(config: &Config) -> Site {
    let storage: Arc<dyn rw_storage::Storage> = Arc::new(FsStorage::with_meta_filename(
        config.docs_resolved.source_dir.clone(),
        &config.metadata.name,
    ));
    let cache: Arc<dyn rw_cache::Cache> = Arc::new(NullCache);
    let renderer_config = PageRendererConfig {
        kroki_url: config.diagrams_resolved.kroki_url.clone(),
        include_dirs: config.diagrams_resolved.include_dirs.clone(),
        dpi: config.diagrams_resolved.dpi,
        ..PageRendererConfig::default()
    };
    Site::new(storage, cache, renderer_config)
}
