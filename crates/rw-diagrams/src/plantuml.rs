//! `PlantUML` diagram processing utilities.
//!
//! This module handles `PlantUML` source preprocessing before rendering via Kroki:
//! - Resolves `!include` directives by searching include directories
//! - Prepends DPI and font configuration for high-resolution output

use std::path::PathBuf;

use regex::Regex;
use std::sync::LazyLock;

static INCLUDE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(\s*)!include\s+(.+)$").unwrap());

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

/// Resolve `PlantUML` !include directives in diagram source.
fn resolve_includes(
    source: &str,
    include_dirs: &[PathBuf],
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

        // Try to resolve from include directories
        let mut resolved = false;
        for dir in include_dirs {
            let full_path = dir.join(include_path);
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                let resolved_content =
                    resolve_includes(&content, include_dirs, depth + 1, warnings);
                // Indent included content to match the !include directive
                let indented_content = indent_content(&resolved_content, leading_whitespace);
                result = result.replace(full_match, &indented_content);
                resolved = true;
                break;
            }
        }

        if !resolved {
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
///
/// # Returns
/// [`PrepareResult`] containing the prepared source and any warnings.
#[must_use]
pub fn prepare_diagram_source(source: &str, include_dirs: &[PathBuf], dpi: u32) -> PrepareResult {
    let mut warnings = Vec::new();
    let resolved = resolve_includes(source, include_dirs, 0, &mut warnings);

    // Inject DPI and font config after @startuml directive
    let config_block = format!("skinparam dpi {dpi}\nskinparam defaultFontName Roboto\n");

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::DEFAULT_DPI;

    #[test]
    fn test_prepare_diagram_source() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI);

        // DPI and font should be injected after @startuml
        assert_eq!(
            result.source,
            "@startuml\nskinparam dpi 192\nskinparam defaultFontName Roboto\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_prepare_diagram_source_custom_dpi() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], 300);

        assert_eq!(
            result.source,
            "@startuml\nskinparam dpi 300\nskinparam defaultFontName Roboto\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_prepare_diagram_source_preserves_content_before_startuml() {
        let source = "' comment\n@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI);

        // Content before @startuml should be preserved
        assert_eq!(
            result.source,
            "' comment\n@startuml\nskinparam dpi 192\nskinparam defaultFontName Roboto\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_unresolved_include_generates_warning() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("not found"));
    }

    #[test]
    fn test_unresolved_include_with_dirs_shows_searched_paths() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let include_dirs = vec![PathBuf::from("/tmp/includes")];
        let result = prepare_diagram_source(source, &include_dirs, DEFAULT_DPI);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("/tmp/includes"));
    }

    #[test]
    fn test_stdlib_include_no_warning() {
        let source = "@startuml\n!include <tupadr3/common>\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI);

        // Stdlib includes should not generate warnings
        assert!(result.warnings.is_empty());
        // Stdlib include should be preserved as-is
        assert!(result.source.contains("!include <tupadr3/common>"));
    }

    #[test]
    fn test_indented_include_resolved() {
        // Create a temp file for the include
        let temp_dir = std::env::temp_dir();
        let include_path = temp_dir.join("test_component.iuml");
        std::fs::write(&include_path, "Component(comp, \"Component\")").unwrap();

        let source = "@startuml\nSystem_Boundary(sys, \"System\")\n  !include test_component.iuml\nBoundary_End()\n@enduml";
        let result = prepare_diagram_source(source, std::slice::from_ref(&temp_dir), DEFAULT_DPI);

        // Cleanup
        std::fs::remove_file(&include_path).unwrap();

        assert!(result.warnings.is_empty());
        // Indented include should be resolved and content should be indented
        assert!(result.source.contains("  Component(comp, \"Component\")"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_indented_include_warning() {
        let source = "@startuml\nSystem_Boundary(sys, \"System\")\n  !include missing.iuml\nBoundary_End()\n@enduml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI);

        // Should generate warning for indented include too
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
    }

    #[test]
    fn test_prepare_diagram_source_no_startuml() {
        // Source without @startuml - fallback to prepending config
        let source = "Alice -> Bob";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI);

        // Config should be prepended
        assert!(result.source.starts_with("skinparam dpi 192\n"));
        assert!(result.source.contains("skinparam defaultFontName Roboto"));
        assert!(result.source.contains("Alice -> Bob"));
    }

    #[test]
    fn test_prepare_diagram_source_startuml_no_newline() {
        // @startuml at end of source without newline
        let source = "@startuml";
        let result = prepare_diagram_source(source, &[], DEFAULT_DPI);

        // Should fallback to prepending
        assert!(result.source.contains("skinparam dpi 192"));
        assert!(result.source.contains("skinparam defaultFontName Roboto"));
        assert!(result.source.contains("@startuml"));
    }

    #[test]
    fn test_include_depth_exceeded() {
        // Create a self-referencing include to trigger depth limit
        let temp_dir = std::env::temp_dir();
        let include_path = temp_dir.join("recursive.iuml");
        // File includes itself
        std::fs::write(&include_path, "!include recursive.iuml\nContent").unwrap();

        let source = "@startuml\n!include recursive.iuml\n@enduml";
        let result = prepare_diagram_source(source, std::slice::from_ref(&temp_dir), DEFAULT_DPI);

        std::fs::remove_file(&include_path).unwrap();

        // Should have warning about depth exceeded
        assert!(result.warnings.iter().any(|w| w.contains("depth exceeded")));
    }

    #[test]
    fn test_multiple_includes_resolved() {
        let temp_dir = std::env::temp_dir();
        let include1 = temp_dir.join("part1.iuml");
        let include2 = temp_dir.join("part2.iuml");
        std::fs::write(&include1, "Alice -> Bob").unwrap();
        std::fs::write(&include2, "Bob -> Charlie").unwrap();

        let source = "@startuml\n!include part1.iuml\n!include part2.iuml\n@enduml";
        let result = prepare_diagram_source(source, std::slice::from_ref(&temp_dir), DEFAULT_DPI);

        std::fs::remove_file(&include1).unwrap();
        std::fs::remove_file(&include2).unwrap();

        assert!(result.warnings.is_empty());
        assert!(result.source.contains("Alice -> Bob"));
        assert!(result.source.contains("Bob -> Charlie"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_nested_includes() {
        let temp_dir = std::env::temp_dir();
        let outer = temp_dir.join("outer.iuml");
        let inner = temp_dir.join("inner.iuml");
        std::fs::write(&inner, "InnerContent").unwrap();
        std::fs::write(&outer, "OuterBefore\n!include inner.iuml\nOuterAfter").unwrap();

        let source = "@startuml\n!include outer.iuml\n@enduml";
        let result = prepare_diagram_source(source, std::slice::from_ref(&temp_dir), DEFAULT_DPI);

        std::fs::remove_file(&outer).unwrap();
        std::fs::remove_file(&inner).unwrap();

        assert!(result.warnings.is_empty());
        assert!(result.source.contains("OuterBefore"));
        assert!(result.source.contains("InnerContent"));
        assert!(result.source.contains("OuterAfter"));
    }

    #[test]
    fn test_indented_content_empty_lines_preserved() {
        let temp_dir = std::env::temp_dir();
        let include_path = temp_dir.join("with_empty.iuml");
        std::fs::write(&include_path, "Line1\n\nLine3").unwrap();

        let source = "@startuml\n  !include with_empty.iuml\n@enduml";
        let result = prepare_diagram_source(source, std::slice::from_ref(&temp_dir), DEFAULT_DPI);

        std::fs::remove_file(&include_path).unwrap();

        assert!(result.warnings.is_empty());
        // Empty lines should remain empty (not indented)
        assert!(result.source.contains("  Line1\n\n  Line3"));
    }
}
