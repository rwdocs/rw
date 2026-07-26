//! Provider-agnostic vocabulary shared by RW's diagram providers.
//!
//! A diagram provider turns fence source into rendered content. Some of that
//! source names entities from the surrounding documentation site — a system, a
//! domain, a service — rather than describing them inline. [`SiteModel`] is the
//! port through which a provider looks those up, so no provider needs to know
//! how the site is stored or scanned.

/// An entity a diagram can refer to: a system, domain, or service.
///
/// Values are already resolved for display. `title` is the human-readable name;
/// `description` and `url_path` are absent when the site has none — a section
/// with no page of its own has no URL to link to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    /// Display title.
    pub title: String,
    /// One-line description, when the site defines one.
    pub description: Option<String>,
    /// Site-absolute path to the entity's page (e.g. `/domains/billing`), or
    /// `None` when the entity has no page to link to.
    pub url_path: Option<String>,
}

/// Looks up documentation-site entities by kind and name.
///
/// Implemented by whatever owns the site's structure; consumed by diagram
/// providers resolving references to entities the fence does not define
/// inline.
///
/// # Examples
///
/// ```
/// use rw_diagrams::{Entity, SiteModel};
///
/// struct OneSystem;
///
/// impl SiteModel for OneSystem {
///     fn entity(&self, kind: &str, name: &str) -> Option<Entity> {
///         (kind == "system" && name == "payment_gateway").then(|| Entity {
///             title: "Payment Gateway".to_owned(),
///             description: Some("Processes payments".to_owned()),
///             url_path: Some("/domains/billing/systems/payment-gateway".to_owned()),
///         })
///     }
/// }
///
/// let model: &dyn SiteModel = &OneSystem;
/// assert_eq!(
///     model.entity("system", "payment_gateway").map(|e| e.title),
///     Some("Payment Gateway".to_owned()),
/// );
/// assert_eq!(model.entity("domain", "billing"), None);
/// ```
pub trait SiteModel: Send + Sync {
    /// Look up an entity, or `None` when the site has no such entity.
    ///
    /// `kind` matches a section kind (`"domain"`, `"system"`, `"service"`).
    /// `name` is normalized with underscores, as diagram source spells it
    /// (e.g. `payment_gateway`).
    fn entity(&self, kind: &str, name: &str) -> Option<Entity>;
}
