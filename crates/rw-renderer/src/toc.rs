//! Table of contents output type and heading accumulator.

use std::collections::HashMap;

use crate::util::slugify_into;

/// A single heading in the table of contents.
///
/// Produced by [`MarkdownRenderer`](crate::MarkdownRenderer) for every heading
/// in the document but the H1 that opens it (see
/// [`RenderResult::title`](crate::RenderResult::title)). Collected in
/// [`RenderResult::toc`](crate::RenderResult::toc).
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, MarkdownRenderer};
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
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
    /// Heading level (1–6), shifted up by one below a dropped title (see
    /// [`with_title_dropped`](crate::MarkdownRenderer::with_title_dropped)).
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
#[derive(Debug)]
pub(crate) struct CompletedHeading {
    /// Heading level after Confluence's title-shift adjustment.
    pub adjusted_level: u8,
    /// Slug-based anchor id, unique within the document.
    pub id: String,
}

/// Heading output across an entire document render. Per-heading state lives
/// in [`Scope::Heading`](crate::scope::Scope).
pub(crate) struct HeadingAccumulator {
    /// Whether to drop the title heading from output (Confluence mode).
    drop_title: bool,
    /// The text of the H1 that opens the page.
    title: Option<String>,
    /// Table of contents entries.
    toc: Vec<TocEntry>,
    /// Every string already claimed as an id in this render, mapped to the
    /// next suffix to try when that string comes up again as a base slug.
    claimed_ids: HashMap<String, usize>,
    /// Reused slug buffer, so `generate_id` doesn't allocate a `String` per
    /// heading just to hash it.
    slug_scratch: String,
}

impl HeadingAccumulator {
    pub fn new(drop_title: bool) -> Self {
        Self {
            drop_title,
            title: None,
            toc: Vec::new(),
            claimed_ids: HashMap::new(),
            slug_scratch: String::new(),
        }
    }

    /// Complete a heading: capture the title (if it `opens_page` and shows
    /// text), generate the id, and push a TOC entry (unless it `opens_page`).
    /// Returns `None` for a title to drop, which the caller must not emit.
    pub fn complete_heading(
        &mut self,
        level: u8,
        opens_page: bool,
        toc_text: &str,
    ) -> Option<CompletedHeading> {
        if opens_page && !toc_text.trim().is_empty() {
            self.title = Some(toc_text.trim().to_owned());
            if self.drop_title {
                return None;
            }
        }
        let id = self.generate_id(toc_text);
        let adjusted_level = self.adjusted_level(level);
        if !opens_page {
            self.toc.push(TocEntry {
                level: adjusted_level,
                title: toc_text.trim().to_owned(),
                id: id.clone(),
            });
        }
        Some(CompletedHeading { adjusted_level, id })
    }

    /// Adjusted heading level for output: once the title is dropped, every
    /// level shifts up by one (H2 → H1, etc.).
    fn adjusted_level(&self, level: u8) -> u8 {
        if self.drop_title && self.title.is_some() && level > 1 {
            level - 1
        } else {
            level
        }
    }

    /// Generate a unique slug-based id from heading plain text.
    ///
    /// Headings with no slug characters fall back to the base `section`.
    fn generate_id(&mut self, text: &str) -> String {
        slugify_into(text, &mut self.slug_scratch);
        if self.slug_scratch.is_empty() {
            self.slug_scratch.push_str("section");
        }
        let base = &self.slug_scratch;

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

    /// Take the title.
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
        let mut acc = HeadingAccumulator::new(false);

        let done = acc.complete_heading(1, true, "My Title").unwrap();
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
        let mut acc = HeadingAccumulator::new(true);

        assert!(acc.complete_heading(1, true, "My Title").is_none());

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
        let mut acc = HeadingAccumulator::new(false);
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
        let mut acc = HeadingAccumulator::new(false);
        let a = acc.complete_heading(2, false, "???").unwrap();
        let b = acc.complete_heading(2, false, "!!!").unwrap();
        assert_eq!(a.id, "section");
        assert_eq!(b.id, "section-1");
    }

    #[test]
    fn test_generate_id_empty_slug_yields_to_real_section_heading() {
        // A real "Section" heading claims "section"; the no-slug heading
        // is suffixed (document order: real heading first here).
        let mut acc = HeadingAccumulator::new(false);
        let a = acc.complete_heading(2, false, "Section").unwrap();
        let b = acc.complete_heading(2, false, "???").unwrap();
        assert_eq!(a.id, "section");
        assert_eq!(b.id, "section-1");
    }

    #[test]
    fn test_generate_id_real_slug_collides_with_section_fallback() {
        // A real heading slugging to "section-1" must not be reused by the
        // empty-slug fallback's "section" + "-1" suffix.
        let mut acc = HeadingAccumulator::new(false);
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
        // "foo-1" is handed out as a suffix, then a later heading slugifies to
        // it as a base, which must see itself as taken.
        let mut acc = HeadingAccumulator::new(false);
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
        let mut acc = HeadingAccumulator::new(false);
        let ids: Vec<String> = (0..4)
            .map(|_| acc.complete_heading(2, false, "Setup").unwrap().id)
            .collect();
        assert_eq!(ids, ["setup", "setup-1", "setup-2", "setup-3"]);
    }
}
