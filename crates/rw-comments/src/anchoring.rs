//! Resolve a `quote` string to a pair of [`Selector`]s by rendering the page
//! and searching its textContent-equivalent output. Mirrors the viewer's
//! browser-side anchoring so offsets line up with what the DOM sees.

use rw_site::{RenderError, Site};
use thiserror::Error;

use crate::Selector;
use crate::html_text::html_to_text_content;

/// Errors returned when resolving a quote into selectors.
#[derive(Debug, Error)]
pub enum QuoteResolutionError {
    /// The requested document does not exist in the site.
    #[error("document '{document_id}' does not exist")]
    DocumentNotFound { document_id: String },
    /// The quote did not appear in the rendered text.
    #[error("quote not found in document '{document_id}'")]
    NotFound { document_id: String },
    /// The quote appeared more than once; caller must add surrounding context.
    #[error(
        "quote matches {count} times in document '{document_id}' — add more surrounding context to disambiguate"
    )]
    Ambiguous { document_id: String, count: usize },
    /// Page rendering failed for some other reason.
    #[error("failed to render document '{document_id}': {reason}")]
    RenderFailed { document_id: String, reason: String },
}

/// Characters of prefix / suffix context stored alongside the quote. Matches
/// the viewer's `rangeToSelectors` (32 chars on either side).
const CONTEXT_CHARS: usize = 32;

/// Resolve a quote string into a pair of selectors by rendering the page's
/// textContent and locating the quote inside it.
///
/// Returns a two-element vector: a [`Selector::TextQuoteSelector`] with
/// `exact`, `prefix` (up to 32 chars), and `suffix` (up to 32 chars), plus a
/// [`Selector::TextPositionSelector`] whose offsets are UTF-16 code units —
/// the same representation the viewer produces from `Range.toString().length`.
///
/// # Errors
///
/// Returns [`QuoteResolutionError`] when the document is missing, the quote
/// does not match, matches more than once, or rendering fails.
pub(crate) fn resolve_quote(
    site: &Site,
    document_id: &str,
    quote: &str,
) -> Result<Vec<Selector>, QuoteResolutionError> {
    let result = site.render(document_id).map_err(|err| match err {
        RenderError::PageNotFound(_) | RenderError::FileNotFound(_) => {
            QuoteResolutionError::DocumentNotFound {
                document_id: document_id.to_owned(),
            }
        }
        other => QuoteResolutionError::RenderFailed {
            document_id: document_id.to_owned(),
            reason: other.to_string(),
        },
    })?;

    let text = html_to_text_content(&result.html);

    let mut occurrences = text.match_indices(quote);
    let byte_start = occurrences
        .next()
        .ok_or_else(|| QuoteResolutionError::NotFound {
            document_id: document_id.to_owned(),
        })?
        .0;
    if occurrences.next().is_some() {
        return Err(QuoteResolutionError::Ambiguous {
            document_id: document_id.to_owned(),
            count: 2 + occurrences.count(),
        });
    }

    let byte_end = byte_start + quote.len();

    let start = text[..byte_start].encode_utf16().count();
    let end = start + quote.encode_utf16().count();

    let prefix_start = text[..byte_start]
        .char_indices()
        .nth_back(CONTEXT_CHARS - 1)
        .map_or(0, |(idx, _)| idx);
    let prefix = text[prefix_start..byte_start].to_owned();
    let suffix: String = text[byte_end..].chars().take(CONTEXT_CHARS).collect();

    Ok(vec![
        Selector::TextQuoteSelector {
            exact: quote.to_owned(),
            prefix,
            suffix,
        },
        Selector::TextPositionSelector { start, end },
    ])
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pretty_assertions::assert_eq;
    use rw_cache::NullCache;
    use rw_site::PageRendererConfig;
    use rw_storage::MockStorage;

    use super::*;

    fn seeded_site(path: &str, markdown: &str) -> Site {
        let storage = Arc::new(
            MockStorage::new()
                .with_file(path, path, markdown)
                .with_mtime(path, 0.0),
        );
        Site::new(storage, Arc::new(NullCache), PageRendererConfig::default())
    }

    #[test]
    fn single_match_returns_both_selectors() {
        let site = seeded_site(
            "guide",
            "# Guide\n\nThe quick brown fox jumps over the lazy dog.\n",
        );

        let selectors = resolve_quote(&site, "guide", "brown fox jumps").unwrap();
        assert_eq!(selectors.len(), 2);

        match &selectors[0] {
            Selector::TextQuoteSelector {
                exact,
                prefix,
                suffix,
            } => {
                assert_eq!(exact, "brown fox jumps");
                assert!(prefix.ends_with("quick "));
                assert!(suffix.starts_with(" over"));
            }
            other => panic!("expected TextQuoteSelector, got {other:?}"),
        }

        match &selectors[1] {
            Selector::TextPositionSelector { start, end } => {
                assert!(end > start);
                assert_eq!(end - start, "brown fox jumps".len());
            }
            other => panic!("expected TextPositionSelector, got {other:?}"),
        }
    }

    #[test]
    fn not_found_returns_error() {
        let site = seeded_site("guide", "# Guide\n\nSome body text.\n");

        let err = resolve_quote(&site, "guide", "missing quote").unwrap_err();
        assert!(matches!(err, QuoteResolutionError::NotFound { .. }));
    }

    #[test]
    fn ambiguous_quote_returns_count() {
        let site = seeded_site("guide", "# Guide\n\nfoo bar foo baz foo.\n");

        let err = resolve_quote(&site, "guide", "foo").unwrap_err();
        match err {
            QuoteResolutionError::Ambiguous { count, .. } => assert_eq!(count, 3),
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn missing_document_returns_document_not_found() {
        let site = seeded_site("guide", "# Guide\n");

        let err = resolve_quote(&site, "nope", "anything").unwrap_err();
        assert!(matches!(err, QuoteResolutionError::DocumentNotFound { .. }));
    }

    #[test]
    fn match_at_start_has_empty_prefix() {
        let site = seeded_site("guide", "Hello world, this is the body.\n");

        let selectors = resolve_quote(&site, "guide", "Hello").unwrap();
        match &selectors[0] {
            Selector::TextQuoteSelector { prefix, .. } => {
                assert!(prefix.is_empty(), "expected empty prefix, got {prefix:?}");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn markdown_syntax_in_quote_not_found_in_rendered_text() {
        // Quote includes raw markdown `**bold**` that gets rendered to
        // `<strong>bold</strong>` — textContent is just `bold`, so the
        // literal `**bold**` quote should not match.
        let site = seeded_site("guide", "This is **bold** text.\n");

        let err = resolve_quote(&site, "guide", "**bold**").unwrap_err();
        assert!(matches!(err, QuoteResolutionError::NotFound { .. }));
    }

    #[test]
    fn non_ascii_page_uses_utf16_code_units() {
        // `TextPositionSelector` offsets are UTF-16 code units so the viewer
        // (which computes positions via `String.length`) can resolve them
        // directly.
        let site = seeded_site("guide", "Привет, world! More text here.\n");

        let selectors = resolve_quote(&site, "guide", "world").unwrap();
        match &selectors[1] {
            Selector::TextPositionSelector { start, end } => {
                // "Привет, " is 8 UTF-16 code units (6 BMP Cyrillic chars
                // + ", ").
                assert_eq!(*start, 8);
                assert_eq!(*end, 13); // 8 + "world".len()
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn non_bmp_char_counts_as_two_utf16_units() {
        // Surrogate pairs (emoji, etc.) occupy 2 UTF-16 code units — matches
        // what `String.prototype.length` returns in the browser.
        let site = seeded_site("guide", "🚀 hello world here.\n");

        let selectors = resolve_quote(&site, "guide", "world").unwrap();
        match &selectors[1] {
            Selector::TextPositionSelector { start, end } => {
                // "🚀 hello " = 2 (rocket surrogate pair) + 1 + 5 + 1 = 9.
                assert_eq!(*start, 9);
                assert_eq!(*end, 14); // 9 + "world".len()
            }
            _ => unreachable!(),
        }
    }
}
