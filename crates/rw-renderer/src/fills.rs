//! Owner-facing side of offset holes.
//!
//! Content that cannot be emitted during the walk reserves a *hole* — a
//! recorded offset in the output buffer — keyed by a value its owner chooses,
//! then supplies the content once the walk is complete. Two kinds of owner
//! reserve holes: a code-block processor returning
//! [`ProcessResult::Deferred`], which fills it through [`Fills`]; and the
//! walker's built-in tab bars ([`Source::Tabs`]), which the walker fills
//! directly.
//!
//! [`ProcessResult::Deferred`]: crate::ProcessResult::Deferred

use std::collections::HashMap;

/// Key identifying one reserved hole. Chosen by whoever reserves it, and
/// unique within that owner.
///
/// An owner's key is *local*: two owners may both pick `0`. The renderer
/// pairs it with the owner's identity to form the globally-unique key it
/// records the hole under, so keys never need to be coordinated across
/// owners.
pub type HoleKey = u32;

/// Which hole owner a fill belongs to, paired with its index where relevant.
///
/// Each code-block processor is a distinct owner (a processor at index 0 and
/// another at index 1 are two owners), so an index alone does not identify one.
/// Making that a type rather than packed arithmetic means disjointness is
/// checked by the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Source {
    CodeBlock(usize),
    /// The walker's built-in tabs rendering. A distinct variant (rather than
    /// reusing `CodeBlock`) keeps its hole keys disambiguated from every
    /// processor's by construction.
    Tabs,
}

/// Globally-unique key: a hole's owner paired with the owner's local key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlobalKey(pub(crate) Source, pub(crate) HoleKey);

/// Content for reserved holes, collected after the walk.
///
/// Keys are owner-local: [`set`](Fills::set) and [`get`](Fills::get) are
/// inverses. Each owner is collected from through its own `Fills`, and the
/// renderer keys those entries by the owner they came from when merging, so
/// a key an owner chooses can never collide with another owner's.
///
/// # Example
///
/// A [`CodeBlockProcessor`](crate::CodeBlockProcessor) that returns
/// [`ProcessResult::Deferred`](crate::ProcessResult::Deferred) supplies each
/// reserved hole's content here, keyed by the same code-block index it deferred
/// under:
///
/// ```
/// use rw_renderer::Fills;
///
/// let mut fills = Fills::new();
/// fills.set(0, "<p>hi</p>".to_owned());
/// assert_eq!(fills.get(0), Some("<p>hi</p>"));
/// ```
#[derive(Debug, Default)]
pub struct Fills {
    map: HashMap<HoleKey, String>,
}

impl Fills {
    /// Create an empty collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Supply the content for `key`. A later call for the same key replaces
    /// the earlier one.
    ///
    /// `key` is local to the calling owner: pick whatever numbering suits
    /// it, starting at `0`. Keys are scoped per owner, so two owners
    /// choosing the same key do not overwrite each other.
    pub fn set(&mut self, key: HoleKey, html: String) {
        self.map.insert(key, html);
    }

    /// Content for `key`, if it was supplied.
    #[must_use]
    pub fn get(&self, key: HoleKey) -> Option<&str> {
        self.map.get(&key).map(String::as_str)
    }
}

/// Every owner's fills, merged under their `Source`s.
///
/// Tracks the total byte length of the content it holds, so
/// [`Holes::assemble`](crate::holes::Holes::assemble) can size its output
/// buffer without a second pass over the entries.
#[derive(Debug, Default)]
pub(crate) struct GlobalFills {
    map: HashMap<GlobalKey, String>,
    total_len: usize,
}

impl GlobalFills {
    /// Merge one owner's [`Fills`] in, keying each entry under `source`.
    pub(crate) fn merge(&mut self, source: Source, fills: Fills) {
        for (local, html) in fills.map {
            self.total_len += html.len();
            if let Some(previous) = self.map.insert(GlobalKey(source, local), html) {
                self.total_len -= previous.len();
            }
        }
    }

    /// Insert one fill directly under its global key, without going through a
    /// per-owner [`Fills`]. Used by the walker's built-in tab rendering, which
    /// holds its content inline and reserves its holes under [`Source::Tabs`].
    /// Keeps the tracked `total_len` in step, mirroring
    /// [`merge`](Self::merge).
    pub(crate) fn insert(&mut self, key: GlobalKey, html: String) {
        self.total_len += html.len();
        if let Some(previous) = self.map.insert(key, html) {
            self.total_len -= previous.len();
        }
    }

    /// Content for `key`, if it was supplied.
    pub(crate) fn get(&self, key: GlobalKey) -> Option<&str> {
        self.map.get(&key).map(String::as_str)
    }

    /// Total byte length of every fill held, maintained as entries are merged.
    pub(crate) fn total_len(&self) -> usize {
        self.total_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_returns_set_value() {
        let mut fills = Fills::new();
        fills.set(7, "<div>".to_owned());
        assert_eq!(fills.get(7), Some("<div>"));
    }

    #[test]
    fn get_returns_none_for_unset_key() {
        let fills = Fills::new();
        assert_eq!(fills.get(7), None);
    }

    #[test]
    fn set_twice_keeps_last() {
        let mut fills = Fills::new();
        fills.set(1, "first".to_owned());
        fills.set(1, "second".to_owned());
        assert_eq!(fills.get(1), Some("second"));
    }

    #[test]
    fn merge_keys_entries_under_their_source() {
        let mut first = Fills::new();
        first.set(0, "a".to_owned());
        let mut second = Fills::new();
        second.set(0, "b".to_owned());

        let mut global = GlobalFills::default();
        global.merge(Source::CodeBlock(0), first);
        global.merge(Source::Tabs, second);

        // Same local key, different owners — no collision.
        assert_eq!(global.get(GlobalKey(Source::CodeBlock(0), 0)), Some("a"));
        assert_eq!(global.get(GlobalKey(Source::Tabs, 0)), Some("b"));
        assert_eq!(global.get(GlobalKey(Source::CodeBlock(1), 0)), None);
    }

    #[test]
    fn total_len_tracks_merged_content() {
        let mut fills = Fills::new();
        fills.set(0, "abc".to_owned());
        fills.set(1, "de".to_owned());

        let mut global = GlobalFills::default();
        global.merge(Source::CodeBlock(0), fills);

        assert_eq!(global.total_len(), 5);
    }

    #[test]
    fn total_len_discounts_replaced_content() {
        let mut global = GlobalFills::default();

        let mut first = Fills::new();
        first.set(0, "long fill".to_owned());
        global.merge(Source::CodeBlock(0), first);

        let mut replacement = Fills::new();
        replacement.set(0, "short".to_owned());
        global.merge(Source::CodeBlock(0), replacement);

        assert_eq!(
            global.get(GlobalKey(Source::CodeBlock(0), 0)),
            Some("short")
        );
        assert_eq!(global.total_len(), "short".len());
    }
}
