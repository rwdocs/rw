//! What a provider produces.

use std::error::Error;
use std::fmt;

/// Rendered size in CSS pixels.
///
/// Already corrected for any oversampling the provider did, so a consumer can
/// put these numbers straight into markup. A provider that renders at 2x for
/// sharpness divides before reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    /// Rendered width in CSS pixels.
    pub width: u32,
    /// Rendered height in CSS pixels.
    pub height: u32,
}

/// Rendered diagram bytes.
///
/// `Svg` is text so a consumer can post-process it — rewriting links, scoping
/// ids. `Png` is opaque.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagramContent {
    Svg(String),
    Png(Vec<u8>),
}

/// Where a consumer gets the rendered diagram from.
///
/// A provider returns [`Asset::Inline`]: it hands back bytes and does no I/O. A
/// caller that writes those bytes somewhere — a Confluence attachment, a file
/// beside the page — replaces the entry with [`Asset::Reference`] before
/// handing resolutions onward, so that whatever emits the final markup points at
/// the written name instead of embedding the bytes again.
///
/// # Examples
///
/// What a Confluence or file-output caller does between resolve and finish:
/// write the bytes, then point the markup at the name.
///
/// ```
/// use rw_diagrams::{Asset, DiagramContent, Resolved, Size};
///
/// let mut resolved = Resolved {
///     asset: Asset::Inline(DiagramContent::Png(Vec::from([0x89, b'P', b'N', b'G']))),
///     size: Size { width: 200, height: 100 },
///     digest: "abc123".to_owned(),
///     warnings: Vec::new(),
/// };
///
/// let Asset::Inline(DiagramContent::Png(bytes)) = &resolved.asset else {
///     panic!("expected inline PNG, got {:?}", resolved.asset);
/// };
/// assert_eq!(bytes.len(), 4);
/// let filename = format!("diagram_{}.png", resolved.digest);
///
/// resolved.asset = Asset::Reference(filename);
/// assert_eq!(
///     resolved.asset,
///     Asset::Reference("diagram_abc123.png".to_owned()),
/// );
/// // Swapping the asset must not disturb the size the markup needs.
/// assert_eq!(resolved.size, Size { width: 200, height: 100 });
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asset {
    /// The bytes themselves.
    Inline(DiagramContent),
    /// A name the consumer's markup should point at.
    Reference(String),
}

/// A successfully rendered diagram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// The rendered bytes, or a reference to them.
    pub asset: Asset,
    /// Display size in CSS pixels, correction already applied.
    pub size: Size,
    /// Content digest for this render: lowercase hex, safe to use as a filename.
    ///
    /// Derived from the inputs that determine the bytes — the source, the output
    /// format, and whatever else this provider varies on — not from the bytes
    /// themselves. So equal digests mean the same render was requested, but two
    /// different sources that happen to render identically still get different
    /// digests: this is not a content-dedupe key.
    ///
    /// Algorithm and length are the provider's choice; callers must not assume
    /// either beyond lowercase hex.
    pub digest: String,
    /// Diagnostics that did not prevent a render, e.g. an unresolved include or
    /// an attribute the provider ignored.
    pub warnings: Vec<String>,
}

/// Why one diagram could not be rendered.
///
/// Per-diagram: one bad fence does not fail the page, and the others in the same
/// batch still resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramError {
    /// A human-readable description of what went wrong.
    pub message: String,
    /// Whether retrying could succeed — a network timeout, not a syntax error.
    /// A caller that caches renders uses this to decide whether storing the
    /// failure would poison the cache.
    pub transient: bool,
}

impl fmt::Display for DiagramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for DiagramError {}

#[cfg(test)]
mod tests {
    use super::DiagramError;

    #[test]
    fn a_diagram_error_displays_as_its_message() {
        let error = DiagramError {
            message: "kroki returned 503".to_owned(),
            transient: true,
        };
        assert_eq!(error.to_string(), "kroki returned 503");
    }
}
