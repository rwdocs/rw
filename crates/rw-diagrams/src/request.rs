//! What a provider is asked to render.

use crate::model::SiteModel;

/// Identifies one diagram request, and the resolution that answers it.
///
/// A renderer mints these from its own counter and uses them to match each
/// resolution back to whatever it reserved for that request. A provider should
/// treat the value as opaque: nothing guarantees the keys in one batch are
/// contiguous, ordered, or start at zero.
///
/// Keys within one batch must be distinct. That is a requirement on whoever
/// builds the batch, not something a provider can violate or check:
/// [`Providers::resolve`](crate::Providers::resolve) returns a map keyed by
/// these, so two requests sharing a key collapse into one entry and one of the
/// two results is lost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestKey(u32);

impl From<u32> for RequestKey {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// One diagram to render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramRequest {
    /// Identifies this request among the others in its batch.
    pub key: RequestKey,
    /// Fence language as the author wrote it, e.g. `plantuml` or `kroki-mermaid`.
    pub language: String,
    /// Fence body, exactly as authored — a provider does its own preprocessing.
    pub source: String,
    /// Fence attributes in author order, e.g. `format=png`. A provider ignores
    /// keys it does not know, and should warn rather than fail on them.
    pub attrs: Vec<(String, String)>,
    /// The writer's `{#id}`, when they set one.
    pub id: Option<String>,
}

/// Per-render inputs, identical for every fence on one page.
///
/// Passed to [`resolve`](crate::DiagramProvider::resolve) rather than held by
/// the provider, so a provider's own state — an HTTP agent, a parsed model —
/// can live as long as the process while these change per page.
#[derive(Default)]
pub struct ResolveContext<'a> {
    /// The site's entities, for providers whose syntax references them. `None`
    /// when the caller has no model to offer; a provider that needs one should
    /// report an error for the requests that depended on it.
    pub model: Option<&'a dyn SiteModel>,
    /// Site-relative path of the page being rendered, for diagnostics.
    pub page: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::{RequestKey, ResolveContext};
    use crate::{Entity, SiteModel};

    #[test]
    fn request_keys_are_distinct_by_value() {
        assert_eq!(RequestKey::from(7), RequestKey::from(7));
        assert_ne!(RequestKey::from(7), RequestKey::from(8));
    }

    #[test]
    fn an_empty_resolve_context_offers_no_model_and_no_page() {
        let ctx = ResolveContext::default();
        assert!(ctx.model.is_none());
        assert_eq!(ctx.page, None);
    }

    #[test]
    fn a_resolve_context_lends_a_site_model_a_provider_can_query() {
        struct OneSystem;
        impl SiteModel for OneSystem {
            fn entity(&self, kind: &str, name: &str) -> Option<Entity> {
                (kind == "system" && name == "payments").then(|| Entity {
                    title: "Payments".to_owned(),
                    description: None,
                    url_path: None,
                })
            }
        }

        let model = OneSystem;
        let ctx = ResolveContext {
            model: Some(&model),
            page: Some("domains/billing.md"),
        };

        // The point of the borrow: a provider reaches the site through the
        // context without owning or knowing the concrete model type.
        let looked_up = ctx
            .model
            .expect("model present")
            .entity("system", "payments");
        assert_eq!(looked_up.map(|e| e.title), Some("Payments".to_owned()));
        assert_eq!(ctx.page, Some("domains/billing.md"));
    }
}
