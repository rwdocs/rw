//! Meta include resolution for `PlantUML` diagrams.
//!
//! Generates C4 model `PlantUML` macros from page metadata, enabling
//! `!include systems/sys_payment_gateway.iuml` to resolve dynamically
//! from `meta.yaml` files without maintaining separate `.iuml` files.

use std::fmt;

use rw_diagrams::{Entity, SiteModel};

/// Check if a path matches the meta include pattern (e.g., `systems/sys_*.iuml`).
pub(crate) fn is_meta_include_pattern(path: &str) -> bool {
    parse_include_path(path).is_some()
}

/// The section kinds a meta include can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    System,
    Domain,
    Service,
}

impl Kind {
    const ALL: [Self; 3] = [Self::System, Self::Domain, Self::Service];

    /// The include filename prefix, which is also the C4 alias prefix.
    fn prefix(self) -> &'static str {
        match self {
            Self::System => "sys",
            Self::Domain => "dmn",
            Self::Service => "svc",
        }
    }
}

/// The section kind the site model looks entities up by.
impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::System => "system",
            Self::Domain => "domain",
            Self::Service => "service",
        })
    }
}

/// Parsed components of a meta include path.
#[derive(Debug, PartialEq, Eq)]
struct ParsedIncludePath {
    kind: Kind,
    /// Name as the include filename spells it, with underscores (e.g.
    /// `payment_gateway`). The C4 alias is built from this form; the section
    /// lookup needs it hyphenated.
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
    let (kind, name) = Kind::ALL.into_iter().find_map(|kind| {
        let name = stem.strip_prefix(kind.prefix())?.strip_prefix('_')?;
        Some((kind, name))
    })?;
    if name.is_empty() {
        return None;
    }
    Some(ParsedIncludePath {
        kind,
        name: name.to_owned(),
        external,
    })
}

/// Escape text for a quoted C4 macro argument.
///
/// C4 macros are `!unquoted` procedures, which reject `\"` outright, so a
/// quote travels as the `&#34;` entity `PlantUML` decodes when it draws the
/// label. Backslashes are doubled so a literal `\n` stays text.
fn escape_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "&#34;")
        .replace('\n', "\\n")
}

/// Render a C4 `PlantUML` macro call from entity metadata.
///
/// The output follows the C4-PlantUML conventions used by the mkdocs arch
/// plugin.
fn render_c4_macro(kind: Kind, name: &str, entity: &Entity, external: bool) -> String {
    let (macro_name, tags, descr_key) = match (external, kind) {
        (true, _) => ("System_Ext", "", "$descr="),
        (false, Kind::System) => ("System", "", ""),
        (false, Kind::Domain) => ("System", ", $tags=\"domain\"", ""),
        (false, Kind::Service) => ("System", ", $tags=\"service\"", "$descr="),
    };
    let prefix = kind.prefix();
    let title = escape_text(&entity.title);
    let descr = entity.description.as_deref().map_or(String::new(), |d| {
        format!(", {descr_key}\"{}\"", escape_text(d))
    });
    // `PlantUML` decodes entities in labels, not in links, and a bracket in a
    // link breaks its `[[…]]` syntax.
    let link = entity.url_path.as_deref().map_or(String::new(), |url| {
        let url = url
            .replace('"', "%22")
            .replace('[', "%5B")
            .replace(']', "%5D");
        format!(", $link=\"{url}\"")
    });
    format!("{macro_name}({prefix}_{name}, \"{title}\"{tags}{descr}{link})")
}

/// Resolve a `PlantUML` `!include` path to a C4 macro using page metadata.
///
/// This is the main public entry point. It parses the include path, looks up
/// the entity via the provided [`SiteModel`], and renders the
/// appropriate C4 `PlantUML` macro.
///
/// Returns `None` if the path doesn't match the meta include pattern or the
/// entity is not found.
pub(crate) fn resolve_meta_include(include_path: &str, source: &dyn SiteModel) -> Option<String> {
    let parsed = parse_include_path(include_path)?;
    // Include filenames spell names with underscores, section names with
    // hyphens. Translate for the lookup only: the alias `render_c4_macro`
    // builds stays in the underscore form the C4 convention uses
    // (`sys_payment_gateway`). Do NOT move this into `parse_include_path` —
    // that feeds the alias too, and would change every emitted macro.
    let entity = source.entity(&parsed.kind.to_string(), &parsed.name.replace('_', "-"))?;
    Some(render_c4_macro(
        parsed.kind,
        &parsed.name,
        &entity,
        parsed.external,
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    // ── Test helpers ──────────────────────────────────────────────────

    fn system_entity() -> Entity {
        Entity {
            title: "Payment Gateway".to_owned(),
            description: Some("Processes payments".to_owned()),
            url_path: Some("/domains/billing/systems/payment-gateway".to_owned()),
        }
    }

    fn domain_entity() -> Entity {
        Entity {
            title: "Billing".to_owned(),
            description: Some("Billing services".to_owned()),
            url_path: Some("/domains/billing".to_owned()),
        }
    }

    fn service_entity() -> Entity {
        Entity {
            title: "invoice-api".to_owned(),
            description: Some("Manages invoices".to_owned()),
            url_path: Some("/domains/billing/systems/invoicing/services/invoice-api".to_owned()),
        }
    }

    struct TestSource {
        entities: HashMap<(String, String), Entity>,
    }

    impl TestSource {
        fn new() -> Self {
            Self {
                entities: HashMap::new(),
            }
        }

        fn with_entity(mut self, kind: &str, name: &str, entity: Entity) -> Self {
            self.entities
                .insert((kind.to_owned(), name.to_owned()), entity);
            self
        }
    }

    impl SiteModel for TestSource {
        fn entity(&self, kind: &str, name: &str) -> Option<Entity> {
            self.entities
                .get(&(kind.to_owned(), name.to_owned()))
                .cloned()
        }
    }

    // ── parse_include_path ──

    #[test]
    fn test_parse_system_regular() {
        let result = parse_include_path("systems/sys_payment_gateway.iuml").unwrap();
        assert_eq!(
            result,
            ParsedIncludePath {
                kind: Kind::System,
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
                kind: Kind::System,
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
                kind: Kind::Domain,
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
                kind: Kind::Domain,
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
                kind: Kind::Service,
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
                kind: Kind::Service,
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

    // ── render_c4_macro ──

    #[test]
    fn test_render_system_regular() {
        let entity = system_entity();
        let result = render_c4_macro(Kind::System, "payment_gateway", &entity, false);
        assert_eq!(
            result,
            "System(sys_payment_gateway, \"Payment Gateway\", \"Processes payments\", $link=\"/domains/billing/systems/payment-gateway\")"
        );
    }

    #[test]
    fn test_render_system_external() {
        let entity = system_entity();
        let result = render_c4_macro(Kind::System, "payment_gateway", &entity, true);
        assert_eq!(
            result,
            "System_Ext(sys_payment_gateway, \"Payment Gateway\", $descr=\"Processes payments\", $link=\"/domains/billing/systems/payment-gateway\")"
        );
    }

    #[test]
    fn test_render_domain_regular() {
        let entity = domain_entity();
        let result = render_c4_macro(Kind::Domain, "billing", &entity, false);
        assert_eq!(
            result,
            "System(dmn_billing, \"Billing\", $tags=\"domain\", \"Billing services\", $link=\"/domains/billing\")"
        );
    }

    #[test]
    fn test_render_domain_external() {
        let entity = domain_entity();
        let result = render_c4_macro(Kind::Domain, "billing", &entity, true);
        assert_eq!(
            result,
            "System_Ext(dmn_billing, \"Billing\", $descr=\"Billing services\", $link=\"/domains/billing\")"
        );
    }

    #[test]
    fn test_render_service_regular() {
        let entity = service_entity();
        let result = render_c4_macro(Kind::Service, "invoice_api", &entity, false);
        assert_eq!(
            result,
            "System(svc_invoice_api, \"invoice-api\", $tags=\"service\", $descr=\"Manages invoices\", $link=\"/domains/billing/systems/invoicing/services/invoice-api\")"
        );
    }

    #[test]
    fn test_render_service_external() {
        let entity = service_entity();
        let result = render_c4_macro(Kind::Service, "invoice_api", &entity, true);
        assert_eq!(
            result,
            "System_Ext(svc_invoice_api, \"invoice-api\", $descr=\"Manages invoices\", $link=\"/domains/billing/systems/invoicing/services/invoice-api\")"
        );
    }

    #[test]
    fn test_render_no_description() {
        let entity = Entity {
            title: "Simple".to_owned(),
            description: None,
            url_path: Some("/simple".to_owned()),
        };
        let result = render_c4_macro(Kind::System, "simple", &entity, false);
        assert_eq!(result, "System(sys_simple, \"Simple\", $link=\"/simple\")");
    }

    #[test]
    fn test_render_no_url_omits_link() {
        let entity = Entity {
            title: "No Docs".to_owned(),
            description: Some("Has no docs".to_owned()),
            url_path: None,
        };
        let result = render_c4_macro(Kind::System, "no_docs", &entity, false);
        assert_eq!(result, "System(sys_no_docs, \"No Docs\", \"Has no docs\")");
        assert!(!result.contains("$link"));
    }

    #[test]
    fn test_render_description_newlines_escaped() {
        let entity = Entity {
            title: "Multi".to_owned(),
            description: Some("Line one\nLine two".to_owned()),
            url_path: Some("/multi".to_owned()),
        };
        let result = render_c4_macro(Kind::System, "multi", &entity, false);
        assert!(result.contains("Line one\\nLine two"));
        assert!(!result.contains('\n'));
    }

    #[test]
    fn test_render_description_quotes_escaped() {
        let entity = Entity {
            title: "Quoted".to_owned(),
            description: Some("He said \"hello\"".to_owned()),
            url_path: Some("/quoted".to_owned()),
        };
        let result = render_c4_macro(Kind::System, "quoted", &entity, false);
        assert!(result.contains("He said &#34;hello&#34;"), "{result}");
    }

    #[test]
    fn test_render_title_and_link_quotes_escaped() {
        let entity = Entity {
            title: "The \"Core\" API".to_owned(),
            description: None,
            url_path: Some("/systems/a\"b".to_owned()),
        };
        let result = render_c4_macro(Kind::System, "core", &entity, true);
        assert_eq!(
            result,
            "System_Ext(sys_core, \"The &#34;Core&#34; API\", $link=\"/systems/a%22b\")"
        );
    }

    #[test]
    fn test_render_link_brackets_escaped() {
        let entity = Entity {
            title: "Core".to_owned(),
            description: None,
            url_path: Some("/systems/a[b]".to_owned()),
        };
        let result = render_c4_macro(Kind::System, "core", &entity, false);
        assert_eq!(
            result,
            "System(sys_core, \"Core\", $link=\"/systems/a%5Bb%5D\")"
        );
    }

    #[test]
    fn test_render_description_backslashes_escaped() {
        let entity = Entity {
            title: "Paths".to_owned(),
            description: Some(r"C:\Users\docs".to_owned()),
            url_path: Some("/paths".to_owned()),
        };
        let result = render_c4_macro(Kind::System, "paths", &entity, false);
        assert!(result.contains(r"C:\\Users\\docs"));
    }

    // ── resolve_meta_include ──

    #[test]
    fn test_resolve_meta_include_system() {
        let source = TestSource::new().with_entity("system", "payment-gateway", system_entity());
        let result = resolve_meta_include("systems/sys_payment_gateway.iuml", &source).unwrap();
        assert!(result.contains("System(sys_payment_gateway"));
    }

    #[test]
    fn test_resolve_meta_include_looks_up_each_kind_by_its_section_kind() {
        let source = TestSource::new()
            .with_entity("domain", "billing", domain_entity())
            .with_entity("service", "invoice-api", service_entity());
        let domain = resolve_meta_include("systems/dmn_billing.iuml", &source).unwrap();
        assert!(domain.starts_with("System(dmn_billing,"), "{domain}");
        let service = resolve_meta_include("systems/svc_invoice_api.iuml", &source).unwrap();
        assert!(service.starts_with("System(svc_invoice_api,"), "{service}");
    }

    #[test]
    fn test_resolve_meta_include_external() {
        let source = TestSource::new().with_entity("system", "payment-gateway", system_entity());
        let result = resolve_meta_include("systems/ext/sys_payment_gateway.iuml", &source).unwrap();
        assert!(result.contains("System_Ext"));
    }

    #[test]
    fn test_resolve_meta_include_not_found() {
        let source = TestSource::new();
        let result = resolve_meta_include("systems/sys_unknown.iuml", &source);
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_meta_include_non_meta_path() {
        let source = TestSource::new().with_entity("system", "payment-gateway", system_entity());
        let result = resolve_meta_include("c4/context.iuml", &source);
        assert!(result.is_none());
    }
}
