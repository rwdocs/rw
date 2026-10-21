//! Per-instance state for currently-active inline-capture scopes.
//!
//! Fenced code blocks and metadata blocks are not scopes here: the
//! [`Parser`](rw_parser::Parser) consumes them whole. A fence cannot occur
//! inside a heading or alt text, so the two never nest.

/// Per-instance state for a currently-active inline-capture scope.
///
/// Inline events write into the innermost scope. Cross-heading state lives on
/// [`HeadingAccumulator`](crate::toc::HeadingAccumulator).
pub(crate) enum Scope {
    /// An open `<h*>` element.
    Heading {
        /// Source heading level (1..=6), not yet adjusted for Confluence's
        /// title-shift.
        level: u8,
        /// See [`Tag::Heading`](rw_parser::Tag::Heading).
        opens_page: bool,
        /// Plain-text shadow used for TOC entry title, slug id generation,
        /// and the page title.
        toc_text: String,
        /// Backend-formatted body of `<h*>…</h*>`.
        rendered_html: String,
    },
    /// An open image whose alt text is being collected. Markup is dropped:
    /// alt text is a plain-text projection.
    Image { alt_text: String, shown_as: ImageAs },
}

/// What an image renders as once its alt text is complete.
pub(crate) enum ImageAs {
    /// `<img src="{dest_url}" alt="{alt_text}" title="{title}">` via `B::image`.
    Image { dest_url: String, title: String },
    /// A diagram file alone in its paragraph: a diagram hole and request.
    Diagram {
        language: &'static str,
        path: String,
    },
    /// Its alt text, in place of a diagram file that can't render here.
    AltText,
}
