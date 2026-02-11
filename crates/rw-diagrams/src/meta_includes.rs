//! Meta include resolution for PlantUML diagrams.
//!
//! Generates C4 model PlantUML macros from page metadata, enabling
//! `!include systems/sys_payment_gateway.iuml` to resolve dynamically
//! from `meta.yaml` files without maintaining separate `.iuml` files.

/// Entity metadata for generating PlantUML C4 includes.
#[derive(Clone, Debug)]
pub struct EntityInfo {
    /// Display title from meta.yaml.
    pub title: String,
    /// Raw directory name (with hyphens, e.g., "payment-gateway").
    pub dir_name: String,
    /// Optional description from meta.yaml.
    pub description: Option<String>,
    /// Whether the page has actual docs (index.md exists).
    pub has_docs: bool,
    /// URL path for linking (e.g., "/domains/billing/systems/payment-gateway/").
    pub url_path: String,
}

/// Source for resolving PlantUML meta includes from page metadata.
///
/// Implemented by site-level registries that track typed pages.
/// The diagram processor queries this trait during `!include` resolution.
pub trait MetaIncludeSource: Send + Sync {
    /// Look up an entity by type and normalized name.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - One of "domain", "system", "service"
    /// * `name` - Normalized name with underscores (e.g., "payment_gateway")
    fn get_entity(&self, entity_type: &str, name: &str) -> Option<EntityInfo>;
}
