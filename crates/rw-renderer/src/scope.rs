//! Per-instance state for currently-active inline-capture scopes.
//!
//! Fenced code blocks and metadata blocks are not scopes here — they are the
//! [`Parser`](rw_parser::Parser)'s: a fence is accumulated into a single
//! [`Event::CodeBlock`](rw_parser::Event::CodeBlock) and a metadata block is
//! swallowed whole. Splitting the stack that way is safe because the two
//! families never interleave — a fence cannot occur inside a heading or inside
//! alt text, so the Parser's single-slot state can never nest with a scope
//! here.

/// Per-instance state for a currently-active inline-capture scope.
///
/// Pushed in `start_tag` when an inline-capture region opens; popped in
/// `end_tag` when it closes. Inline event methods (`text`, `inline_code`,
/// `raw_html`, `soft_break`, `hard_break`) and `with_markup_buffer`
/// dispatch on `self.scopes.last_mut()` to choose where to write.
///
/// Cross-instance accumulators (TOC entries, title, `claimed_ids`,
/// `at_page_start`) live on [`HeadingAccumulator`](crate::toc::HeadingAccumulator),
/// not here.
pub(crate) enum Scope {
    /// An open `<h*>` element. Inline events route to `rendered_html` for
    /// backend output and to `toc_text` for the TOC/title plain-text shadow.
    /// On pop, the renderer either captures the title (Confluence-mode
    /// skipped H1) or emits `<h*>` open + html + close into `self.output`.
    Heading {
        /// Source heading level (1..=6), not yet adjusted for Confluence's
        /// title-shift.
        level: u8,
        /// Whether this heading is the page title. Set at push time from
        /// `HeadingAccumulator::is_title(level)`.
        is_title: bool,
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
}
