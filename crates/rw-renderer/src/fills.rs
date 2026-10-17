//! Owner-facing side of offset holes.
//!
//! Content that cannot be emitted during the walk reserves a *hole* — a
//! recorded offset in the output buffer — keyed by a value its owner chooses,
//! then supplies the content once it is known. Two kinds of owner reserve
//! holes: the walker's built-in tab bars ([`Source::Tabs`]), which the walker
//! fills at each group's close once every label is known; and diagram fences
//! ([`Source::Diagram`]), which [`RenderPass::finish`] fills once the caller
//! has resolved them.
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

/// Narrow a document-order index (a code-block index, a tab-group id) to the
/// hole-key width. The one conversion policy for every hole owner: fail loud
/// rather than wrap into a colliding key.
pub(crate) fn hole_key(index: usize) -> HoleKey {
    HoleKey::try_from(index).unwrap_or_else(|_| panic!("hole index {index} exceeds hole key width"))
}

/// Every owner's fills, keyed under their `Source`s.
pub(crate) type GlobalFills = HashMap<GlobalKey, String>;
