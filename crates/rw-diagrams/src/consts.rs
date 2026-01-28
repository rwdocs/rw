//! Internal constants for diagram rendering.

use std::time::Duration;

/// Default DPI for diagram rendering (192 = 2x for retina displays).
pub const DEFAULT_DPI: u32 = 192;

/// Standard display DPI (96 = CSS reference pixel).
pub const STANDARD_DPI: u32 = 96;

/// Default HTTP timeout for Kroki requests (30 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
