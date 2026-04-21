use rw_site::Site;

use crate::anchoring::resolve_quote;
use crate::error::CreateError;
use crate::model::{CreateComment, NewComment};
use crate::sqlite::SqliteCommentStore;
use crate::{Comment, Selector};

/// Create a comment, turning a `quote` into selectors first if one was given.
///
/// - `input.selectors = Some(non-empty)` and `input.quote = None` — the
///   selectors are used verbatim.
/// - `input.quote = Some(_)` and `input.selectors` is `None` or an empty list
///   — the quote is resolved against `site` into a `TextQuote` +
///   `TextPosition` selector pair.
/// - Both set — [`CreateError::BothQuoteAndSelectors`].
/// - Neither set — the comment is created with no anchor (page-level comment).
///
/// # Errors
///
/// Returns [`CreateError::BothQuoteAndSelectors`] when the caller supplies
/// both `quote` and `selectors`, [`CreateError::Quote`] when the quote cannot
/// be resolved, and [`CreateError::Store`] for underlying storage failures.
pub async fn create_comment(
    store: &SqliteCommentStore,
    site: &Site,
    input: NewComment,
) -> Result<Comment, CreateError> {
    let NewComment {
        document_id,
        parent_id,
        author,
        body,
        selectors,
        quote,
    } = input;

    let selectors = resolve_selectors(site, &document_id, selectors, quote)?;

    store
        .create(CreateComment {
            document_id,
            parent_id,
            author,
            body,
            selectors,
        })
        .await
        .map_err(Into::into)
}

fn resolve_selectors(
    site: &Site,
    document_id: &str,
    selectors: Option<Vec<Selector>>,
    quote: Option<String>,
) -> Result<Vec<Selector>, CreateError> {
    match (quote, selectors) {
        (Some(_), Some(s)) if !s.is_empty() => Err(CreateError::BothQuoteAndSelectors),
        (Some(quote), _) => Ok(resolve_quote(site, document_id, &quote)?),
        (None, Some(s)) => Ok(s),
        (None, None) => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pretty_assertions::assert_eq;
    use rw_cache::NullCache;
    use rw_site::PageRendererConfig;
    use rw_storage::MockStorage;

    use super::*;
    use crate::model::Author;

    fn empty_site() -> Site {
        Site::new(
            Arc::new(MockStorage::new()),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        )
    }

    fn new_comment(author: Option<Author>) -> NewComment {
        NewComment {
            document_id: "guide".to_owned(),
            parent_id: None,
            author,
            body: "hi".to_owned(),
            selectors: None,
            quote: None,
        }
    }

    #[tokio::test]
    async fn stamps_local_human_when_author_omitted() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let site = empty_site();

        let comment = create_comment(&store, &site, new_comment(None))
            .await
            .unwrap();

        assert_eq!(comment.author, Author::local_human());
    }

    #[tokio::test]
    async fn preserves_supplied_author() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let site = empty_site();

        let claimed = Author {
            id: "local:claude-code".to_owned(),
            name: "Claude Code".to_owned(),
            avatar_url: None,
        };
        let comment = create_comment(&store, &site, new_comment(Some(claimed.clone())))
            .await
            .unwrap();

        assert_eq!(comment.author, claimed);
    }
}
