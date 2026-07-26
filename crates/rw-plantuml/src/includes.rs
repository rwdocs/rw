//! `PlantUML` `!include` resolution and render-config injection.
//!
//! Resolves `!include` directives against the configured include directories
//! and the site model. On top of that, [`prepare_diagram_source`] injects the
//! caller's DPI plus the font settings a render needs, while [`bundle_source`]
//! only resolves. Stdlib includes (`!include <tupadr3/common>`) are left alone.

use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;
use rw_diagrams::SiteModel;

use crate::is_plantuml_fence;
use crate::meta_includes::{is_meta_include_pattern, resolve_meta_include};

static INCLUDE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^([ \t]*)!include\s+(.+)$").unwrap());

/// Indent content with the given whitespace prefix, preserving empty lines.
fn indent_content(content: &str, indent: &str) -> String {
    if indent.is_empty() {
        return content.to_owned();
    }
    content
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Result of preparing diagram source with potential warnings.
#[derive(Debug)]
pub struct PrepareResult {
    /// Prepared diagram source.
    pub source: String,
    /// Warnings generated during preparation (e.g., unresolved includes).
    pub warnings: Vec<String>,
}

/// Resolve `PlantUML` `!include` directives in diagram source.
///
/// Include paths are searched in `include_dirs` order. When `meta_source` is
/// given, meta includes (`systems/sys_payment_gateway.iuml`) are looked up
/// there first; a miss falls back to the include directories like any other
/// path. An include that resolves nowhere is left in place and a message is
/// pushed onto `warnings` — except stdlib includes (`<tupadr3/common>`),
/// and, when `meta_source` is `None`, meta-pattern paths: those are left in
/// place silently, for a later pass that has a model to expand them.
#[must_use]
pub fn resolve_includes(
    source: &str,
    include_dirs: &[PathBuf],
    meta_source: Option<&dyn SiteModel>,
    warnings: &mut Vec<String>,
) -> String {
    resolve_includes_at(source, include_dirs, meta_source, 0, warnings)
}

/// Resolve includes `depth` levels below the fence.
///
/// There is no cycle detection: `depth > 10` is a fixed cap that stops
/// runaway or cyclic includes after ten levels, leaving that level's
/// `!include` lines unexpanded and a warning behind.
fn resolve_includes_at(
    source: &str,
    include_dirs: &[PathBuf],
    meta_source: Option<&dyn SiteModel>,
    depth: usize,
    warnings: &mut Vec<String>,
) -> String {
    if depth > 10 {
        warnings.push("Include depth exceeded maximum of 10".to_owned());
        return source.to_owned();
    }

    let mut result = source.to_owned();

    for caps in INCLUDE_PATTERN.captures_iter(source) {
        let leading_whitespace = caps.get(1).unwrap().as_str();
        let include_path = caps.get(2).unwrap().as_str().trim();
        let full_match = caps.get(0).unwrap().as_str();

        // Skip stdlib includes
        if include_path.starts_with('<') && include_path.ends_with('>') {
            continue;
        }

        // Try meta include source first
        if let Some(meta) = meta_source
            && let Some(content) = resolve_meta_include(include_path, meta)
        {
            let indented_content = indent_content(&content, leading_whitespace);
            result = result.replace(full_match, &indented_content);
            continue;
        }

        // Try to resolve from include directories
        let mut resolved = false;
        for dir in include_dirs {
            let full_path = dir.join(include_path);
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                let resolved_content =
                    resolve_includes_at(&content, include_dirs, meta_source, depth + 1, warnings);
                // Indent included content to match the !include directive
                let indented_content = indent_content(&resolved_content, leading_whitespace);
                result = result.replace(full_match, &indented_content);
                resolved = true;
                break;
            }
        }

        if !resolved {
            // Skip warning for meta-pattern includes (systems/sys_*.iuml etc.)
            // when no meta source is available — they'll be resolved at request time.
            if meta_source.is_none() && is_meta_include_pattern(include_path) {
                continue;
            }

            let searched_paths: Vec<_> = include_dirs
                .iter()
                .map(|d| d.join(include_path).display().to_string())
                .collect();
            if searched_paths.is_empty() {
                warnings.push(format!(
                    "Include file not found: '{include_path}' (no include directories configured)"
                ));
            } else {
                warnings.push(format!(
                    "Include file not found: '{}' (searched: {})",
                    include_path,
                    searched_paths.join(", ")
                ));
            }
        }
    }

    result
}

/// Prepare `PlantUML` source for rendering.
///
/// Resolves includes and injects DPI and font settings
/// after the `@startuml` directive.
///
/// # Arguments
/// * `source` - Raw `PlantUML` diagram source
/// * `include_dirs` - Directories to search for `!include` files
/// * `dpi` - DPI setting for rendering
/// * `meta_source` - Site model that meta includes
///   (`systems/sys_payment_gateway.iuml`) expand from; `None` leaves those
///   includes in place, unexpanded and unwarned
///
/// # Returns
/// [`PrepareResult`] containing the prepared source and any warnings.
#[must_use]
pub fn prepare_diagram_source(
    source: &str,
    include_dirs: &[PathBuf],
    dpi: u32,
    meta_source: Option<&dyn SiteModel>,
) -> PrepareResult {
    let mut warnings = Vec::new();
    let resolved = resolve_includes(source, include_dirs, meta_source, &mut warnings);

    // Inject DPI and font config after @startuml directive
    let config_block = format!(
        "skinparam dpi {dpi}\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\n"
    );

    // Find @startuml and inject config after it
    let final_source = if let Some(pos) = resolved.find("@startuml") {
        // Find the end of the @startuml line
        let after_startuml = &resolved[pos..];
        if let Some(newline_pos) = after_startuml.find('\n') {
            let insert_pos = pos + newline_pos + 1;
            let mut result = String::with_capacity(resolved.len() + config_block.len());
            result.push_str(&resolved[..insert_pos]);
            result.push_str(&config_block);
            result.push_str(&resolved[insert_pos..]);
            result
        } else {
            // Fallback: just prepend (may not work for all diagrams)
            format!("{config_block}{resolved}")
        }
    } else {
        // Fallback: just prepend (may not work for all diagrams)
        format!("{config_block}{resolved}")
    };

    PrepareResult {
        source: final_source,
        warnings,
    }
}

/// Inline `PlantUML` `!include` directives in a fence's source, returning
/// `None` when there is nothing to change.
///
/// `None` covers fences outside the `PlantUML` family and sources whose
/// includes all failed to resolve; in both cases the fence is left as the
/// author wrote it. Failures are appended to `warnings`.
///
/// Meta includes are deliberately left unresolved: they expand at request time
/// from live page metadata, which a published bundle does not carry.
#[must_use]
pub fn bundle_source(
    language: &str,
    source: &str,
    include_dirs: &[PathBuf],
    warnings: &mut Vec<String>,
) -> Option<String> {
    if !is_plantuml_fence(language) {
        return None;
    }
    let resolved = resolve_includes(source, include_dirs, None, warnings);
    (resolved != source).then_some(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the DPI the diagram render path passes in practice. The
    /// expectations below spell `dpi 192` out literally, so this and those have
    /// to change together.
    const DEFAULT_DPI: u32 = 192;

    #[test]
    fn test_prepare_diagram_source() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, None);

        // DPI and font should be injected after @startuml
        assert_eq!(
            result.source,
            "@startuml\nskinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_prepare_diagram_source_custom_dpi() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], 300, None);

        assert_eq!(
            result.source,
            "@startuml\nskinparam dpi 300\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_prepare_diagram_source_preserves_content_before_startuml() {
        let source = "' comment\n@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, None);

        // Content before @startuml should be preserved
        assert_eq!(
            result.source,
            "' comment\n@startuml\nskinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_unresolved_include_generates_warning() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, None);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("not found"));
    }

    #[test]
    fn test_unresolved_include_with_dirs_shows_searched_paths() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let include_dirs = vec![PathBuf::from("/tmp/includes")];
        let result = prepare_diagram_source(source, &include_dirs, DEFAULT_DPI, None);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("/tmp/includes"));
    }

    #[test]
    fn test_stdlib_include_no_warning() {
        let source = "@startuml\n!include <tupadr3/common>\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, None);

        // Stdlib includes should not generate warnings
        assert!(result.warnings.is_empty());
        // Stdlib include should be preserved as-is
        assert!(result.source.contains("!include <tupadr3/common>"));
    }

    #[test]
    fn test_indented_include_resolved() {
        // Create a temp file for the include
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("test_component.iuml");
        std::fs::write(&include_path, "Component(comp, \"Component\")").unwrap();

        let source = "@startuml\nSystem_Boundary(sys, \"System\")\n  !include test_component.iuml\nBoundary_End()\n@enduml";
        let result =
            prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], DEFAULT_DPI, None);

        assert!(result.warnings.is_empty());
        // Indented include should be resolved and content should be indented
        assert!(result.source.contains("  Component(comp, \"Component\")"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_indented_include_warning() {
        let source = "@startuml\nSystem_Boundary(sys, \"System\")\n  !include missing.iuml\nBoundary_End()\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, None);

        // Should generate warning for indented include too
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
    }

    #[test]
    fn test_prepare_diagram_source_no_startuml() {
        // Source without @startuml - fallback to prepending config
        let source = "Alice -> Bob";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, None);

        // Config should be prepended
        assert!(result.source.starts_with("skinparam dpi 192\n"));
        assert!(result.source.contains("skinparam defaultFontName Roboto"));
        assert!(result.source.contains("Alice -> Bob"));
    }

    #[test]
    fn test_prepare_diagram_source_startuml_no_newline() {
        // @startuml at end of source without newline
        let source = "@startuml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, None);

        // Should fallback to prepending
        assert!(result.source.contains("skinparam dpi 192"));
        assert!(result.source.contains("skinparam defaultFontName Roboto"));
        assert!(result.source.contains("@startuml"));
    }

    #[test]
    fn test_include_depth_exceeded() {
        // Create a self-referencing include to trigger depth limit
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("recursive.iuml");
        // File includes itself
        std::fs::write(&include_path, "!include recursive.iuml\nContent").unwrap();

        let source = "@startuml\n!include recursive.iuml\n@enduml";
        let result =
            prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], DEFAULT_DPI, None);

        // Should have warning about depth exceeded
        assert!(result.warnings.iter().any(|w| w.contains("depth exceeded")));
    }

    #[test]
    fn test_multiple_includes_resolved() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include1 = temp_dir.path().join("part1.iuml");
        let include2 = temp_dir.path().join("part2.iuml");
        std::fs::write(&include1, "Alice -> Bob").unwrap();
        std::fs::write(&include2, "Bob -> Charlie").unwrap();

        let source = "@startuml\n!include part1.iuml\n!include part2.iuml\n@enduml";
        let result =
            prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], DEFAULT_DPI, None);

        assert!(result.warnings.is_empty());
        assert!(result.source.contains("Alice -> Bob"));
        assert!(result.source.contains("Bob -> Charlie"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_nested_includes() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let outer = temp_dir.path().join("outer.iuml");
        let inner = temp_dir.path().join("inner.iuml");
        std::fs::write(&inner, "InnerContent").unwrap();
        std::fs::write(&outer, "OuterBefore\n!include inner.iuml\nOuterAfter").unwrap();

        let source = "@startuml\n!include outer.iuml\n@enduml";
        let result =
            prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], DEFAULT_DPI, None);

        assert!(result.warnings.is_empty());
        assert!(result.source.contains("OuterBefore"));
        assert!(result.source.contains("InnerContent"));
        assert!(result.source.contains("OuterAfter"));
    }

    #[test]
    fn test_indented_content_empty_lines_preserved() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("with_empty.iuml");
        std::fs::write(&include_path, "Line1\n\nLine3").unwrap();

        let source = "@startuml\n  !include with_empty.iuml\n@enduml";
        let result =
            prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], DEFAULT_DPI, None);

        assert!(result.warnings.is_empty());
        // Empty lines should remain empty (not indented)
        assert!(result.source.contains("  Line1\n\n  Line3"));
    }

    #[test]
    fn test_include_after_blank_line_keeps_line_structure() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("after_blank_line.iuml");
        std::fs::write(&include_path, "line1\nline2\nline3").unwrap();

        let source = "@startuml\n\n!include after_blank_line.iuml\n@enduml";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
        );

        assert!(warnings.is_empty());
        assert_eq!(result, "@startuml\n\nline1\nline2\nline3\n@enduml");
    }

    #[test]
    fn test_indented_include_after_blank_line_indents_without_injecting_blanks() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("indented_after_blank.iuml");
        std::fs::write(&include_path, "line1\nline2").unwrap();

        let source = "@startuml\n\n  !include indented_after_blank.iuml\n@enduml";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
        );

        assert!(warnings.is_empty());
        assert_eq!(result, "@startuml\n\n  line1\n  line2\n@enduml");
    }

    #[test]
    fn test_crlf_include_after_blank_line_not_indented_by_carriage_return() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("crlf_after_blank.iuml");
        std::fs::write(&include_path, "line1\nline2").unwrap();

        let source = "@startuml\r\n\r\n!include crlf_after_blank.iuml\r\n@enduml";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
        );

        assert!(warnings.is_empty());
        // Characterizes CRLF handling; it cannot fail independently of the LF
        // cases above, since both corruptions came from `\n` being in the
        // capture's character class. Kept because this is the crate's only
        // CRLF coverage, and because it pins a pre-existing wart: `(.+)$`
        // swallows the include line's `\r` and the replacement never restores
        // it, so `@enduml` below arrives with a bare `\n`. A positional-splice
        // rewrite of the `String::replace` calls would change that byte —
        // deliberately, not silently.
        assert_eq!(result, "@startuml\r\n\r\nline1\nline2\n@enduml");
    }

    use rw_diagrams::{Entity, SiteModel};

    struct TestMetaSource;

    impl SiteModel for TestMetaSource {
        fn entity(&self, kind: &str, name: &str) -> Option<Entity> {
            if kind == "system" && name == "payment-gateway" {
                Some(Entity {
                    title: "Payment Gateway".to_owned(),
                    description: Some("Processes payments".to_owned()),
                    url_path: Some("/domains/billing/systems/payment-gateway".to_owned()),
                })
            } else {
                None
            }
        }
    }

    #[test]
    fn test_meta_include_resolves_before_filesystem() {
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\nA -> B\n@enduml";
        let meta_source = TestMetaSource;
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, Some(&meta_source));
        assert!(result.warnings.is_empty());
        assert!(result.source.contains("System(sys_payment_gateway,"));
        assert!(result.source.contains("Payment Gateway"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_meta_include_falls_back_to_filesystem() {
        let source = "@startuml\n!include systems/sys_unknown.iuml\n@enduml";
        let meta_source = TestMetaSource;
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, Some(&meta_source));
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].contains("sys_unknown.iuml"));
    }

    #[test]
    fn test_no_meta_source_behaves_as_before() {
        let source = "@startuml\n!include missing.iuml\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, None);
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].contains("missing.iuml"));
    }

    #[test]
    fn test_meta_pattern_include_no_warning_without_meta_source() {
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI, None);
        assert!(
            result.warnings.is_empty(),
            "Meta-pattern includes should not warn when no meta source: {:?}",
            result.warnings
        );
        assert!(
            result
                .source
                .contains("!include systems/sys_payment_gateway.iuml")
        );
    }

    #[test]
    fn bundle_source_skips_non_plantuml_fences() {
        // Every fixture carries a resolvable `!include`, so deleting the language
        // guard makes `bundle_source` rewrite the source and return `Some` — these
        // assertions fail. Fixtures without a resolvable include would pass either
        // way and prove nothing.
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("shared.iuml"), "included content")
            .expect("write include");
        let include_dirs = [temp_dir.path().to_path_buf()];

        let mut warnings = Vec::new();
        assert_eq!(
            bundle_source(
                "mermaid",
                "graph TD\nA --> B\n!include shared.iuml",
                &include_dirs,
                &mut warnings
            ),
            None
        );
        assert_eq!(
            bundle_source(
                "rust",
                "fn main() {}\n!include shared.iuml",
                &include_dirs,
                &mut warnings
            ),
            None
        );
        assert_eq!(
            bundle_source(
                "graphviz",
                "digraph {}\n!include shared.iuml",
                &include_dirs,
                &mut warnings
            ),
            None
        );
        assert_eq!(
            bundle_source(
                "console",
                "$ cat notes.txt\n!include shared.iuml",
                &include_dirs,
                &mut warnings
            ),
            None
        );
        assert!(
            warnings.is_empty(),
            "non-PlantUML fences warn: {warnings:?}"
        );
    }

    #[test]
    fn bundle_source_returns_none_when_there_are_no_includes() {
        let mut warnings = Vec::new();
        assert_eq!(
            bundle_source(
                "plantuml",
                "@startuml\nAlice -> Bob\n@enduml",
                &[],
                &mut warnings
            ),
            None
        );
        assert_eq!(
            bundle_source(
                "c4plantuml",
                "@startuml\nPerson(user, \"User\")\n@enduml",
                &[],
                &mut warnings,
            ),
            None
        );
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn bundle_source_inlines_a_filesystem_include() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("bundle_test.iuml"), "Bob -> Charlie")
            .expect("write include");

        let mut warnings = Vec::new();
        let result = bundle_source(
            "plantuml",
            "@startuml\n!include bundle_test.iuml\n@enduml",
            &[temp_dir.path().to_path_buf()],
            &mut warnings,
        );

        assert_eq!(
            result.as_deref(),
            Some("@startuml\nBob -> Charlie\n@enduml"),
        );
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn bundle_source_accepts_the_kroki_prefixed_spelling() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp_dir.path().join("kroki_prefixed.iuml"),
            "Bob -> Charlie",
        )
        .expect("write include");

        let mut warnings = Vec::new();
        let result = bundle_source(
            "kroki-plantuml",
            "@startuml\n!include kroki_prefixed.iuml\n@enduml",
            &[temp_dir.path().to_path_buf()],
            &mut warnings,
        );

        assert_eq!(
            result.as_deref(),
            Some("@startuml\nBob -> Charlie\n@enduml"),
        );
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn bundle_source_inlines_what_it_can_and_warns_about_the_rest() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("good.iuml"), "Bob -> Charlie").expect("write include");
        let mut warnings = Vec::new();
        let result = bundle_source(
            "plantuml",
            "@startuml\n!include good.iuml\n!include missing.iuml\n@enduml",
            &[temp_dir.path().to_path_buf()],
            &mut warnings,
        );
        assert_eq!(
            result.as_deref(),
            Some("@startuml\nBob -> Charlie\n!include missing.iuml\n@enduml"),
        );
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("missing.iuml"), "{warnings:?}");
    }

    #[test]
    fn bundle_source_leaves_meta_includes_for_request_time() {
        struct OneSystem;
        impl SiteModel for OneSystem {
            fn entity(&self, kind: &str, name: &str) -> Option<Entity> {
                (kind == "system" && name == "payment-gateway").then(|| Entity {
                    title: "Payment Gateway".to_owned(),
                    description: Some("Processes payments".to_owned()),
                    url_path: None,
                })
            }
        }

        // The model is reachable, and `resolve_includes` would expand this
        // include if handed it — `bundle_source` passes `None` on purpose.
        let mut with_model = Vec::new();
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let expanded = resolve_includes(source, &[], Some(&OneSystem), &mut with_model);
        assert!(
            expanded.contains("Payment Gateway"),
            "fixture is inert: {expanded}"
        );

        let mut warnings = Vec::new();
        assert_eq!(bundle_source("plantuml", source, &[], &mut warnings), None);
        assert!(
            warnings.is_empty(),
            "meta includes must not warn: {warnings:?}"
        );
    }

    #[test]
    fn bundle_source_reports_an_unresolvable_include() {
        let mut warnings = Vec::new();
        let result = bundle_source(
            "plantuml",
            "@startuml\n!include nonexistent.iuml\n@enduml",
            &[],
            &mut warnings,
        );

        assert_eq!(result, None, "an unchanged source bundles to None");
        assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
        assert!(
            warnings[0].contains("Include file not found")
                && warnings[0].contains("nonexistent.iuml"),
            "unexpected warning: {}",
            warnings[0],
        );
    }
}
