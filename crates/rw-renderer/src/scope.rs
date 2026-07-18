//! Per-instance state for currently-active inline-capture scopes.
//!
//! [`Scope`] is the dispatch type the renderer pushes onto its stack when
//! a heading / image / fenced code block / metadata block opens, and pops
//! when it closes. The renderer's inline event methods (`text`,
//! `inline_code`, `raw_html`, `soft_break`, `hard_break`) and
//! `with_markup_buffer` dispatch on `self.scopes.last_mut()` to choose
//! where to write.
//!
//! Cross-instance accumulators (TOC entries, title, `id_counts`,
//! `seen_first_h1`) live on [`HeadingAccumulator`](crate::toc::HeadingAccumulator),
//! not here.

use crate::code_block::FenceAttrs;

/// Per-instance state for a currently-active inline-capture scope.
///
/// Pushed in `start_tag` when an inline-capture region opens; popped in
/// `end_tag` when it closes. Inline event methods (`text`, `inline_code`,
/// `raw_html`, `soft_break`, `hard_break`) and `with_markup_buffer`
/// dispatch on `self.scopes.last_mut()` to choose where to write.
///
/// Cross-instance accumulators (TOC entries, title, `id_counts`,
/// `seen_first_h1`) live on `HeadingAccumulator`, NOT here.
pub(crate) enum Scope {
    /// An open `<h*>` element. Inline events route to `rendered_html` for
    /// backend output and to `toc_text` for the TOC/title plain-text shadow.
    /// On pop, the renderer either captures the title (Confluence-mode
    /// skipped H1) or emits `<h*>` open + html + close into `self.output`.
    Heading {
        /// Original heading level (1..=6), as emitted by pulldown-cmark.
        /// Not yet adjusted for Confluence's title-shift.
        level: u8,
        /// True iff this is the Confluence-mode first H1 that should be
        /// title-extracted and skipped from output. Set at push time from
        /// `HeadingAccumulator::is_skipped_title(level)`.
        in_first_h1: bool,
        /// Plain-text shadow used for TOC entry title, slug id generation,
        /// and (HTML mode) the extracted page title.
        toc_text: String,
        /// Rendered HTML body that ends up inside `<h*>…</h*>`. Backend-
        /// formatted (already escape-encoded by `B::text` etc.); ready to
        /// splice into `output` after `trim()`.
        rendered_html: String,
    },
    /// An open `<img>` whose alt text is being collected. Inline events
    /// append plain text to `alt_text`; markup events (Emphasis/Strong/…)
    /// are dropped by `with_markup_buffer`, matching the `CommonMark` rule
    /// that alt text is a plain-text projection. On pop, the renderer emits
    /// `<img src="{dest_url}" alt="{alt_text}" title="{title}">` via `B::image`.
    Image {
        alt_text: String,
        dest_url: String,
        title: String,
    },
    /// An open fenced code block. Only `text` (and `soft_break` → `\n`)
    /// reach this scope — pulldown-cmark doesn't emit markup or raw HTML
    /// inside a fenced block. On pop, the renderer runs code-block
    /// processors with `self.code_block_index`. `ProcessResult::Inline` and
    /// `PassThrough` emit their result immediately; `Deferred` — e.g. a
    /// diagram — emits nothing here and reserves a hole instead,
    /// filled later by the processor's `fills` in the post-walk assembly
    /// pass. This is why `output` doesn't grow at a diagram fence.
    CodeBlock {
        language: Option<String>,
        buffer: String,
        attrs: FenceAttrs,
    },
    /// An open YAML frontmatter / metadata block. Suppresses every inline
    /// event — nothing should appear in `output`.
    Metadata,
}
