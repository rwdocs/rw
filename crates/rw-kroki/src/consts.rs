//! Internal constants for diagram rendering.

use std::time::Duration;

/// Default DPI for diagram rendering (192 = 2x for retina displays).
pub const DEFAULT_DPI: u32 = 192;

/// Standard display DPI (96 = CSS reference pixel).
pub const STANDARD_DPI: u32 = 96;

/// Default HTTP timeout for Kroki requests (30 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Prefix of the data URI an inline PNG render is carried and cached as.
///
/// Whoever writes one of these and whoever reads it back have to agree
/// character for character, so both spell it from here.
pub const PNG_DATA_URI_PREFIX: &str = "data:image/png;base64,";
