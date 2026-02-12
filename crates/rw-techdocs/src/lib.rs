//! TechDocs static site generation and S3 publishing for RW.

mod builder;
mod template;

pub use builder::{BuildConfig, BuildError, StaticSiteBuilder};
