//! Table of contents output type and heading accumulator.
//!
//! [`TocEntry`] is the public output type collected in
//! [`RenderResult::toc`](crate::RenderResult::toc).
//! [`HeadingAccumulator`] is walker-private scratch that tracks
//! cross-heading state (title, TOC entries, id-counts, and the
//! "have we seen the first H1?" flag) across an entire document render.

use std::collections::HashMap;

use crate::util::slugify;

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
    /// Duplicate headings get a numeric suffix (e.g., `setup`, `setup-1`).
    pub id: String,
}

/// Result of completing a heading via `HeadingAccumulator::complete_heading`.
/// Returned by-value so the caller can emit `B::heading_start` / push `html` /
/// `B::heading_end` without holding any borrow on the accumulator.
#[derive(Debug)]
pub(crate) struct CompletedHeading {
    /// Heading level after Confluence's title-shift adjustment.
    pub adjusted_level: u8,
    /// Slug-based anchor id, deduped via the accumulator's `id_counts`.
    pub id: String,
    /// Backend-formatted HTML body, ready to splice into `output` between
    /// the heading's open and close tags. Encoding/escaping is whatever the
    /// active `RenderBackend` produced during the inline phase.
    pub rendered_html: String,
}

/// Persistent accumulator for heading-related output across an entire
/// document render. Holds cross-heading state (title, TOC entries,
/// id-counts, the "have we seen the first H1?" flag, and the config
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
    /// Counter for generating unique heading IDs.
    id_counts: HashMap<String, usize>,
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
            id_counts: HashMap::new(),
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
    fn generate_id(&mut self, text: &str) -> String {
        let base_id = slugify(text);
        let count = self.id_counts.entry(base_id.clone()).or_default();
        let id = match *count {
            0 => base_id,
            n => format!("{base_id}-{n}"),
        };
        *count += 1;
        id
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
}
