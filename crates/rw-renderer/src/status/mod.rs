//! Status badge inline directive (`:status[Label]{color=NAME}`).
//!
//! [`StatusColor`] is the six-value color palette; `StatusDirective` (in the
//! `directive` submodule) is the directive handler.

mod directive;

pub use directive::{StatusColor, StatusDirective};
