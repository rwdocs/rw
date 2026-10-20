//! Table of contents output type and heading accumulator.
//!
//! [`TocEntry`] is the public output type collected in
//! [`RenderResult::toc`](crate::RenderResult::toc).
//! [`HeadingAccumulator`] is walker-private scratch that tracks
//! cross-heading state (title, TOC entries, id de-duplication state, and
//! whether the walk is still at the page's start) across an entire document
//! render.

use std::collections::HashMap;

use rw_parser::{Event, is_comment_only_html};

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
/// use rw_renderer::{HtmlBackend, MarkdownRenderer};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_title_extraction()
///     .render("# Page Title\n\n## Introduction\n\n## Setup");
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

/// A heading `HeadingAccumulator::complete_heading` keeps in the output.
/// Returned by-value so the caller can emit `B::heading_start` / the body /
/// `B::heading_end` without holding any borrow on the accumulator.
#[derive(Debug)]
pub(crate) struct CompletedHeading {
    /// Heading level after Confluence's title-shift adjustment.
    pub adjusted_level: u8,
    /// Slug-based anchor id, deduped to be unique within the document via the
    /// accumulator's `claimed_ids`.
    pub id: String,
}

/// Persistent accumulator for heading-related output across an entire
/// document render. Holds cross-heading state (title, TOC entries,
/// claimed ids, whether the walk is still at the page's start, and the config
/// flags that govern title extraction).
///
/// Per-heading state (the heading's level, plain-text shadow, formatted
/// HTML body, and the `is_title` flag) lives in
/// [`Scope::Heading`](crate::scope::Scope) — *not* here. The
/// accumulator is consulted at `Tag::Heading` start (to decide whether
/// the heading is the title) and at `TagEnd::Heading` (to capture the title
/// and/or emit the heading).
pub(crate) struct HeadingAccumulator {
    /// Whether to extract the title from a leading H1.
    extract_title: bool,
    /// Whether to drop the title heading from output (Confluence mode).
    title_as_metadata: bool,
    /// Whether nothing but HTML blocks has been walked yet: only a heading
    /// that opens the page, below nothing but comments, can be its title, as
    /// in `rw-meta`.
    at_page_start: bool,
    /// The HTML blocks walked while `at_page_start`.
    leading_html: String,
    /// Extracted title from the leading H1.
    title: Option<String>,
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
    /// * `extract_title` — whether to extract the title from a leading H1
    /// * `title_as_metadata` — whether to drop that H1 from output (Confluence mode)
    pub fn new(extract_title: bool, title_as_metadata: bool) -> Self {
        Self {
            extract_title,
            title_as_metadata,
            at_page_start: true,
            leading_html: String::new(),
            title: None,
            toc: Vec::new(),
            claimed_ids: HashMap::new(),
            slug_scratch: String::new(),
        }
    }

    /// Whether a heading of `level` starting now is the page title.
    pub fn is_title(&self, level: u8) -> bool {
        self.extract_title
            && self.at_page_start
            && level == 1
            && is_comment_only_html(&self.leading_html)
    }

    /// Whether the page's start survives `event`, keeping its HTML to check
    /// for comments. Raw HTML before any block opens is an HTML block, and a
    /// directive-line flag renders nothing.
    pub fn keeps_page_start(&mut self, event: &Event<'_>) -> bool {
        match event {
            Event::RawHtml(html) if self.at_page_start => {
                self.leading_html.push_str(html);
                true
            }
            Event::LiteralDirectiveLine { .. } => true,
            _ => false,
        }
    }

    /// Mark the page's start as passed.
    pub fn leave_page_start(&mut self) {
        self.at_page_start = false;
    }

    /// Complete a heading: capture the title (if `is_title`), generate the
    /// id, and push a TOC entry (unless this *is* the title). Returns `None`
    /// for a title the backend keeps as metadata, which the caller must not
    /// emit.
    pub fn complete_heading(
        &mut self,
        level: u8,
        is_title: bool,
        toc_text: &str,
    ) -> Option<CompletedHeading> {
        // A blank H1 is no title, as in `rw-meta`.
        let is_title = is_title && !toc_text.trim().is_empty();
        if is_title {
            self.title = Some(toc_text.trim().to_owned());
            if self.title_as_metadata {
                return None;
            }
        }
        let id = self.generate_id(toc_text);
        let adjusted_level = self.adjusted_level(level);
        if !is_title {
            self.toc.push(TocEntry {
                level: adjusted_level,
                title: toc_text.trim().to_owned(),
                id: id.clone(),
            });
        }
        Some(CompletedHeading { adjusted_level, id })
    }

    /// Adjusted heading level for output: in Confluence mode, once the title
    /// is taken out, every level shifts up by one (H2 → H1, etc.).
    fn adjusted_level(&self, level: u8) -> u8 {
        if self.title_as_metadata && self.title.is_some() && level > 1 {
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
        // HTML mode: the title H1 is captured AND emitted, but kept out of the TOC.
        let mut acc = HeadingAccumulator::new(true, false);

        assert!(acc.is_title(1));
        let done = acc.complete_heading(1, true, "My Title").unwrap();
        acc.leave_page_start();
        assert_eq!(done.adjusted_level, 1);
        assert_eq!(done.id, "my-title");

        // H2: rendered at level 2 (no shift in HTML mode).
        let done = acc.complete_heading(2, false, "Section").unwrap();
        assert_eq!(done.adjusted_level, 2);

        assert_eq!(acc.take_title(), Some("My Title".to_owned()));
        let toc = acc.take_toc();
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].level, 2);
        assert_eq!(toc[0].title, "Section");
        assert_eq!(toc[0].id, "section");
    }

    #[test]
    fn test_heading_accumulator_confluence_mode() {
        // Confluence mode: the title H1 is NOT emitted; later levels shift up by one.
        let mut acc = HeadingAccumulator::new(true, true);

        assert!(acc.is_title(1));
        assert!(acc.complete_heading(1, true, "My Title").is_none());
        acc.leave_page_start();
        assert!(!acc.is_title(1));

        let done = acc.complete_heading(2, false, "Section").unwrap();
        assert_eq!(done.adjusted_level, 1);

        assert_eq!(acc.take_title(), Some("My Title".to_owned()));
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
        let a = acc.complete_heading(2, false, "Foo").unwrap();
        let b = acc.complete_heading(2, false, "Foo 1").unwrap();
        let c = acc.complete_heading(2, false, "Foo").unwrap();
        assert_eq!(a.id, "foo");
        assert_eq!(b.id, "foo-1");
        assert_eq!(c.id, "foo-2", "second 'Foo' must not reuse 'foo-1'");
    }

    #[test]
    fn test_generate_id_empty_slug_falls_back_to_section() {
        // Headings with no slug characters must not produce empty ids.
        let mut acc = HeadingAccumulator::new(false, false);
        let a = acc.complete_heading(2, false, "???").unwrap();
        let b = acc.complete_heading(2, false, "!!!").unwrap();
        assert_eq!(a.id, "section");
        assert_eq!(b.id, "section-1");
    }

    #[test]
    fn test_generate_id_empty_slug_yields_to_real_section_heading() {
        // A real "Section" heading claims "section"; the no-slug heading
        // is suffixed (document order: real heading first here).
        let mut acc = HeadingAccumulator::new(false, false);
        let a = acc.complete_heading(2, false, "Section").unwrap();
        let b = acc.complete_heading(2, false, "???").unwrap();
        assert_eq!(a.id, "section");
        assert_eq!(b.id, "section-1");
    }

    #[test]
    fn test_generate_id_real_slug_collides_with_section_fallback() {
        // A real heading slugging to "section-1" must not be reused by the
        // empty-slug fallback's "section" + "-1" suffix.
        let mut acc = HeadingAccumulator::new(false, false);
        let a = acc.complete_heading(2, false, "Section 1").unwrap();
        let b = acc.complete_heading(2, false, "???").unwrap();
        let c = acc.complete_heading(2, false, "???").unwrap();
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
        // heading slugifies to that same string as its *base*. One map
        // holds both roles, so this is where the id-set and the suffix-counter
        // could disagree: the base must see itself as taken and skip past it.
        let mut acc = HeadingAccumulator::new(false, false);
        let a = acc.complete_heading(2, false, "Foo").unwrap();
        let b = acc.complete_heading(2, false, "Foo").unwrap();
        let c = acc.complete_heading(2, false, "Foo 1").unwrap();
        let d = acc.complete_heading(2, false, "Foo 1").unwrap();
        assert_eq!(a.id, "foo");
        assert_eq!(b.id, "foo-1");
        assert_eq!(c.id, "foo-1-1", "base 'foo-1' was already handed out");
        assert_eq!(d.id, "foo-1-2");
    }

    #[test]
    fn test_generate_id_repeated_headings_increment_via_hint() {
        let mut acc = HeadingAccumulator::new(false, false);
        let ids: Vec<String> = (0..4)
            .map(|_| acc.complete_heading(2, false, "Setup").unwrap().id)
            .collect();
        assert_eq!(ids, ["setup", "setup-1", "setup-2", "setup-3"]);
    }
}
