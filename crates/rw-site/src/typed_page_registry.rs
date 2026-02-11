//! Registry of typed pages for meta include resolution.
//!
//! Indexes site pages by `(type, normalized_name)` so that `PlantUML`
//! `!include` directives like `!include meta://system/payment_gateway`
//! can be resolved to concrete entity information.

use std::collections::HashMap;

use rw_diagrams::{EntityInfo, MetaIncludeSource};

use crate::site_state::SiteState;

/// Registry mapping `(entity_type, normalized_name)` to [`EntityInfo`].
///
/// Built from [`SiteState`] by iterating over sections and indexing
/// pages whose type is one of the known entity types (`domain`,
/// `system`, `service`).
pub struct TypedPageRegistry {
    entities: HashMap<(String, String), EntityInfo>,
}

impl TypedPageRegistry {
    /// Build a registry from the current site state.
    ///
    /// Iterates over all sections, keeping only those with a recognized
    /// entity type. The directory name (last path segment) is normalized
    /// by replacing hyphens with underscores to form the lookup key.
    #[must_use]
    pub fn from_site_state(state: &SiteState) -> Self {
        let mut entities = HashMap::new();
        let entity_types = ["domain", "system", "service"];

        for (path, section) in state.sections() {
            if !entity_types.contains(&section.section_type.as_str()) {
                continue;
            }

            // Extract directory name from path (last segment)
            let dir_name = path.rsplit('/').next().unwrap_or(path);

            // Normalize name: hyphens -> underscores
            let normalized_name = dir_name.replace('-', "_");

            // Look up the page for has_content
            let has_docs = state
                .get_page(path)
                .is_some_and(|page| page.has_content);

            let entity = EntityInfo {
                title: section.title.clone(),
                dir_name: dir_name.to_owned(),
                description: None, // Will be populated from metadata in Task 8
                has_docs,
                url_path: format!("/{path}/"),
            };

            let key = (section.section_type.clone(), normalized_name.clone());
            if let Some(existing) = entities.get(&key) {
                let existing: &EntityInfo = existing;
                tracing::warn!(
                    entity_type = %section.section_type,
                    name = %normalized_name,
                    path1 = %existing.url_path,
                    path2 = %entity.url_path,
                    "Meta include name collision: two pages generate the same include path"
                );
            }
            entities.insert(key, entity);
        }

        Self { entities }
    }
}

impl MetaIncludeSource for TypedPageRegistry {
    fn get_entity(&self, entity_type: &str, name: &str) -> Option<EntityInfo> {
        self.entities
            .get(&(entity_type.to_owned(), name.to_owned()))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site_state::SiteStateBuilder;

    #[test]
    fn test_empty_site_produces_empty_registry() {
        let state = SiteStateBuilder::new().build();
        let registry = TypedPageRegistry::from_site_state(&state);
        assert!(registry.get_entity("system", "anything").is_none());
    }

    #[test]
    fn test_system_page_registered() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Payment Gateway".to_owned(),
            "domains/billing/systems/payment-gateway".to_owned(),
            true,
            None,
            Some("system"),
        );
        let state = builder.build();
        let registry = TypedPageRegistry::from_site_state(&state);

        let entity = registry.get_entity("system", "payment_gateway");
        assert!(entity.is_some());
        let entity = entity.unwrap();
        assert_eq!(entity.title, "Payment Gateway");
        assert_eq!(entity.dir_name, "payment-gateway");
        assert!(entity.has_docs);
        assert_eq!(entity.url_path, "/domains/billing/systems/payment-gateway/");
    }

    #[test]
    fn test_domain_page_registered() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Billing".to_owned(),
            "domains/billing".to_owned(),
            true,
            None,
            Some("domain"),
        );
        let state = builder.build();
        let registry = TypedPageRegistry::from_site_state(&state);

        let entity = registry.get_entity("domain", "billing");
        assert!(entity.is_some());
        let entity = entity.unwrap();
        assert_eq!(entity.title, "Billing");
        assert_eq!(entity.dir_name, "billing");
    }

    #[test]
    fn test_service_page_registered() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Invoice API".to_owned(),
            "domains/billing/systems/invoicing/services/invoice-api".to_owned(),
            true,
            None,
            Some("service"),
        );
        let state = builder.build();
        let registry = TypedPageRegistry::from_site_state(&state);

        let entity = registry.get_entity("service", "invoice_api");
        assert!(entity.is_some());
        let entity = entity.unwrap();
        assert_eq!(entity.dir_name, "invoice-api");
    }

    #[test]
    fn test_virtual_page_has_docs_false() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Virtual Domain".to_owned(),
            "domains/virtual".to_owned(),
            false,
            None,
            Some("domain"),
        );
        let state = builder.build();
        let registry = TypedPageRegistry::from_site_state(&state);

        let entity = registry.get_entity("domain", "virtual");
        assert!(entity.is_some());
        assert!(!entity.unwrap().has_docs);
    }

    #[test]
    fn test_non_typed_pages_ignored() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page("Guide".to_owned(), "guide".to_owned(), true, None, None);
        let state = builder.build();
        let registry = TypedPageRegistry::from_site_state(&state);

        assert!(registry.get_entity("system", "guide").is_none());
        assert!(registry.get_entity("domain", "guide").is_none());
        assert!(registry.get_entity("service", "guide").is_none());
    }

    #[test]
    fn test_non_entity_types_ignored() {
        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Getting Started".to_owned(),
            "guide".to_owned(),
            true,
            None,
            Some("guide"),
        );
        let state = builder.build();
        let registry = TypedPageRegistry::from_site_state(&state);

        assert!(registry.get_entity("guide", "guide").is_none());
    }
}
