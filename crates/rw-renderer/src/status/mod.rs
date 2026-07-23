//! Status badge (`:status[Label]{color=NAME}`), a built-in inline element.
//!
//! [`StatusColor`] is the six-value color palette; the walker recognizes the
//! `status` directive name internally for the built-in badge. Rendering
//! goes straight through [`RenderBackend::status_open`](crate::RenderBackend::status_open)/
//! [`status_close`](crate::RenderBackend::status_close) — there is no
//! separate directive handler to register.

mod directive;

pub(crate) use directive::STATUS_NAME;
pub use directive::StatusColor;
