//! Diagram rendering via Kroki for RW.
//!
//! [`KrokiProvider`] implements `rw_diagrams::DiagramProvider`: it turns diagram
//! source into rendered content — SVG text or PNG bytes, a display size, a
//! content digest, and any warnings — and never markup. What a diagram becomes
//! on a page is the render backend's decision, so this crate does not depend on
//! a renderer at all.
//!
//! # Architecture
//!
//! The crate is organized into modules:
//! - [`language`]: Diagram type definitions (`DiagramLanguage`, `DiagramFormat`)
//! - [`provider`]: [`KrokiProvider`] implementing the `DiagramProvider` trait
//! - [`kroki`]: Parallel HTTP rendering via Kroki service
//! - [`html_embed`]: SVG post-processing — DPI scaling and Google Fonts stripping
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
mod html_embed;
mod kroki;
mod language;
mod provider;
mod scale;
#[cfg(test)]
mod test_support;

pub use provider::KrokiProvider;
