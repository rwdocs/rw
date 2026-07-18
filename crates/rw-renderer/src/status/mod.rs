//! Status badge inline directive (`:status[Label]{color=NAME}`).
//!
//! [`StatusColor`] is the six-value color palette; `StatusDirective` (in the
//! `directive` submodule) is the directive handler. [`STATUS_MARKER`] is the
//! semantic marker name that handler emits — a backend matches on it to render
//! badges its own way.

mod directive;

pub use directive::{STATUS_MARKER, StatusColor, StatusDirective};
