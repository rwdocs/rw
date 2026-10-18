//! Diagram rendering via Kroki for RW.
//!
//! [`KrokiProvider`] implements `rw_diagrams::DiagramProvider`: it turns diagram
//! source into rendered content — SVG text or PNG bytes, a display size, a
//! content digest, and any warnings — and never markup. What a diagram becomes
//! on a page is the render backend's decision, so this crate does not depend on
//! a renderer at all.
//!
//! # Example
//!
//! ```
//! use rw_diagrams::DiagramProvider;
//! use rw_kroki::KrokiProvider;
//!
//! let provider = KrokiProvider::new("https://kroki.io");
//! assert!(provider.handles("plantuml"));
//! assert!(!provider.handles("rust"));
//! ```

mod cache;
mod consts;
mod kroki;
mod language;
mod provider;
mod svg;
#[cfg(test)]
mod test_support;

pub use provider::KrokiProvider;
