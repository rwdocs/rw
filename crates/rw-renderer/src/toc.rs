//! Table of contents output type and heading accumulator.
//!
//! [`TocEntry`] is the public output type collected in
//! [`RenderResult::toc`](crate::RenderResult::toc).
//! [`HeadingAccumulator`] is walker-private scratch that tracks
//! cross-heading state (title, TOC entries, id de-duplication state, and the
//! "have we seen the first H1?" flag) across an entire document render.

use std::collections::HashMap;

use crate::util::slugify_into;

/// A single heading in the table of contents.
///
/// Produced by [`MarkdownRenderer`](crate::MarkdownRenderer) for every heading
/// in the document (excluding the page title when
/// [`with_title_extraction`](crate::MarkdownRenderer::with_title_extraction) is enabled).
/// Collected in [`RenderResult::toc`](crate::RenderResult::toc).
///
/// The `id` field matches the `id` attribute on the rendered `<h*>` element,
/// so frontends can build clickable heading links with `#{id}` fragments.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer, Pipeline};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .render("# Page Title\n\n## Introduction\n\n## Setup", Pipeline::new());
///
/// assert_eq!(result.toc.len(), 2);
/// assert_eq!(result.toc[0].title, "Introduction");
/// assert_eq!(result.toc[0].id, "introduction");
/// assert_eq!(result.toc[0].level, 2);
/// ```
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TocEntry {
    /// Heading level (1–6), adjusted for the backend when
    /// [`TITLE_AS_METADATA`](crate::RenderBackend::TITLE_AS_METADATA) is `true`
    /// (headings shift up by one after the title H1).
    pub level: u8,
    /// Plain-text heading content (inline formatting stripped).
    pub title: String,
    /// Slug-based anchor ID, matching the `id` attribute on the rendered heading.
    /// Guaranteed unique within the document: duplicate or colliding headings get
    /// a numeric suffix (e.g., `setup`, `setup-1`), and headings with no slug
    /// characters fall back to `section`.
    pub id: String,
}

/// Result of completing a heading via `HeadingAccumulator::complete_heading`.
/// Returned by-value so the caller can emit `B::heading_start` / push `html` /
/// `B::heading_end` without holding any borrow on the accumulator.
#[derive(Debug)]
pub(crate) struct CompletedHeading {
    /// Heading level after Confluence's title-shift adjustment.
    pub adjusted_level: u8,
    /// Slug-based anchor id, deduped to be unique within the document via the
    /// accumulator's `claimed_ids`.
    pub id: String,
    /// Backend-formatted HTML body, ready to splice into `output` between
    /// the heading's open and close tags. Encoding/escaping is whatever the
    /// active `RenderBackend` produced during the inline phase.
    pub rendered_html: String,
}

/// Persistent accumulator for heading-related output across an entire
/// document render. Holds cross-heading state (title, TOC entries,
/// claimed ids, the "have we seen the first H1?" flag, and the config
/// flags that govern title extraction).
///
/// Per-heading state (the heading's level, plain-text shadow, formatted
/// HTML body, and the `in_first_h1` flag) lives in
/// [`Scope::Heading`](crate::scope::Scope) — *not* here. The
/// accumulator is consulted at `Tag::Heading` start (to decide whether
/// the heading is the skipped Confluence first H1) and at
/// `TagEnd::Heading` (to capture the title and/or emit the heading).
pub(crate) struct HeadingAccumulator {
    /// Whether to extract title from first H1.
    extract_title: bool,
    /// Whether to skip first H1 in output (Confluence mode).
    title_as_metadata: bool,
    /// Extracted title from first H1.
    title: Option<String>,
    /// Whether we've seen the first H1.
    seen_first_h1: bool,
    /// Table of contents entries.
    toc: Vec<TocEntry>,
    /// Every string already claimed as an id in this render, mapped to the
    /// next suffix to try when that string comes up again as a base slug.
    ///
    /// One map rather than a separate id-set and counter-map: a key's presence
    /// *is* the "already used" answer, so a heading costs one lookup instead of
    /// two, and a colliding `{base}-{n}` is caught by the same table that
    /// hands out the counter.
    claimed_ids: HashMap<String, usize>,
    /// Reused slug buffer, so `generate_id` doesn't allocate a `String` per
    /// heading just to hash it.
    slug_scratch: String,
}

impl HeadingAccumulator {
    /// Create a new accumulator.
    ///
    /// * `extract_title` — whether to extract title from first H1
    /// * `title_as_metadata` — whether to skip first H1 in output (Confluence mode)
    pub fn new(extract_title: bool, title_as_metadata: bool) -> Self {
        Self {
            extract_title,
            title_as_metadata,
            title: None,
            seen_first_h1: false,
            toc: Vec::new(),
            claimed_ids: HashMap::new(),
            slug_scratch: String::new(),
        }
    }

    /// Whether `level` is the Confluence-mode first H1 that should be
    /// title-extracted and skipped from output. Consulted at
    /// `Tag::Heading` start time to set `Scope::Heading::in_first_h1`.
    pub fn is_skipped_title(&self, level: u8) -> bool {
        self.extract_title && self.title_as_metadata && level == 1 && !self.seen_first_h1
    }

    /// Confluence-mode skipped first H1: capture `toc_text` as the page
    /// title and mark `seen_first_h1`. The matching `Scope::Heading`'s
    /// rendered HTML must be discarded by the caller — this function
    /// emits nothing to `output`.
    pub fn complete_first_h1(&mut self, toc_text: &str) {
        self.title = Some(toc_text.trim().to_owned());
        self.seen_first_h1 = true;
    }

    /// Complete a non-skipped heading: generate the id, capture the title
    /// (HTML-mode first H1 only), push a TOC entry (unless this *is* the
    /// title), and return the data the caller needs to emit
    /// `<h*>` open + body + close.
    pub fn complete_heading(
        &mut self,
        level: u8,
        toc_text: &str,
        rendered_html: String,
    ) -> CompletedHeading {
        let id = self.generate_id(toc_text);
        // HTML-mode first H1: capture title (still render).
        let is_title =
            self.extract_title && !self.title_as_metadata && level == 1 && self.title.is_none();
        if is_title {
            self.title = Some(toc_text.trim().to_owned());
            self.seen_first_h1 = true;
        }
        let adjusted_level = self.adjusted_level(level);
        if !is_title {
            self.toc.push(TocEntry {
                level: adjusted_level,
                title: toc_text.trim().to_owned(),
                id: id.clone(),
            });
        }
        CompletedHeading {
            adjusted_level,
            id,
            rendered_html,
        }
    }

    /// Adjusted heading level for output: in Confluence mode after the
    /// first H1, every level shifts up by one (H2 → H1, etc.).
    fn adjusted_level(&self, level: u8) -> u8 {
        if self.title_as_metadata && self.seen_first_h1 && level > 1 {
            level - 1
        } else {
            level
        }
    }

    /// Generate a unique slug-based id from heading plain text.
    ///
    /// Headings with no slug characters fall back to the base `section`.
    /// The returned id is guaranteed not to equal any id previously returned
    /// for this document: a numeric suffix is bumped until the candidate is
    /// unused, even when it would collide with another heading's slug.
    fn generate_id(&mut self, text: &str) -> String {
        slugify_into(text, &mut self.slug_scratch);
        if self.slug_scratch.is_empty() {
            self.slug_scratch.push_str("section");
        }
        let base = &self.slug_scratch;

        // Unclaimed base: the slug itself is the id.
        let Some(&start) = self.claimed_ids.get(base) else {
            self.claimed_ids.insert(base.clone(), 1);
            return base.clone();
        };

        // Claimed: walk suffixes from the stored hint. The hint alone isn't
        // enough — `{base}-{n}` can collide with another heading's literal
        // slug — so each candidate is checked against the same map.
        let mut n = start;
        let mut candidate = format!("{base}-{n}");
        while self.claimed_ids.contains_key(&candidate) {
            n += 1;
            candidate = format!("{base}-{n}");
        }
        self.claimed_ids.insert(base.clone(), n + 1);
        self.claimed_ids.insert(candidate.clone(), 1);
        candidate
    }

    /// Take the extracted title.
    pub fn take_title(&mut self) -> Option<String> {
        self.title.take()
    }

    /// Take the table of contents entries.
    pub fn take_toc(&mut self) -> Vec<TocEntry> {
        std::mem::take(&mut self.toc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading_accumulator_html_mode() {
        // HTML mode: extract_title=true, title_as_metadata=false
        // First H1 is captured as title AND emitted.
        let mut acc = HeadingAccumulator::new(true, false);

        // First H1: not skipped — caller pushes Scope::Heading with in_first_h1=false
        // and routes through complete_heading.
        assert!(!acc.is_skipped_title(1));
        let done = acc.complete_heading(1, "My Title", "My Title".to_owned());
        assert_eq!(done.adjusted_level, 1);
        assert_eq!(done.id, "my-title");
        assert_eq!(done.rendered_html, "My Title");

        // H2: rendered at level 2 (no shift in HTML mode).
        let done = acc.complete_heading(2, "Section", "Section".to_owned());
        assert_eq!(done.adjusted_level, 2);

        assert_eq!(acc.take_title(), Some("My Title".to_owned()));
        // Title is NOT in the TOC; only H2 is.
        let toc = acc.take_toc();
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].level, 2);
        assert_eq!(toc[0].title, "Section");
        assert_eq!(toc[0].id, "section");
    }

    #[test]
    fn test_heading_accumulator_confluence_mode() {
        // Confluence mode: extract_title=true, title_as_metadata=true
        // First H1 is title-extracted and NOT emitted; subsequent levels shift up by one.
        let mut acc = HeadingAccumulator::new(true, true);

        // First H1: skipped — caller pushes Scope::Heading with in_first_h1=true
        // and routes through complete_first_h1.
        assert!(acc.is_skipped_title(1));
        acc.complete_first_h1("My Title");

        // After the skipped first H1, is_skipped_title now returns false.
        assert!(!acc.is_skipped_title(1));

        // H2 shifts to level 1.
        let done = acc.complete_heading(2, "Section", "Section".to_owned());
        assert_eq!(done.adjusted_level, 1);

        assert_eq!(acc.take_title(), Some("My Title".to_owned()));
        // Skipped first H1 must NOT appear in the TOC; only the level-shifted H2.
        let toc = acc.take_toc();
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].level, 1, "H2 shifts to level 1 in Confluence mode");
        assert_eq!(toc[0].title, "Section");
        assert_eq!(toc[0].id, "section");
    }

    #[test]
    fn test_generate_id_suffix_collision_is_unique() {
        // "Foo 1" slugifies to "foo-1", which must NOT collide with the
        // second "Foo" (which would otherwise also become "foo-1").
        let mut acc = HeadingAccumulator::new(false, false);
        let a = acc.complete_heading(2, "Foo", "Foo".to_owned());
        let b = acc.complete_heading(2, "Foo 1", "Foo 1".to_owned());
        let c = acc.complete_heading(2, "Foo", "Foo".to_owned());
        assert_eq!(a.id, "foo");
        assert_eq!(b.id, "foo-1");
        assert_eq!(c.id, "foo-2", "second 'Foo' must not reuse 'foo-1'");
    }

    #[test]
    fn test_generate_id_empty_slug_falls_back_to_section() {
        // Headings with no slug characters must not produce empty ids.
        let mut acc = HeadingAccumulator::new(false, false);
        let a = acc.complete_heading(2, "???", "???".to_owned());
        let b = acc.complete_heading(2, "!!!", "!!!".to_owned());
        assert_eq!(a.id, "section");
        assert_eq!(b.id, "section-1");
    }

    #[test]
    fn test_generate_id_empty_slug_yields_to_real_section_heading() {
        // A real "Section" heading claims "section"; the no-slug heading
        // is suffixed (document order: real heading first here).
        let mut acc = HeadingAccumulator::new(false, false);
        let a = acc.complete_heading(2, "Section", "Section".to_owned());
        let b = acc.complete_heading(2, "???", "???".to_owned());
        assert_eq!(a.id, "section");
        assert_eq!(b.id, "section-1");
    }

    #[test]
    fn test_generate_id_real_slug_collides_with_section_fallback() {
        // A real heading slugging to "section-1" must not be reused by the
        // empty-slug fallback's "section" + "-1" suffix.
        let mut acc = HeadingAccumulator::new(false, false);
        let a = acc.complete_heading(2, "Section 1", "Section 1".to_owned());
        let b = acc.complete_heading(2, "???", "???".to_owned());
        let c = acc.complete_heading(2, "???", "???".to_owned());
        assert_eq!(a.id, "section-1");
        assert_eq!(b.id, "section");
        assert_eq!(
            c.id, "section-2",
            "fallback must skip the taken 'section-1'"
        );
    }

    #[test]
    fn test_generate_id_synthesized_id_reused_as_a_base() {
        // "foo-1" is handed out as a *synthesized* suffix, then a later
        // heading slugifies to that same string as its *base*. One map now
        // holds both roles, so this is where the id-set and the suffix-counter
        // could disagree: the base must see itself as taken and skip past it.
        let mut acc = HeadingAccumulator::new(false, false);
        let a = acc.complete_heading(2, "Foo", "Foo".to_owned());
        let b = acc.complete_heading(2, "Foo", "Foo".to_owned());
        let c = acc.complete_heading(2, "Foo 1", "Foo 1".to_owned());
        let d = acc.complete_heading(2, "Foo 1", "Foo 1".to_owned());
        assert_eq!(a.id, "foo");
        assert_eq!(b.id, "foo-1");
        assert_eq!(c.id, "foo-1-1", "base 'foo-1' was already handed out");
        assert_eq!(d.id, "foo-1-2");
    }

    #[test]
    fn test_generate_id_repeated_headings_increment_via_hint() {
        // Four identical headings increment monotonically (id_counts hint path).
        let mut acc = HeadingAccumulator::new(false, false);
        let ids: Vec<String> = (0..4)
            .map(|_| acc.complete_heading(2, "Setup", "Setup".to_owned()).id)
            .collect();
        assert_eq!(ids, ["setup", "setup-1", "setup-2", "setup-3"]);
    }
}
