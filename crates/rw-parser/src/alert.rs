//! GitHub-style alert kinds (`> [!NOTE]`, `> [!TIP]`, …).
//!
//! A blockquote's alert marker is syntax, so it is recognized while the
//! markdown is tokenized and reported on the blockquote's opening tag. What
//! the marker then *looks* like is the consumer's decision.

use pulldown_cmark::BlockQuoteKind;

/// Alert variant for GitHub-style blockquotes (`> [!NOTE]`, `> [!TIP]`, etc.).
///
/// Reported on [`Tag::BlockQuote`](crate::Tag::BlockQuote), converted from
/// [`pulldown_cmark::BlockQuoteKind`]. A blockquote without an alert marker
/// carries `None`.
///
/// # Examples
///
/// ```
/// use rw_parser::{AlertKind, Event, Parser, Tag};
///
/// let mut parser = Parser::new("> [!WARNING]\n> Do not delete this file.", false, false);
/// assert_eq!(
///     parser.next(),
///     Some(Event::Start(Tag::BlockQuote(Some(AlertKind::Warning)))),
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// Informational note — highlights something the reader should be aware of.
    Note,
    /// Helpful advice — suggests a better approach or useful trick.
    Tip,
    /// Critical information — something the reader must not overlook.
    Important,
    /// Potential issue — something that could go wrong.
    Warning,
    /// Dangerous action — something that could cause data loss or security issues.
    Caution,
}

impl From<BlockQuoteKind> for AlertKind {
    fn from(kind: BlockQuoteKind) -> Self {
        match kind {
            BlockQuoteKind::Note => AlertKind::Note,
            BlockQuoteKind::Tip => AlertKind::Tip,
            BlockQuoteKind::Important => AlertKind::Important,
            BlockQuoteKind::Warning => AlertKind::Warning,
            BlockQuoteKind::Caution => AlertKind::Caution,
        }
    }
}
