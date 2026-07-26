//! The vocabulary RW's diagram providers share.
//!
//! A *provider* turns diagram source into rendered content: bytes, a display
//! [`Size`], a [`Resolved::digest`], and warnings — never markup. It never
//! writes files either, so the same [`Resolved`] can be embedded inline by one
//! caller and written out as an attachment by another.
//!
//! Some diagram syntaxes name entities from the surrounding documentation site
//! rather than describing them inline. [`SiteModel`] is the port through which a
//! provider looks those up, so no provider needs to know how the site is stored
//! or scanned.
//!
//! [`Providers`] routes a batch of [`DiagramRequest`]s to whichever provider
//! claims each fence language, and returns [`Resolutions`] keyed back to the
//! requests. A renderer is meant to hold only the narrower [`DiagramRouter`],
//! so it can recognise a diagram fence without being able to render one.

mod model;
mod provider;
mod request;
mod resolved;

pub use model::{Entity, SiteModel};
pub use provider::{DiagramProvider, DiagramRouter, Providers, Resolutions};
pub use request::{DiagramRequest, RequestKey, ResolveContext};
pub use resolved::{Asset, DiagramContent, DiagramError, Resolved, Size};
