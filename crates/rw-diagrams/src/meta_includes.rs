//! Meta include resolution for `PlantUML` diagrams.
//!
//! Generates C4 model `PlantUML` macros from page metadata, enabling
//! `!include systems/sys_payment_gateway.iuml` to resolve dynamically
//! from `meta.yaml` files without maintaining separate `.iuml` files.

use rw_renderer::relative_path;

/// Configuration for transforming `$link` URLs in C4 macros.
///
/// When provided, diagram link URLs are transformed to match the renderer's
/// link mode (relative paths, trailing slashes).
#[derive(Clone, Debug)]
pub struct LinkConfig {
    /// Base path of the page containing the diagram (e.g., "/domains/billing").
    pub base_path: String,
    /// Convert absolute URLs to relative paths.
    pub relative_links: bool,
    /// Append trailing slash to URLs.
    pub trailing_slash: bool,
}

/// Entity metadata for generating `PlantUML` C4 includes.
#[derive(Clone, Debug)]
pub struct EntityInfo {
    /// Display title for the C4 macro.
    pub title: String,
    /// Optional description from meta.yaml.
    pub description: Option<String>,
    /// URL path for linking (e.g., "/domains/billing/systems/payment-gateway").
    /// `None` for virtual pages without content.
    pub url_path: Option<String>,
}

/// Source for resolving `PlantUML` meta includes from page metadata.
///
/// Implemented by site-level registries that track typed pages.
/// The diagram processor queries this trait during `!include` resolution.
pub trait MetaIncludeSource: Send + Sync {
    /// Look up an entity by type and normalized name.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - One of "domain", "system", "service"
    /// * `name` - Normalized name with underscores (e.g., `payment_gateway`)
    fn get_entity(&self, entity_type: &str, name: &str) -> Option<EntityInfo>;
}

/// Parsed components of a meta include path.
#[derive(Debug, PartialEq, Eq)]
struct ParsedIncludePath {
    /// The entity type: "system", "domain", or "service".
    entity_type: &'static str,
    /// Normalized name with underscores (e.g., `payment_gateway`).
    name: String,
    /// Whether the entity is external (from the `ext/` subdirectory).
    external: bool,
}

/// Parse a `PlantUML` include path into structured components.
///
/// Recognizes paths of the form `systems/{ext/}{prefix}_{name}.iuml` where
/// prefix is one of `sys_`, `dmn_`, or `svc_`.
///
/// Returns `None` if the path doesn't match the expected pattern.
fn parse_include_path(path: &str) -> Option<ParsedIncludePath> {
    let rest = path.strip_prefix("systems/")?;
    let (external, rest) = if let Some(r) = rest.strip_prefix("ext/") {
        (true, r)
    } else {
        (false, rest)
    };
    let stem = rest.strip_suffix(".iuml")?;
    let (entity_type, name) = if let Some(name) = stem.strip_prefix("sys_") {
        ("system", name)
    } else if let Some(name) = stem.strip_prefix("dmn_") {
        ("domain", name)
    } else if let Some(name) = stem.strip_prefix("svc_") {
        ("service", name)
    } else {
        return None;
    };
    if name.is_empty() {
        return None;
    }
    Some(ParsedIncludePath {
        entity_type,
        name: name.to_owned(),
        external,
    })
}

/// Return the short prefix for an entity type.
fn type_prefix(entity_type: &str) -> &'static str {
    match entity_type {
        "domain" => "dmn",
        "system" => "sys",
        "service" => "svc",
        _ => "unknown",
    }
}

/// Escape newlines in a description for use inside `PlantUML` strings.
fn escape_description(desc: &str) -> String {
    desc.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Transform a URL according to link configuration.
///
/// Mirrors the logic of `MarkdownRenderer::resolve_href()`:
/// 1. Add trailing slash if enabled
/// 2. Convert to relative path if enabled
fn resolve_link(url: &str, config: &LinkConfig) -> String {
    let mut path = url.to_owned();

    if config.trailing_slash && !path.ends_with('/') {
        path.push('/');
    }

    if config.relative_links {
        let from = if config.trailing_slash && !config.base_path.ends_with('/') {
            format!("{}/", config.base_path)
        } else {
            config.base_path.clone()
        };
        relative_path(&from, &path)
    } else {
        path
    }
}

/// Render a C4 `PlantUML` macro call from entity metadata.
///
/// The output follows the C4-PlantUML conventions used by the mkdocs arch
/// plugin. The macro variant (`System` vs `System_Ext`) depends on the
/// `external` flag.
///
/// When `link_config` is provided, URLs are transformed to match the
/// renderer's link mode. When `None`, absolute URLs are used as-is.
fn render_c4_macro(
    entity_type: &str,
    name: &str,
    entity: &EntityInfo,
    external: bool,
    link_config: Option<&LinkConfig>,
) -> String {
    let prefix = type_prefix(entity_type);
    let alias = format!("{prefix}_{name}");

    let title = &entity.title;

    let link_part = entity.url_path.as_deref().map_or(String::new(), |url| {
        let resolved = match link_config {
            Some(config) => resolve_link(url, config),
            None => url.to_owned(),
        };
        format!(", $link=\"{resolved}\"")
    });

    let desc_escaped = entity.description.as_deref().map(escape_description);

    if external {
        // System_Ext({alias}, "{title}", $descr="{desc}", $link="{url}")
        let desc_part = desc_escaped
            .as_deref()
            .map_or(String::new(), |d| format!(", $descr=\"{d}\""));
        format!("System_Ext({alias}, \"{title}\"{desc_part}{link_part})")
    } else {
        // Regular macros vary by entity type
        match entity_type {
            "domain" => {
                // System({alias}, "{title}", $tags="domain", "{desc}", $link="{url}")
                let desc_part = desc_escaped
                    .as_deref()
                    .map_or(String::new(), |d| format!(", \"{d}\""));
                format!("System({alias}, \"{title}\", $tags=\"domain\"{desc_part}{link_part})")
            }
            "system" => {
                // System({alias}, "{title}", "{desc}", $link="{url}")
                let desc_part = desc_escaped
                    .as_deref()
                    .map_or(String::new(), |d| format!(", \"{d}\""));
                format!("System({alias}, \"{title}\"{desc_part}{link_part})")
            }
            "service" => {
                // System({alias}, "{dir_name}", $tags="service", $descr="{desc}", $link="{url}")
                let desc_part = desc_escaped
                    .as_deref()
                    .map_or(String::new(), |d| format!(", $descr=\"{d}\""));
                format!("System({alias}, \"{title}\", $tags=\"service\"{desc_part}{link_part})")
            }
            _ => format!("System({alias}, \"{title}\"{link_part})"),
        }
    }
}

/// Resolve a `PlantUML` `!include` path to a C4 macro using page metadata.
///
/// This is the main public entry point. It parses the include path, looks up
/// the entity via the provided [`MetaIncludeSource`], and renders the
/// appropriate C4 `PlantUML` macro.
///
/// When `link_config` is provided, `$link` URLs are transformed to match the
/// renderer's link mode (relative paths, trailing slashes).
///
/// Returns `None` if the path doesn't match the meta include pattern or the
/// entity is not found.
pub fn resolve_meta_include(
    include_path: &str,
    source: &dyn MetaIncludeSource,
    link_config: Option<&LinkConfig>,
) -> Option<String> {
    let parsed = parse_include_path(include_path)?;
    let entity = source.get_entity(parsed.entity_type, &parsed.name)?;
    Some(render_c4_macro(
        parsed.entity_type,
        &parsed.name,
        &entity,
        parsed.external,
        link_config,
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    // ── Test helpers ──────────────────────────────────────────────────

    fn system_entity() -> EntityInfo {
        EntityInfo {
            title: "Payment Gateway".to_owned(),
            description: Some("Processes payments".to_owned()),
            url_path: Some("/domains/billing/systems/payment-gateway".to_owned()),
        }
    }

    fn domain_entity() -> EntityInfo {
        EntityInfo {
            title: "Billing".to_owned(),
            description: Some("Billing services".to_owned()),
            url_path: Some("/domains/billing".to_owned()),
        }
    }

    fn service_entity() -> EntityInfo {
        EntityInfo {
            title: "invoice-api".to_owned(),
            description: Some("Manages invoices".to_owned()),
            url_path: Some("/domains/billing/systems/invoicing/services/invoice-api".to_owned()),
        }
    }

    struct TestSource {
        entities: HashMap<(String, String), EntityInfo>,
    }

    impl TestSource {
        fn new() -> Self {
            Self {
                entities: HashMap::new(),
            }
        }

        fn with_entity(mut self, entity_type: &str, name: &str, entity: EntityInfo) -> Self {
            self.entities
                .insert((entity_type.to_owned(), name.to_owned()), entity);
            self
        }
    }

    impl MetaIncludeSource for TestSource {
        fn get_entity(&self, entity_type: &str, name: &str) -> Option<EntityInfo> {
            self.entities
                .get(&(entity_type.to_owned(), name.to_owned()))
                .cloned()
        }
    }

    // ── Task 2: parse_include_path tests ─────────────────────────────

    #[test]
    fn test_parse_system_regular() {
        let result = parse_include_path("systems/sys_payment_gateway.iuml").unwrap();
        assert_eq!(
            result,
            ParsedIncludePath {
                entity_type: "system",
                name: "payment_gateway".to_owned(),
                external: false,
            }
        );
    }

    #[test]
    fn test_parse_system_external() {
        let result = parse_include_path("systems/ext/sys_payment_gateway.iuml").unwrap();
        assert_eq!(
            result,
            ParsedIncludePath {
                entity_type: "system",
                name: "payment_gateway".to_owned(),
                external: true,
            }
        );
    }

    #[test]
    fn test_parse_domain_regular() {
        let result = parse_include_path("systems/dmn_billing.iuml").unwrap();
        assert_eq!(
            result,
            ParsedIncludePath {
                entity_type: "domain",
                name: "billing".to_owned(),
                external: false,
            }
        );
    }

    #[test]
    fn test_parse_domain_external() {
        let result = parse_include_path("systems/ext/dmn_billing.iuml").unwrap();
        assert_eq!(
            result,
            ParsedIncludePath {
                entity_type: "domain",
                name: "billing".to_owned(),
                external: true,
            }
        );
    }

    #[test]
    fn test_parse_service_regular() {
        let result = parse_include_path("systems/svc_invoice_api.iuml").unwrap();
        assert_eq!(
            result,
            ParsedIncludePath {
                entity_type: "service",
                name: "invoice_api".to_owned(),
                external: false,
            }
        );
    }

    #[test]
    fn test_parse_service_external() {
        let result = parse_include_path("systems/ext/svc_invoice_api.iuml").unwrap();
        assert_eq!(
            result,
            ParsedIncludePath {
                entity_type: "service",
                name: "invoice_api".to_owned(),
                external: true,
            }
        );
    }

    #[test]
    fn test_parse_unrelated_path_returns_none() {
        assert!(parse_include_path("c4/context.iuml").is_none());
        assert!(parse_include_path("actors/customer.iuml").is_none());
        assert!(parse_include_path("random.iuml").is_none());
    }

    #[test]
    fn test_parse_wrong_extension_returns_none() {
        assert!(parse_include_path("systems/sys_foo.puml").is_none());
    }

    #[test]
    fn test_parse_unknown_prefix_returns_none() {
        assert!(parse_include_path("systems/xyz_foo.iuml").is_none());
    }

    // ── Task 3: render_c4_macro tests ────────────────────────────────

    #[test]
    fn test_render_system_regular() {
        let entity = system_entity();
        let result = render_c4_macro("system", "payment_gateway", &entity, false, None);
        assert_eq!(
            result,
            "System(sys_payment_gateway, \"Payment Gateway\", \"Processes payments\", $link=\"/domains/billing/systems/payment-gateway\")"
        );
    }

    #[test]
    fn test_render_system_external() {
        let entity = system_entity();
        let result = render_c4_macro("system", "payment_gateway", &entity, true, None);
        assert_eq!(
            result,
            "System_Ext(sys_payment_gateway, \"Payment Gateway\", $descr=\"Processes payments\", $link=\"/domains/billing/systems/payment-gateway\")"
        );
    }

    #[test]
    fn test_render_domain_regular() {
        let entity = domain_entity();
        let result = render_c4_macro("domain", "billing", &entity, false, None);
        assert_eq!(
            result,
            "System(dmn_billing, \"Billing\", $tags=\"domain\", \"Billing services\", $link=\"/domains/billing\")"
        );
    }

    #[test]
    fn test_render_domain_external() {
        let entity = domain_entity();
        let result = render_c4_macro("domain", "billing", &entity, true, None);
        assert_eq!(
            result,
            "System_Ext(dmn_billing, \"Billing\", $descr=\"Billing services\", $link=\"/domains/billing\")"
        );
    }

    #[test]
    fn test_render_service_regular() {
        let entity = service_entity();
        let result = render_c4_macro("service", "invoice_api", &entity, false, None);
        assert_eq!(
            result,
            "System(svc_invoice_api, \"invoice-api\", $tags=\"service\", $descr=\"Manages invoices\", $link=\"/domains/billing/systems/invoicing/services/invoice-api\")"
        );
    }

    #[test]
    fn test_render_service_external() {
        let entity = service_entity();
        let result = render_c4_macro("service", "invoice_api", &entity, true, None);
        assert_eq!(
            result,
            "System_Ext(svc_invoice_api, \"invoice-api\", $descr=\"Manages invoices\", $link=\"/domains/billing/systems/invoicing/services/invoice-api\")"
        );
    }

    #[test]
    fn test_render_no_description() {
        let entity = EntityInfo {
            title: "Simple".to_owned(),
            description: None,
            url_path: Some("/simple".to_owned()),
        };
        let result = render_c4_macro("system", "simple", &entity, false, None);
        assert_eq!(result, "System(sys_simple, \"Simple\", $link=\"/simple\")");
    }

    #[test]
    fn test_render_no_url_omits_link() {
        let entity = EntityInfo {
            title: "No Docs".to_owned(),
            description: Some("Has no docs".to_owned()),
            url_path: None,
        };
        let result = render_c4_macro("system", "no_docs", &entity, false, None);
        assert_eq!(result, "System(sys_no_docs, \"No Docs\", \"Has no docs\")");
        assert!(!result.contains("$link"));
    }

    #[test]
    fn test_render_description_newlines_escaped() {
        let entity = EntityInfo {
            title: "Multi".to_owned(),
            description: Some("Line one\nLine two".to_owned()),
            url_path: Some("/multi".to_owned()),
        };
        let result = render_c4_macro("system", "multi", &entity, false, None);
        assert!(result.contains("Line one\\nLine two"));
        assert!(!result.contains('\n'));
    }

    #[test]
    fn test_render_description_quotes_escaped() {
        let entity = EntityInfo {
            title: "Quoted".to_owned(),
            description: Some("He said \"hello\"".to_owned()),
            url_path: Some("/quoted".to_owned()),
        };
        let result = render_c4_macro("system", "quoted", &entity, false, None);
        assert!(result.contains(r#"He said \"hello\""#));
    }

    #[test]
    fn test_render_description_backslashes_escaped() {
        let entity = EntityInfo {
            title: "Paths".to_owned(),
            description: Some(r"C:\Users\docs".to_owned()),
            url_path: Some("/paths".to_owned()),
        };
        let result = render_c4_macro("system", "paths", &entity, false, None);
        assert!(result.contains(r"C:\\Users\\docs"));
    }

    // ── Task 4: resolve_meta_include tests ───────────────────────────

    #[test]
    fn test_resolve_meta_include_system() {
        let source = TestSource::new().with_entity("system", "payment_gateway", system_entity());
        let result =
            resolve_meta_include("systems/sys_payment_gateway.iuml", &source, None).unwrap();
        assert!(result.contains("System(sys_payment_gateway"));
    }

    #[test]
    fn test_resolve_meta_include_external() {
        let source = TestSource::new().with_entity("system", "payment_gateway", system_entity());
        let result =
            resolve_meta_include("systems/ext/sys_payment_gateway.iuml", &source, None).unwrap();
        assert!(result.contains("System_Ext"));
    }

    #[test]
    fn test_resolve_meta_include_not_found() {
        let source = TestSource::new();
        let result = resolve_meta_include("systems/sys_unknown.iuml", &source, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_meta_include_non_meta_path() {
        let source = TestSource::new().with_entity("system", "payment_gateway", system_entity());
        let result = resolve_meta_include("c4/context.iuml", &source, None);
        assert!(result.is_none());
    }

    // ── LinkConfig tests ─────────────────────────────────────────────

    #[test]
    fn test_render_with_trailing_slash() {
        let entity = system_entity();
        let config = LinkConfig {
            base_path: "/overview".to_owned(),
            relative_links: false,
            trailing_slash: true,
        };
        let result = render_c4_macro("system", "payment_gateway", &entity, false, Some(&config));
        assert!(result.contains("$link=\"/domains/billing/systems/payment-gateway/\""));
    }

    #[test]
    fn test_render_with_relative_links() {
        let entity = system_entity();
        let config = LinkConfig {
            base_path: "/domains/payments".to_owned(),
            relative_links: true,
            trailing_slash: false,
        };
        let result = render_c4_macro("system", "payment_gateway", &entity, false, Some(&config));
        assert!(result.contains("$link=\"billing/systems/payment-gateway\""));
    }

    #[test]
    fn test_render_with_relative_links_and_trailing_slash() {
        let entity = system_entity();
        let config = LinkConfig {
            base_path: "/domains/payments".to_owned(),
            relative_links: true,
            trailing_slash: true,
        };
        let result = render_c4_macro("system", "payment_gateway", &entity, false, Some(&config));
        assert!(result.contains("$link=\"../billing/systems/payment-gateway/\""));
    }

    #[test]
    fn test_render_no_url_with_link_config() {
        let entity = EntityInfo {
            title: "Virtual".to_owned(),
            description: None,
            url_path: None,
        };
        let config = LinkConfig {
            base_path: "/overview".to_owned(),
            relative_links: true,
            trailing_slash: true,
        };
        let result = render_c4_macro("system", "virtual", &entity, false, Some(&config));
        assert!(!result.contains("$link"));
    }
}
