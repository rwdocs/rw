//! What a provider produces.

use std::error::Error;
use std::fmt;

/// Rendered size in CSS pixels, with any oversampling already divided out.
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
/// A provider returns [`Asset::Inline`]. A caller that writes the bytes
/// somewhere replaces it with [`Asset::Reference`], so the final markup points
/// at the written name.
///
/// # Examples
///
///
/// ```
/// use rw_diagrams::{Asset, DiagramContent, Resolved, Size};
///
/// let mut resolved = Resolved {
///     asset: Asset::Inline(DiagramContent::Png(Vec::from([0x89, b'P', b'N', b'G']))),
///     size: Some(Size { width: 200, height: 100 }),
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
/// assert_eq!(resolved.size, Some(Size { width: 200, height: 100 }));
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
    /// Display size; `None` when the content sizes itself (an SVG with
    /// intrinsic `width` and `height`) or the provider could not determine one,
    /// so `None` on a raster does not mean self-sizing.
    pub size: Option<Size>,
    /// Content digest for this render: lowercase hex, safe to use as a filename.
    ///
    /// Derived from the render's inputs, not its bytes, so it is not a
    /// content-dedupe key. Callers must not assume an algorithm or length.
    pub digest: String,
    /// Diagnostics that did not prevent a render, e.g. an unresolved include or
    /// an attribute the provider ignored.
    ///
    /// A warning names the problem, not the diagram; attributing it to a fence
    /// is the caller's job. Order carries no meaning.
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
    /// A cache must not store a transient failure.
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
