//! TechDocs static site generation and S3 publishing for RW.

mod builder;
mod publisher;
mod template;

pub use builder::{BuildConfig, BuildError, StaticSiteBuilder};
pub use publisher::{PublishConfig, PublishError, S3Publisher};
