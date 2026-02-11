//! Registry of typed pages for meta include resolution.
//!
//! Indexes site pages by `(type, normalized_name)` so that `PlantUML`
//! `!include` directives like `!include systems/sys_payment_gateway.iuml`
//! can be resolved to concrete entity information.

use std::collections::HashMap;

use rw_diagrams::{EntityInfo, MetaIncludeSource};
use rw_storage::Storage;

use crate::site_state::SiteState;

/// Registry mapping `(entity_type, normalized_name)` to [`EntityInfo`].
///
/// Built from site state by iterating over sections and indexing
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

            let has_content = state.get_page(path).is_some_and(|page| page.has_content);

            let title = match section.section_type.as_str() {
                "service" => dir_name.to_owned(),
                _ => section.title.clone(),
            };

            let entity = EntityInfo {
                title,
                description: None,
                url_path: has_content.then(|| format!("/{path}")),
            };

            let key = (section.section_type.clone(), normalized_name.clone());
            if entities.contains_key(&key) {
                tracing::warn!(
                    entity_type = %section.section_type,
                    name = %normalized_name,
                    path = %path,
                    "Meta include name collision: two pages generate the same include path"
                );
            }
            entities.insert(key, entity);
        }

        Self { entities }
    }

    /// Build a registry from site state, populating descriptions from storage.
    ///
    /// Like `from_site_state`, but also queries the storage for each entity's
    /// `meta.yaml` description field.
    #[must_use]
    pub fn from_site_state_with_storage(state: &SiteState, storage: &dyn Storage) -> Self {
        let mut registry = Self::from_site_state(state);

        // Populate descriptions from storage metadata
        for (path, section) in state.sections() {
            let dir_name = path.rsplit('/').next().unwrap_or(path);
            let normalized_name = dir_name.replace('-', "_");
            let key = (section.section_type.clone(), normalized_name);
            if let Some(entity) = registry.entities.get_mut(&key) {
                if let Ok(Some(meta)) = storage.meta(path) {
                    entity.description = meta.description;
                }
            }
        }

        registry
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
        assert_eq!(
            entity.url_path.as_deref(),
            Some("/domains/billing/systems/payment-gateway")
        );
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
        assert_eq!(entity.title, "invoice-api");
    }

    #[test]
    fn test_virtual_page_has_no_url() {
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
        assert!(entity.unwrap().url_path.is_none());
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

    #[test]
    fn test_from_site_state_with_storage_populates_descriptions() {
        use rw_storage::{Metadata, MockStorage};

        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Payment Gateway".to_owned(),
            "domains/billing/systems/payment-gateway".to_owned(),
            true,
            None,
            Some("system"),
        );
        let state = builder.build();

        let storage = MockStorage::new().with_metadata(
            "domains/billing/systems/payment-gateway",
            Metadata {
                description: Some("Handles payment processing".to_owned()),
                ..Default::default()
            },
        );

        let registry = TypedPageRegistry::from_site_state_with_storage(&state, &storage);

        let entity = registry.get_entity("system", "payment_gateway").unwrap();
        assert_eq!(
            entity.description.as_deref(),
            Some("Handles payment processing")
        );
    }

    #[test]
    fn test_from_site_state_with_storage_no_metadata() {
        use rw_storage::MockStorage;

        let mut builder = SiteStateBuilder::new();
        builder.add_page(
            "Billing".to_owned(),
            "domains/billing".to_owned(),
            true,
            None,
            Some("domain"),
        );
        let state = builder.build();

        let storage = MockStorage::new();
        let registry = TypedPageRegistry::from_site_state_with_storage(&state, &storage);

        let entity = registry.get_entity("domain", "billing").unwrap();
        assert!(entity.description.is_none());
    }
}
