//! Offset holes: deferred content filled by position, never by scanning.
//!
//! A hole records the byte offset it was reserved at and writes nothing. After
//! the walk, [`Holes::assemble`] makes a single pass: copy the span of
//! untouched source, write the fill through the backend, repeat.
//!
//! Two kinds of owner reserve holes: the walker's built-in tab bars, which the
//! walker fills at each group's close once every label is known; and diagrams,
//! which [`RenderPass::finish`](crate::RenderPass::finish) fills once
//! the caller has resolved them.
//!
//! # Invariant
//!
//! From the first hole reservation until [`Holes::assemble`] runs, the walk
//! buffer is **append-only**. Offsets are byte positions into that buffer, so
//! appending is safe — it only extends the buffer, leaving every recorded
//! offset naming the same byte. Container closes that the walk emits at a block
//! or EOF boundary append for exactly this reason.
//!
//! Do **not** insert any step that rewrites the walk buffer before `assemble`:
//! an insertion, deletion, or replacement anywhere ahead of a recorded offset
//! shifts the bytes out from under it, and every later hole splices into the
//! wrong place. Appending is the only safe mutation. Assembly is deliberately
//! the sole post-walk transformation — keep it that way.

/// Byte offsets in the walk buffer where deferred content belongs, each with
/// its fill once the owner supplies it.
///
/// Entries are appended in document order, so they are sorted by construction.
#[derive(Debug, Default)]
pub(crate) struct Holes {
    entries: Vec<(usize, Option<String>)>,
}

impl Holes {
    /// Record a hole at `offset`, returning the slot its owner fills.
    ///
    /// Callers must pass a non-decreasing sequence of offsets — guaranteed by
    /// reserving at the current length of an append-only buffer.
    pub(crate) fn reserve(&mut self, offset: usize) -> usize {
        debug_assert!(
            self.entries.last().is_none_or(|(prev, _)| *prev <= offset),
            "holes must be reserved in document order: {offset} follows {:?}",
            self.entries.last()
        );
        self.entries.push((offset, None));
        self.entries.len() - 1
    }

    pub(crate) fn fill(&mut self, slot: usize, content: String) {
        self.entries[slot].1 = Some(content);
    }

    /// Build the final document: copy spans of `source`, writing each fill at
    /// its reserved offset through `write_fill`.
    ///
    /// A fill is markup the backend never saw during the walk, so it must reach
    /// the buffer the same way every other emission does — `write_fill` is the
    /// backend's `raw_html`. A backend that drops markup (the search-document
    /// one) therefore drops fills too.
    ///
    /// The initial allocation is sized from the raw fill lengths, which is an
    /// estimate: `write_fill` decides what actually lands in the buffer, so it
    /// may write more (escaping) or nothing at all.
    ///
    /// With no holes, `source` is moved through untouched: no output buffer is
    /// allocated and no bytes are copied.
    ///
    /// # Contract: every reserved hole must be filled
    ///
    /// Whoever reserves a hole is responsible for filling it before this runs.
    /// An unfilled hole is an internal renderer bug, not a recoverable
    /// condition: in debug builds it trips `debug_assert!` below; in release it
    /// silently writes nothing for that hole, so the deferred content just
    /// vanishes from the page with no warning and no visible marker. This is
    /// deliberate: `RenderResult::warnings` is a user-facing channel that gates
    /// `--strict` publishes, and a missed fill is a bug in `rw`'s own code, not
    /// something a document author did wrong.
    pub(crate) fn assemble(self, source: String, write_fill: impl Fn(&str, &mut String)) -> String {
        if self.entries.is_empty() {
            return source;
        }

        let fills: usize = self
            .entries
            .iter()
            .flat_map(|(_, fill)| fill)
            .map(String::len)
            .sum();
        let mut out = String::with_capacity(source.len() + fills);

        let mut pos = 0;
        for (slot, (offset, fill)) in self.entries.iter().enumerate() {
            out.push_str(&source[pos..*offset]);
            if let Some(fill) = fill {
                write_fill(fill, &mut out);
            } else {
                debug_assert!(false, "hole {slot} was reserved but never filled");
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

    /// Reserve a hole at each offset and fill it.
    fn holes(entries: &[(usize, &str)]) -> Holes {
        let mut holes = Holes::default();
        for &(offset, fill) in entries {
            let slot = holes.reserve(offset);
            holes.fill(slot, fill.to_owned());
        }
        holes
    }

    /// Stand-in for a pass-through backend's `raw_html`.
    fn passthrough(fill: &str, out: &mut String) {
        out.push_str(fill);
    }

    #[test]
    fn no_holes_returns_source_unchanged() {
        let out = holes(&[]).assemble("<p>hi</p>".to_owned(), passthrough);
        assert_eq!(out, "<p>hi</p>");
    }

    #[test]
    fn single_hole_splices_at_offset() {
        let out = holes(&[(3, "MID")]).assemble("abcdef".to_owned(), passthrough);
        assert_eq!(out, "abcMIDdef");
    }

    #[test]
    fn hole_at_start_and_end() {
        let out = holes(&[(0, "<"), (3, ">")]).assemble("abc".to_owned(), passthrough);
        assert_eq!(out, "<abc>");
    }

    #[test]
    fn two_holes_at_same_offset_keep_reservation_order() {
        let out = holes(&[(2, "["), (2, "]")]).assemble("ab".to_owned(), passthrough);
        assert_eq!(out, "ab[]");
    }

    #[test]
    fn fills_go_through_the_writer() {
        // A backend that drops markup (SearchDocumentBackend) drops fills too.
        let dropped = holes(&[(2, "<b>")]).assemble("ab".to_owned(), |_, _| {});
        assert_eq!(dropped, "ab");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "was reserved but never filled")]
    fn unfilled_hole_panics_in_debug() {
        let mut holes = holes(&[(0, "filled")]);
        holes.reserve(1);
        let _ = holes.assemble("ab".to_owned(), passthrough);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "holes must be reserved in document order")]
    fn out_of_order_reservation_panics_in_debug() {
        let mut holes = Holes::default();
        holes.reserve(5);
        holes.reserve(2);
    }
}
