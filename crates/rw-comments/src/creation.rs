use rw_site::Site;

use crate::Comment;
use crate::anchoring::{QuoteResolutionError, resolve_quote};
use crate::error::CreateError;
use crate::model::{CreateComment, NewComment};
use crate::sqlite::SqliteCommentStore;

fn page_path_for_key(site: &Site, document_id: &str) -> Result<String, QuoteResolutionError> {
    let render_failed = |source| QuoteResolutionError::RenderFailed {
        document_id: document_id.to_owned(),
        source,
    };
    let page_path = site
        .page_for_document_id(document_id)
        .map_err(render_failed)?
        .ok_or_else(|| QuoteResolutionError::DocumentNotFound {
            document_id: document_id.to_owned(),
        })?;
    if !site.has_page(&page_path).map_err(render_failed)? {
        return Err(QuoteResolutionError::DocumentNotFound {
            document_id: page_path,
        });
    }
    Ok(page_path)
}

/// A comment whose target page exists and whose anchor is resolved, ready to
/// [`insert`](Self::insert).
#[derive(Debug)]
pub struct PreparedComment(CreateComment);

impl PreparedComment {
    /// Store the comment.
    ///
    /// # Errors
    ///
    /// Returns [`CreateError::Store`] when the store rejects or fails the
    /// insert.
    pub async fn insert(self, store: &SqliteCommentStore) -> Result<Comment, CreateError> {
        Ok(store.create(self.0).await?)
    }
}

/// Check that the comment's page exists, and turn a `quote` into selectors if
/// one was given. Renders the page for a quote, so it can block.
///
/// - `input.selectors = Some(non-empty)` and `input.quote = None` — the
///   selectors are used verbatim.
/// - `input.quote = Some(_)` and `input.selectors` is `None` or an empty list
///   — the quote is resolved against `site` into a `TextQuote` +
///   `TextPosition` selector pair.
/// - Both set — [`CreateError::BothQuoteAndSelectors`].
/// - Neither set — the comment has no anchor (page-level comment).
///
/// # Errors
///
/// Returns [`CreateError::BothQuoteAndSelectors`] when the caller supplies
/// both `quote` and `selectors`, and [`CreateError::Quote`] when the page does
/// not exist or the quote cannot be resolved.
pub fn prepare_comment(site: &Site, input: NewComment) -> Result<PreparedComment, CreateError> {
    let NewComment {
        document_id,
        parent_id,
        author,
        body,
        selectors,
        quote,
    } = input;

    let page_path = page_path_for_key(site, &document_id)?;
    let selectors = match (quote, selectors) {
        (Some(_), Some(s)) if !s.is_empty() => return Err(CreateError::BothQuoteAndSelectors),
        (Some(quote), _) => resolve_quote(site, &page_path, &quote)?,
        (None, s) => s.unwrap_or_default(),
    };

    Ok(PreparedComment(CreateComment {
        document_id,
        parent_id,
        author,
        body,
        selectors,
    }))
}

/// [`prepare_comment`], then [`PreparedComment::insert`].
///
/// # Errors
///
/// Returns the errors of both steps.
pub async fn create_comment(
    store: &SqliteCommentStore,
    site: &Site,
    input: NewComment,
) -> Result<Comment, CreateError> {
    prepare_comment(site, input)?.insert(store).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pretty_assertions::assert_eq;
    use rw_cache::NullCache;
    use rw_error::error_chain;
    use rw_site::PageRendererConfig;
    use rw_storage::{MockStorage, StorageErrorKind};

    use super::*;
    use crate::anchoring::QuoteResolutionError;
    use crate::model::{Author, CommentFilter, Selector};

    /// A one-page site whose page has no section `kind`, so it lands under the
    /// implicit root — its composite comment key is therefore
    /// `section:default/root#{path}`.
    fn seeded_site(path: &str, markdown: &str) -> Site {
        let storage = Arc::new(
            MockStorage::new()
                .with_file(path, path, markdown)
                .with_mtime(path, 0.0),
        );
        Site::new(storage, Arc::new(NullCache), PageRendererConfig::default())
    }

    fn new_comment(author: Option<Author>) -> NewComment {
        NewComment {
            document_id: "section:default/root#guide".to_owned(),
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
        let site = seeded_site("guide", "# Guide\n");

        let comment = create_comment(&store, &site, new_comment(None))
            .await
            .unwrap();

        assert_eq!(comment.author, Author::local_human());
    }

    #[tokio::test]
    async fn preserves_supplied_author() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let site = seeded_site("guide", "# Guide\n");

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

    #[tokio::test]
    async fn missing_page_is_rejected_and_nothing_is_stored() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let site = seeded_site("guide", "# Guide\n");
        let gone = "section:default/root#gone";
        let thread = store
            .create(CreateComment {
                document_id: gone.to_owned(),
                parent_id: None,
                author: None,
                body: "thread".to_owned(),
                selectors: Vec::new(),
            })
            .await
            .unwrap();
        let missing = "section:default/root#nope";

        for input in [
            NewComment {
                document_id: missing.to_owned(),
                ..new_comment(None)
            },
            NewComment {
                document_id: missing.to_owned(),
                selectors: Some(vec![Selector::TextPositionSelector { start: 0, end: 1 }]),
                ..new_comment(None)
            },
            NewComment {
                document_id: gone.to_owned(),
                parent_id: Some(thread.id),
                ..new_comment(None)
            },
            NewComment {
                document_id: "guide".to_owned(),
                ..new_comment(None)
            },
        ] {
            let document_id = input.document_id.clone();
            let err = create_comment(&store, &site, input).await.unwrap_err();

            assert!(
                matches!(
                    err,
                    CreateError::Quote(QuoteResolutionError::DocumentNotFound { .. })
                ),
                "{document_id}: expected DocumentNotFound, got {err:?}",
            );
        }
        assert_eq!(
            store.list(CommentFilter::default()).await.unwrap(),
            vec![thread]
        );
    }

    #[tokio::test]
    async fn virtual_page_is_a_valid_target() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let storage = Arc::new(MockStorage::new().with_virtual_page("area", "title: Area"));
        let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());

        let input = NewComment {
            document_id: "section:default/root#area".to_owned(),
            ..new_comment(None)
        };

        create_comment(&store, &site, input).await.unwrap();
    }

    #[tokio::test]
    async fn quote_resolves_against_composite_document_key() {
        // Browser/CLI store comments under the composite "{sectionRef}#{subpath}"
        // key, not the URL path. create_comment must decode it to find the page
        // to anchor the quote against — rendering the key verbatim 404s.
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let site = seeded_site("guide", "# Guide\n\nThe quick brown fox jumps.\n");

        let input = NewComment {
            document_id: "section:default/root#guide".to_owned(),
            parent_id: None,
            author: None,
            body: "anchored".to_owned(),
            selectors: None,
            quote: Some("brown fox".to_owned()),
        };

        let comment = create_comment(&store, &site, input).await.unwrap();

        // Stored under the composite key, with the quote anchored to the actual
        // passage — a TextQuote (carrying the matched text) + TextPosition pair.
        assert_eq!(comment.document_id, "section:default/root#guide");
        assert_eq!(comment.selectors.len(), 2);
        assert!(
            comment.selectors.iter().any(|s| matches!(
                s,
                Selector::TextQuoteSelector { exact, .. } if exact == "brown fox"
            )),
            "expected a TextQuoteSelector for the anchored passage, got {:?}",
            comment.selectors,
        );
    }

    #[tokio::test]
    async fn empty_quote_is_rejected_and_nothing_is_stored() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let site = seeded_site("guide", "# Guide\n\nThe quick brown fox jumps.\n");

        let input = NewComment {
            document_id: "section:default/root#guide".to_owned(),
            quote: Some(String::new()),
            ..new_comment(None)
        };

        let err = create_comment(&store, &site, input).await.unwrap_err();

        assert!(
            matches!(err, CreateError::Quote(QuoteResolutionError::EmptyQuote)),
            "expected EmptyQuote, got {err:?}",
        );
        assert_eq!(
            store.list(CommentFilter::default()).await.unwrap(),
            Vec::new()
        );
    }

    #[tokio::test]
    async fn quote_resolves_for_explicit_section_before_first_load() {
        // An EXPLICIT section's ref (here `domain:default/billing`) exists only
        // once the site has loaded. The implicit root is always in the map, so
        // root-page keys resolve even on a fresh Site — but an explicit section
        // is absent from the empty initial snapshot. `try_page_path_for` must
        // trigger a real load, not read that snapshot alone, or the key
        // resolves to no page. Here `create_comment` is the first thing to
        // touch the Site.
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let storage = Arc::new(
            MockStorage::new()
                .with_document_meta("billing", "title: Billing\nkind: domain")
                .with_mtime("billing", 0.0)
                .with_file(
                    "billing/payments",
                    "Payments",
                    "# Payments\n\nThe quick brown fox jumps.\n",
                )
                .with_mtime("billing/payments", 0.0),
        );
        let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());

        let input = NewComment {
            document_id: "domain:default/billing#payments".to_owned(),
            parent_id: None,
            author: None,
            body: "anchored".to_owned(),
            selectors: None,
            quote: Some("brown fox".to_owned()),
        };

        let comment = create_comment(&store, &site, input).await.unwrap();

        assert_eq!(comment.document_id, "domain:default/billing#payments");
        assert_eq!(comment.selectors.len(), 2);
    }

    #[tokio::test]
    async fn quote_resolves_when_section_name_and_subpath_contain_hash() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let storage = Arc::new(
            MockStorage::new()
                .with_document_meta("sys#1", "title: Sys\nkind: system")
                .with_mtime("sys#1", 0.0)
                .with_file("sys#1/c#d", "C", "# C\n\nThe quick brown fox jumps.\n")
                .with_mtime("sys#1/c#d", 0.0),
        );
        let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());

        let input = NewComment {
            document_id: "system:default/sys#1#c#d".to_owned(),
            quote: Some("brown fox".to_owned()),
            ..new_comment(None)
        };

        let comment = create_comment(&store, &site, input).await.unwrap();

        assert_eq!(comment.document_id, "system:default/sys#1#c#d");
        assert_eq!(comment.selectors.len(), 2);
    }

    #[tokio::test]
    async fn quote_prefers_longest_section_ref_when_a_shorter_one_also_matches() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let storage = Arc::new(
            MockStorage::new()
                .with_document_meta("sys", "title: Sys\nkind: system")
                .with_content("sys", "# Sys\n\nNothing here.\n")
                .with_mtime("sys", 0.0)
                .with_document_meta("sys#1", "title: Sys 1\nkind: system")
                .with_content("sys#1", "# Sys 1\n\nThe quick brown fox jumps.\n")
                .with_mtime("sys#1", 0.0)
                .with_file("sys#1/api", "API", "# API\n\nThe quick brown fox jumps.\n")
                .with_mtime("sys#1/api", 0.0),
        );
        let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());

        for document_id in ["system:default/sys#1#api", "system:default/sys#1#"] {
            let input = NewComment {
                document_id: document_id.to_owned(),
                quote: Some("brown fox".to_owned()),
                ..new_comment(None)
            };

            let comment = create_comment(&store, &site, input)
                .await
                .unwrap_or_else(|err| panic!("{document_id}: {err:?}"));

            assert_eq!(comment.selectors.len(), 2, "{document_id}");
        }
    }

    #[tokio::test]
    async fn quote_against_unreachable_storage_errors_render_failed() {
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let site = Site::new(
            Arc::new(MockStorage::new().with_scan_error(StorageErrorKind::Unavailable)),
            Arc::new(NullCache),
            PageRendererConfig::default(),
        );

        let input = NewComment {
            document_id: "domain:default/billing#payments".to_owned(),
            parent_id: None,
            author: None,
            body: "anchored".to_owned(),
            selectors: None,
            quote: Some("brown fox".to_owned()),
        };

        let err = create_comment(&store, &site, input).await.unwrap_err();

        assert!(
            matches!(
                err,
                CreateError::Quote(QuoteResolutionError::RenderFailed { .. })
            ),
            "expected RenderFailed, got {:?}",
            error_chain(&err),
        );
    }

    #[tokio::test]
    async fn quote_with_unknown_section_in_composite_key_errors_not_found() {
        // When the composite key names a section that isn't in the site,
        // `page_path_for_key` finds no page and create_comment surfaces a
        // `DocumentNotFound` rather than silently dropping the error.
        let store = SqliteCommentStore::open_memory().await.unwrap();
        let site = seeded_site("guide", "# Guide\n\nThe quick brown fox jumps.\n");

        let input = NewComment {
            document_id: "domain:default/unknown#guide".to_owned(),
            parent_id: None,
            author: None,
            body: "anchored".to_owned(),
            selectors: None,
            quote: Some("brown fox".to_owned()),
        };

        let err = create_comment(&store, &site, input).await.unwrap_err();

        assert!(
            matches!(
                err,
                CreateError::Quote(QuoteResolutionError::DocumentNotFound { .. })
            ),
            "expected DocumentNotFound, got {err:?}",
        );
    }
}
