//! Owner-facing side of offset holes.
//!
//! Content that cannot be emitted during the walk reserves a *hole* — a
//! recorded offset in the output buffer — keyed by a value its owner chooses,
//! then supplies the content once the walk is complete. Two kinds of owner
//! reserve holes: the walker's built-in tab bars ([`Source::Tabs`]), which the
//! walker fills at [`pause`](crate::walker::Walker::pause) once every label is
//! known; and diagram fences ([`Source::Diagram`]), which
//! [`RenderPass::finish`] fills once the caller has resolved them.
//!
//! [`RenderPass::finish`]: crate::RenderPass::finish

use std::collections::HashMap;

/// Key identifying one reserved hole. Chosen by whoever reserves it, and
/// unique within that owner.
///
/// An owner's key is *local*: two owners may both pick `0`. The renderer
/// pairs it with the owner's identity to form the globally-unique key it
/// records the hole under, so keys never need to be coordinated across
/// owners.
pub(crate) type HoleKey = u32;

/// Which hole owner a fill belongs to.
///
/// Making that a type rather than packed arithmetic means disjointness is
/// checked by the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Source {
    /// The walker's built-in tabs rendering. Its hole keys count tab groups,
    /// which is a numbering of its own.
    Tabs,
    /// A diagram fence, filled by
    /// [`RenderPass::finish`](crate::RenderPass::finish) from the caller's
    /// resolutions. A diagram's local key is its code-block index, which shares
    /// nothing with the tab numbering — hence two variants rather than one
    /// shared key space.
    Diagram,
}

/// Globally-unique key: a hole's owner paired with the owner's local key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlobalKey(pub(crate) Source, pub(crate) HoleKey);

/// Every owner's fills, keyed under their `Source`s.
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
    /// Supply the content for `key`. A later call for the same key replaces the
    /// earlier one, keeping the tracked `total_len` in step.
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

    /// Total byte length of every fill held, maintained as entries are inserted.
    pub(crate) fn total_len(&self) -> usize {
        self.total_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_returns_inserted_value() {
        let mut fills = GlobalFills::default();
        fills.insert(GlobalKey(Source::Diagram, 7), "<div>".to_owned());
        assert_eq!(fills.get(GlobalKey(Source::Diagram, 7)), Some("<div>"));
    }

    #[test]
    fn get_returns_none_for_an_unset_key() {
        let fills = GlobalFills::default();
        assert_eq!(fills.get(GlobalKey(Source::Diagram, 7)), None);
    }

    #[test]
    fn a_key_is_scoped_to_its_source() {
        let mut fills = GlobalFills::default();
        fills.insert(GlobalKey(Source::Diagram, 0), "a".to_owned());
        fills.insert(GlobalKey(Source::Tabs, 0), "b".to_owned());

        // Same local key, different owners — no collision. A tab group's bar
        // hole and the first code block's diagram hole are both keyed `0`.
        assert_eq!(fills.get(GlobalKey(Source::Diagram, 0)), Some("a"));
        assert_eq!(fills.get(GlobalKey(Source::Tabs, 0)), Some("b"));
    }

    #[test]
    fn total_len_tracks_inserted_content() {
        let mut fills = GlobalFills::default();
        fills.insert(GlobalKey(Source::Diagram, 0), "abc".to_owned());
        fills.insert(GlobalKey(Source::Tabs, 1), "de".to_owned());

        assert_eq!(fills.total_len(), 5);
    }

    #[test]
    fn total_len_discounts_replaced_content() {
        let mut fills = GlobalFills::default();
        fills.insert(GlobalKey(Source::Diagram, 0), "long fill".to_owned());
        fills.insert(GlobalKey(Source::Diagram, 0), "short".to_owned());

        assert_eq!(fills.get(GlobalKey(Source::Diagram, 0)), Some("short"));
        assert_eq!(fills.total_len(), "short".len());
    }
}
