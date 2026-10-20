use rw_site::Site;

use crate::anchoring::{QuoteResolutionError, resolve_quote};
use crate::error::CreateError;
use crate::model::{CreateComment, NewComment};
use crate::sqlite::SqliteCommentStore;
use crate::{Comment, Selector};

/// Decode a comment storage key to the page path to anchor a quote against.
///
/// The viewer and CLI store comments under the composite `"{sectionRef}#{subpath}"`
/// key; split it and resolve the section ref to its URL path.
///
/// Section names and subpaths may both contain `#`, so every `#` is tried as
/// the separator, longest section ref first, as keys name the deepest
/// enclosing section.
fn page_path_for_key(site: &Site, document_id: &str) -> Result<String, QuoteResolutionError> {
    for (i, _) in document_id.rmatch_indices('#') {
        let (section_ref, subpath) = (&document_id[..i], &document_id[i + 1..]);
        let page_path = site
            .try_page_path_for(section_ref, subpath)
            .map_err(|source| QuoteResolutionError::RenderFailed {
                document_id: document_id.to_owned(),
                source,
            })?;
        if let Some(page_path) = page_path {
            return Ok(page_path);
        }
    }
    Err(QuoteResolutionError::DocumentNotFound {
        document_id: document_id.to_owned(),
    })
}

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
        (Some(quote), _) => {
            let page_path = page_path_for_key(site, document_id)?;
            Ok(resolve_quote(site, &page_path, &quote)?)
        }
        (None, Some(s)) => Ok(s),
        (None, None) => Ok(Vec::new()),
    }
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
    use crate::model::{Author, CommentFilter};

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
