//! Handler-facing side of offset holes.
//!
//! A directive or code-block processor that cannot emit its final content
//! during the walk reserves a hole keyed by a value it chooses — a
//! [`Hole`](Part::Hole) part for a directive, [`ProcessResult::Deferred`] for a
//! code-block processor — then supplies the content through [`Fills`] once the
//! walk is complete.
//!
//! [`ProcessResult::Deferred`]: crate::ProcessResult::Deferred

use std::borrow::Cow;
use std::collections::HashMap;

/// Key identifying one reserved hole. Chosen by whoever reserves it, and
/// unique within that handler.
///
/// A handler's key is *local*: two handlers may both pick `0`. The renderer
/// pairs it with the handler's identity to form the globally-unique key it
/// records the hole under, so keys never need to be coordinated across
/// handlers.
pub type HoleKey = u32;

/// Which registry a hole's owner lives in, paired with its index there.
///
/// Handlers are registered in separate `Vec`s, so an index alone does not
/// identify one — a leaf at index 0, a container at index 0, and a code-block
/// processor at index 0 are three different owners. Making that a type rather
/// than packed arithmetic means disjointness is checked by the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Source {
    Leaf(usize),
    Container(usize),
    CodeBlock(usize),
}

/// Globally-unique key: a hole's owner paired with the owner's local key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlobalKey(pub(crate) Source, pub(crate) HoleKey);

/// One piece of a deferred directive's output, in document order.
#[derive(Debug, PartialEq, Eq)]
pub enum Part {
    /// Literal HTML, emitted during the walk. Borrowed for the common case of a
    /// static closing tag, so a constant piece costs no allocation.
    Html(Cow<'static, str>),
    /// A gap to be filled after the walk, identified by `HoleKey`.
    Hole(HoleKey),
}

/// Content for reserved holes, collected after the walk.
///
/// Keys are handler-local: [`set`](Fills::set) and [`get`](Fills::get) are
/// inverses. Each handler is collected from through its own `Fills`, and the
/// renderer keys those entries by the handler they came from when merging, so
/// a key a handler chooses can never collide with another handler's.
///
/// # Example
///
/// Both halves of the contract: `process` reserves hole `0` during the walk,
/// and `fills` supplies its content afterwards under the same key. Here the
/// content is a count the handler cannot know until every invocation has been
/// seen — the reason to defer at all.
///
/// ```
/// use rw_renderer::directive::{
///     DirectiveArgs, DirectiveContext, DirectiveOutput, DirectiveProcessor, Fills,
///     LeafDirective, Part,
/// };
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
///
/// #[derive(Default)]
/// struct CountDirective {
///     seen: usize,
/// }
///
/// impl LeafDirective for CountDirective {
///     fn name(&self) -> &str { "count" }
///
///     fn process(&mut self, _args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
///         self.seen += 1;
///         // Writes nothing now — just records where the fill belongs.
///         DirectiveOutput::deferred(vec![Part::Hole(0)])
///     }
///
///     fn fills(&mut self, fills: &mut Fills) {
///         // Runs after the walk, so the total is known.
///         fills.set(0, format!("<p>{} directives</p>", self.seen));
///     }
/// }
///
/// let directives = DirectiveProcessor::new().with_leaf(CountDirective::default());
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .render("::count\n\n::count\n", Pipeline::new().with_directives(directives));
///
/// // Every hole is filled with the final count, even the first one.
/// assert_eq!(result.html.matches("<p>2 directives</p>").count(), 2);
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
    /// `key` is local to the calling handler: pick whatever numbering suits
    /// it, starting at `0`. Keys are scoped per handler, so two handlers
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

/// Every handler's fills, merged under their `Source`s.
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
        global.merge(Source::Leaf(0), first);
        global.merge(Source::Container(0), second);

        // Same local key, different handlers — no collision.
        assert_eq!(global.get(GlobalKey(Source::Leaf(0), 0)), Some("a"));
        assert_eq!(global.get(GlobalKey(Source::Container(0), 0)), Some("b"));
        assert_eq!(global.get(GlobalKey(Source::Leaf(1), 0)), None);
    }

    #[test]
    fn total_len_tracks_merged_content() {
        let mut fills = Fills::new();
        fills.set(0, "abc".to_owned());
        fills.set(1, "de".to_owned());

        let mut global = GlobalFills::default();
        global.merge(Source::Leaf(0), fills);

        assert_eq!(global.total_len(), 5);
    }

    #[test]
    fn total_len_discounts_replaced_content() {
        let mut global = GlobalFills::default();

        let mut first = Fills::new();
        first.set(0, "long fill".to_owned());
        global.merge(Source::Leaf(0), first);

        let mut replacement = Fills::new();
        replacement.set(0, "short".to_owned());
        global.merge(Source::Leaf(0), replacement);

        assert_eq!(global.get(GlobalKey(Source::Leaf(0), 0)), Some("short"));
        assert_eq!(global.total_len(), "short".len());
    }
}
