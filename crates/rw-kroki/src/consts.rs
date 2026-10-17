//! Internal constants for diagram rendering.

use std::time::Duration;

/// Standard display DPI (96 = CSS reference pixel).
pub(crate) const STANDARD_DPI: u32 = 96;

/// Default HTTP timeout for Kroki requests (30 seconds).
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
