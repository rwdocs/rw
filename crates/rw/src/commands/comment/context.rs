use rw_comments::SqliteCommentStore;
use rw_config::Config;

use crate::error::CliError;

/// Per-invocation context shared across subcommands.
pub(super) struct Context {
    pub config: Config,
    pub store: SqliteCommentStore,
}

impl Context {
    pub(super) async fn load() -> Result<Self, CliError> {
        let config = Config::load(None, None)?;
        let path = SqliteCommentStore::default_path(&config.docs_resolved.project_dir);
        let store = SqliteCommentStore::open(&path).await?;
        Ok(Self { config, store })
    }
}
