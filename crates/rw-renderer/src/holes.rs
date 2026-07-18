//! Offset holes: deferred content filled by position, never by scanning.
//!
//! A hole records the byte offset it was reserved at and writes nothing. After
//! the walk, [`Holes::assemble`] makes a single pass: copy the span of
//! untouched source, write the fill through the backend, repeat.
//!
//! # Invariant
//!
//! From the first hole reservation until [`Holes::assemble`] runs, the walk
//! buffer is **append-only**. Offsets are byte positions into that buffer, so
//! appending is safe — it only extends the buffer, leaving every recorded
//! offset naming the same byte. `close_unclosed_containers` appends after the
//! walk for exactly this reason.
//!
//! Do **not** insert any step that rewrites the walk buffer before `assemble`:
//! an insertion, deletion, or replacement anywhere ahead of a recorded offset
//! shifts the bytes out from under it, and every later hole splices into the
//! wrong place. `CodeBlockProcessor::post_process` rewrites the buffer
//! (sentinel replacement), which is why it runs *after* assembly.

use crate::directive::fills::{GlobalFills, GlobalKey};

/// Byte offsets in the walk buffer where deferred content belongs.
///
/// Entries are appended in document order, so they are sorted by construction.
#[derive(Debug, Default)]
pub(crate) struct Holes {
    entries: Vec<(usize, GlobalKey)>,
}

impl Holes {
    /// Record a hole at `offset`, to be filled by `key`.
    ///
    /// Callers must pass a non-decreasing sequence of offsets — guaranteed by
    /// reserving at the current length of an append-only buffer.
    pub(crate) fn reserve(&mut self, offset: usize, key: GlobalKey) {
        debug_assert!(
            self.entries.last().is_none_or(|(prev, _)| *prev <= offset),
            "holes must be reserved in document order: {offset} follows {:?}",
            self.entries.last()
        );
        self.entries.push((offset, key));
    }

    /// Whether any hole was reserved.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Build the final document: copy spans of `source`, writing each fill at
    /// its reserved offset through `write_fill`.
    ///
    /// A fill is markup the backend never saw during the walk, so it must reach
    /// the buffer the same way every other emission does — `write_fill` is the
    /// backend's `raw_html`. A backend that drops markup (the search-document
    /// one) therefore drops fills too.
    ///
    /// The initial allocation is sized from the raw fill lengths `fills` has
    /// tracked, which is an estimate: `write_fill` decides what actually lands
    /// in the buffer, so it may write more (escaping) or nothing at all.
    ///
    /// With no holes, `source` is returned untouched — no copy at all. This is
    /// what keeps a page with no deferred content free.
    pub(crate) fn assemble(
        self,
        source: String,
        fills: &GlobalFills,
        write_fill: impl Fn(&str, &mut String),
    ) -> String {
        if self.entries.is_empty() {
            return source;
        }

        let mut out = String::with_capacity(source.len() + fills.total_len());

        let mut pos = 0;
        for (offset, key) in &self.entries {
            out.push_str(&source[pos..*offset]);
            if let Some(fill) = fills.get(*key) {
                write_fill(fill, &mut out);
            } else {
                debug_assert!(false, "hole {key:?} was reserved but never filled");
            }
            pos = *offset;
        }
        out.push_str(&source[pos..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directive::{Fills, HoleKey};

    /// Namespace the test fills are collected under. Any single value works —
    /// namespacing is the processor's concern, not `Holes`'.
    const NS: u32 = 1;

    /// Build the merged fills for `entries`, as the processor would.
    fn fills(entries: &[(HoleKey, &str)]) -> GlobalFills {
        let mut local = Fills::new();
        for (key, html) in entries {
            local.set(*key, (*html).to_owned());
        }
        let mut global = GlobalFills::default();
        global.merge(NS, local);
        global
    }

    /// Stand-in for a pass-through backend's `raw_html`.
    fn passthrough(fill: &str, out: &mut String) {
        out.push_str(fill);
    }

    #[test]
    fn no_holes_returns_source_unchanged() {
        let holes = Holes::default();
        assert!(holes.is_empty());
        let out = holes.assemble("<p>hi</p>".to_owned(), &fills(&[]), passthrough);
        assert_eq!(out, "<p>hi</p>");
    }

    #[test]
    fn single_hole_splices_at_offset() {
        let mut holes = Holes::default();
        holes.reserve(3, GlobalKey(NS, 1));
        assert_eq!(
            holes.assemble("abcdef".to_owned(), &fills(&[(1, "MID")]), passthrough),
            "abcMIDdef"
        );
    }

    #[test]
    fn hole_at_start_and_end() {
        let mut holes = Holes::default();
        holes.reserve(0, GlobalKey(NS, 1));
        holes.reserve(3, GlobalKey(NS, 2));
        assert_eq!(
            holes.assemble("abc".to_owned(), &fills(&[(1, "<"), (2, ">")]), passthrough),
            "<abc>"
        );
    }

    #[test]
    fn two_holes_at_same_offset_keep_reservation_order() {
        let mut holes = Holes::default();
        holes.reserve(2, GlobalKey(NS, 1));
        holes.reserve(2, GlobalKey(NS, 2));
        assert_eq!(
            holes.assemble("ab".to_owned(), &fills(&[(1, "["), (2, "]")]), passthrough),
            "ab[]"
        );
    }

    #[test]
    fn holes_in_different_namespaces_do_not_collide() {
        let mut holes = Holes::default();
        holes.reserve(1, GlobalKey(1, 0));
        holes.reserve(1, GlobalKey(2, 0));

        let mut first = Fills::new();
        first.set(0, "[one]".to_owned());
        let mut second = Fills::new();
        second.set(0, "[two]".to_owned());
        let mut global = GlobalFills::default();
        global.merge(1, first);
        global.merge(2, second);

        assert_eq!(
            holes.assemble("ab".to_owned(), &global, passthrough),
            "a[one][two]b"
        );
    }

    #[test]
    fn fills_go_through_the_writer() {
        let mut holes = Holes::default();
        holes.reserve(2, GlobalKey(NS, 1));
        // A backend that drops markup (SearchDocumentBackend) drops fills too.
        let dropped = holes.assemble("ab".to_owned(), &fills(&[(1, "<b>")]), |_, _| {});
        assert_eq!(dropped, "ab");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "was reserved but never filled")]
    fn unfilled_hole_panics_in_debug() {
        let mut holes = Holes::default();
        holes.reserve(1, GlobalKey(NS, 99));
        // Key 1 is filled; key 99 — the one actually reserved — is not.
        let _ = holes.assemble("ab".to_owned(), &fills(&[(1, "unused")]), passthrough);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "holes must be reserved in document order")]
    fn out_of_order_reservation_panics_in_debug() {
        let mut holes = Holes::default();
        holes.reserve(5, GlobalKey(NS, 1));
        holes.reserve(2, GlobalKey(NS, 2));
    }
}
