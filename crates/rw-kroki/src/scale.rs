//! Scaling diagram dimensions from render DPI down to display size.
//!
//! Kroki renders PlantUML-family diagrams at [`PLANTUML_DPI`](rw_plantuml::PLANTUML_DPI)
//! so they stay sharp on retina displays, which makes the output twice the size
//! it should occupy on the page. Every consumer of a rendered diagram has to
//! undo that, and each format reaches for a different numeric type: PNG headers
//! are whole pixels, SVG dimensions may carry decimals.

use crate::consts::STANDARD_DPI;

/// Scale a whole-pixel dimension rendered at `dpi` down to its display size.
///
/// Rounds down, but never to zero: a dimension that survives rendering is worth
/// at least one pixel, and `width="0"` would hide the diagram entirely.
#[must_use]
pub fn to_display_px(value: u32, dpi: u32) -> u32 {
    if value == 0 {
        return 0;
    }
    let scaled = u64::from(value) * u64::from(STANDARD_DPI) / u64::from(dpi);
    u32::try_from(scaled).unwrap_or(u32::MAX).max(1)
}

/// Scale a possibly-fractional dimension rendered at `dpi` down to its display
/// size.
///
/// SVG carries decimal dimensions (`PlantUML` emits `height="98.2656"`), so this
/// keeps the fraction that [`to_display_px`] would truncate.
#[must_use]
pub fn to_display_f64(value: f64, dpi: u32) -> f64 {
    value * f64::from(STANDARD_DPI) / f64::from(dpi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_at_retina_dpi() {
        assert_eq!(to_display_px(400, 192), 200);
        assert!((to_display_f64(98.2656, 192) - 49.1328).abs() < f64::EPSILON);
    }

    #[test]
    fn leaves_standard_dpi_untouched() {
        assert_eq!(to_display_px(300, STANDARD_DPI), 300);
        assert!((to_display_f64(70.0, STANDARD_DPI) - 70.0).abs() < f64::EPSILON);
    }

    /// `value * STANDARD_DPI` overflows u32 above ~44.7M, which used to panic in
    /// debug builds and wrap to a nonsense width in release.
    #[test]
    fn wide_input_saturates_instead_of_overflowing() {
        assert_eq!(to_display_px(u32::MAX, STANDARD_DPI), u32::MAX);
        assert_eq!(to_display_px(u32::MAX, 192), u32::MAX / 2);
    }

    /// Rounding a 1px dimension down to 0 would collapse the element.
    #[test]
    fn small_input_keeps_at_least_one_pixel() {
        assert_eq!(to_display_px(1, 192), 1);
        assert_eq!(to_display_px(0, 192), 0, "nothing to show stays nothing");
    }
}
