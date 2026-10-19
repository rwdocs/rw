//! `PlantUML` `!include` resolution and render-config injection.
//!
//! Resolves `!include` directives against the configured include directories
//! and the site model. On top of that, [`prepare_diagram_source`] injects the
//! DPI and font settings a render needs, while [`bundle_source`]
//! only resolves. Stdlib includes (`!include <tupadr3/common>`) are left alone.

use std::path::PathBuf;
use std::sync::LazyLock;

use regex::{Captures, Regex};
use rw_diagrams::{FileStamp, SiteModel};

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
    /// Every include candidate probed on disk, including misses, so a caller
    /// can tell when an edit or a new file would change [`source`](Self::source).
    pub reads: Vec<FileStamp>,
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
///
/// Each include-directory candidate probed is pushed onto `reads`.
#[must_use]
pub(crate) fn resolve_includes(
    source: &str,
    include_dirs: &[PathBuf],
    meta_source: Option<&dyn SiteModel>,
    warnings: &mut Vec<String>,
    reads: &mut Vec<FileStamp>,
) -> String {
    resolve_includes_at(source, include_dirs, meta_source, 0, warnings, reads)
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
    reads: &mut Vec<FileStamp>,
) -> String {
    if depth > 10 {
        warnings.push("Include depth exceeded maximum of 10".to_owned());
        return source.to_owned();
    }

    INCLUDE_PATTERN
        .replace_all(source, |caps: &Captures| {
            let leading_whitespace = &caps[1];
            let include_path = caps[2].trim();

            if include_path.starts_with('<') && include_path.ends_with('>') {
                return caps[0].to_owned();
            }

            if let Some(meta) = meta_source
                && let Some(content) = resolve_meta_include(include_path, meta)
            {
                return indent_content(&content, leading_whitespace);
            }

            for dir in include_dirs {
                let path = dir.join(include_path);
                // Stamped before the read, so an edit racing it leaves a stale
                // stamp (a spurious re-render) rather than stale content.
                reads.push(FileStamp::of(path.clone()));
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let resolved_content = resolve_includes_at(
                        &content,
                        include_dirs,
                        meta_source,
                        depth + 1,
                        warnings,
                        reads,
                    );
                    return indent_content(&resolved_content, leading_whitespace);
                }
            }

            // Without a model, meta-pattern includes (systems/sys_*.iuml etc.)
            // are left for request time, when one is available.
            if meta_source.is_some() || !is_meta_include_pattern(include_path) {
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
            caps[0].to_owned()
        })
        .into_owned()
}

/// The `skinparam dpi` that [`prepare_diagram_source`] injects: twice the
/// 96-DPI CSS pixel, so diagrams stay sharp on retina displays once scaled
/// back down for display.
pub const PLANTUML_DPI: u32 = 192;

/// Prepare `PlantUML` source for rendering.
///
/// Resolves includes and injects [`PLANTUML_DPI`] and font settings
/// after the `@startuml` directive.
///
/// # Arguments
/// * `source` - Raw `PlantUML` diagram source
/// * `include_dirs` - Directories to search for `!include` files
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
    meta_source: Option<&dyn SiteModel>,
) -> PrepareResult {
    let mut warnings = Vec::new();
    let mut reads = Vec::new();
    let resolved = resolve_includes(source, include_dirs, meta_source, &mut warnings, &mut reads);

    // Inject DPI and font config after @startuml directive
    let config_block = format!(
        "skinparam dpi {PLANTUML_DPI}\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\n"
    );

    // Without a complete `@startuml` line the config is prepended, which not
    // every diagram type accepts.
    let insert_pos = resolved
        .find("@startuml")
        .and_then(|pos| Some(pos + resolved[pos..].find('\n')? + 1))
        .unwrap_or(0);
    let (head, tail) = resolved.split_at(insert_pos);

    PrepareResult {
        source: format!("{head}{config_block}{tail}"),
        warnings,
        reads,
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
    let resolved = resolve_includes(source, include_dirs, None, warnings, &mut Vec::new());
    (resolved != source).then_some(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_diagram_source() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        // DPI and font should be injected after @startuml
        assert_eq!(
            result.source,
            "@startuml\nskinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\nAlice -> Bob\n@enduml"
        );
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    #[test]
    fn test_prepare_diagram_source_preserves_content_before_startuml() {
        let source = "' comment\n@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        // Content before @startuml should be preserved
        assert_eq!(
            result.source,
            "' comment\n@startuml\nskinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\nAlice -> Bob\n@enduml"
        );
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    #[test]
    fn test_unresolved_include_generates_warning() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("not found"));
    }

    #[test]
    fn test_unresolved_include_with_dirs_shows_searched_paths() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let include_dirs = vec![PathBuf::from("/tmp/includes")];
        let result = prepare_diagram_source(source, &include_dirs, None);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("/tmp/includes"));
    }

    #[test]
    fn test_stdlib_include_no_warning() {
        let source = "@startuml\n!include <tupadr3/common>\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        // Stdlib includes should not generate warnings
        assert_eq!(result.warnings, [] as [String; 0]);
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
        let result = prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], None);

        assert_eq!(result.warnings, [] as [String; 0]);
        // Indented include should be resolved and content should be indented
        assert!(result.source.contains("  Component(comp, \"Component\")"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_indented_include_warning() {
        let source = "@startuml\nSystem_Boundary(sys, \"System\")\n  !include missing.iuml\nBoundary_End()\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        // Should generate warning for indented include too
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
    }

    #[test]
    fn test_prepare_diagram_source_no_startuml() {
        let source = "Alice -> Bob";
        let result = prepare_diagram_source(source, &[], None);

        assert_eq!(
            result.source,
            "skinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\nAlice -> Bob"
        );
    }

    #[test]
    fn test_prepare_diagram_source_startuml_no_newline() {
        let source = "@startuml";
        let result = prepare_diagram_source(source, &[], None);

        assert_eq!(
            result.source,
            "skinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\n@startuml"
        );
    }

    #[test]
    fn test_include_depth_exceeded() {
        // Create a self-referencing include to trigger depth limit
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("recursive.iuml");
        // File includes itself
        std::fs::write(&include_path, "!include recursive.iuml\nContent").unwrap();

        let source = "@startuml\n!include recursive.iuml\n@enduml";
        let result = prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], None);

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
        let result = prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], None);

        assert_eq!(result.warnings, [] as [String; 0]);
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
        let result = prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], None);

        assert_eq!(result.warnings, [] as [String; 0]);
        assert!(result.source.contains("OuterBefore"));
        assert!(result.source.contains("InnerContent"));
        assert!(result.source.contains("OuterAfter"));
    }

    /// Misses count: creating `shadow.iuml` in the first directory, or the
    /// missing `absent.iuml` anywhere, would change the prepared source.
    #[test]
    fn every_probed_candidate_is_reported_with_its_stamp() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        std::fs::write(second.path().join("outer.iuml"), "!include inner.iuml").unwrap();
        std::fs::write(first.path().join("inner.iuml"), "Inner").unwrap();
        let dirs = [first.path().to_path_buf(), second.path().to_path_buf()];

        let source = "@startuml\n!include outer.iuml\n!include absent.iuml\n@enduml";
        let result = prepare_diagram_source(source, &dirs, None);

        let stamp = |dir: &PathBuf, name: &str| FileStamp::of(dir.join(name));
        let [first, second] = &dirs;
        assert_eq!(
            result.reads,
            [
                stamp(first, "outer.iuml"),
                stamp(second, "outer.iuml"),
                stamp(first, "inner.iuml"),
                stamp(first, "absent.iuml"),
                stamp(second, "absent.iuml"),
            ],
        );
        assert_eq!(result.reads[0].modified, None);
        assert!(result.reads[1].modified.is_some());
    }

    #[test]
    fn test_indented_content_empty_lines_preserved() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("with_empty.iuml");
        std::fs::write(&include_path, "Line1\n\nLine3").unwrap();

        let source = "@startuml\n  !include with_empty.iuml\n@enduml";
        let result = prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], None);

        assert_eq!(result.warnings, [] as [String; 0]);
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
            &mut Vec::new(),
        );

        assert_eq!(warnings, [] as [String; 0]);
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
            &mut Vec::new(),
        );

        assert_eq!(warnings, [] as [String; 0]);
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
            &mut Vec::new(),
        );

        assert_eq!(warnings, [] as [String; 0]);
        // Characterizes CRLF handling; it cannot fail independently of the LF
        // cases above, since both corruptions came from `\n` being in the
        // capture's character class. Kept because this is the crate's only
        // CRLF coverage, and because it pins a wart: `(.+)$` swallows the
        // include line's `\r` into the match and the expansion never restores
        // it, so `@enduml` below arrives with a bare `\n`.
        assert_eq!(result, "@startuml\r\n\r\nline1\nline2\n@enduml");
    }

    #[test]
    fn include_does_not_expand_inside_a_longer_path() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("a.iuml"), "Alice -> Bob").unwrap();

        let source = "!include a.iuml\n!include a.iuml.bak";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
            &mut Vec::new(),
        );

        assert_eq!(result, "Alice -> Bob\n!include a.iuml.bak");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("a.iuml.bak"), "{warnings:?}");
    }

    #[test]
    fn commented_out_include_is_left_alone() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("a.iuml"), "Alice -> Bob").unwrap();

        let source = "' !include a.iuml\n!include a.iuml";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
            &mut Vec::new(),
        );

        assert_eq!(result, "' !include a.iuml\nAlice -> Bob");
        assert_eq!(warnings, [] as [String; 0]);
    }

    #[test]
    fn each_include_keeps_its_own_indentation() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("a.iuml"), "line1\nline2").unwrap();

        let source = "!include a.iuml\n  !include a.iuml";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
            &mut Vec::new(),
        );

        assert_eq!(result, "line1\nline2\n  line1\n  line2");
        assert_eq!(warnings, [] as [String; 0]);
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
        let result = prepare_diagram_source(source, &[], Some(&meta_source));
        assert_eq!(result.warnings, [] as [String; 0]);
        assert!(result.source.contains("System(sys_payment_gateway,"));
        assert!(result.source.contains("Payment Gateway"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_meta_include_falls_back_to_filesystem() {
        let source = "@startuml\n!include systems/sys_unknown.iuml\n@enduml";
        let meta_source = TestMetaSource;
        let result = prepare_diagram_source(source, &[], Some(&meta_source));
        assert_ne!(result.warnings, [] as [String; 0]);
        assert!(result.warnings[0].contains("sys_unknown.iuml"));
    }

    #[test]
    fn test_meta_pattern_include_no_warning_without_meta_source() {
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let result = prepare_diagram_source(source, &[], None);
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
        let expanded = resolve_includes(
            source,
            &[],
            Some(&OneSystem),
            &mut with_model,
            &mut Vec::new(),
        );
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
