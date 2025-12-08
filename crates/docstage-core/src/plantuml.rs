//! `PlantUML` diagram processing utilities.
//!
//! This module handles `PlantUML` source preprocessing before rendering via Kroki:
//! - Resolves `!include` directives by searching include directories
//! - Prepends DPI configuration for high-resolution output
//! - Loads optional `PlantUML` config files (e.g., `config.iuml`)

use std::path::PathBuf;

use regex::Regex;
use std::sync::LazyLock;

static INCLUDE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^!include\s+(.+)$").unwrap());

/// Default DPI for `PlantUML` diagram rendering (192 = 2x for retina displays).
pub const DEFAULT_DPI: u32 = 192;

/// Result of preparing diagram source with potential warnings.
#[derive(Clone, Debug)]
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
        warnings.push("Include depth exceeded maximum of 10".to_string());
        return source.to_string();
    }

    let mut result = source.to_string();

    for caps in INCLUDE_PATTERN.captures_iter(source) {
        let include_path = caps.get(1).unwrap().as_str().trim();
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
                result = result.replace(full_match, &resolved_content);
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
                    "Include file not found: '{}' (no include directories configured)",
                    include_path
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
/// Resolves includes and injects DPI setting and optional config content
/// after the `@startuml` directive.
///
/// # Arguments
/// * `source` - Raw `PlantUML` diagram source
/// * `include_dirs` - Directories to search for `!include` files
/// * `config_content` - Optional config file content to inject
/// * `dpi` - DPI setting for rendering (default: 192 for retina)
///
/// # Returns
/// [`PrepareResult`] containing the prepared source and any warnings.
#[must_use]
pub fn prepare_diagram_source(
    source: &str,
    include_dirs: &[PathBuf],
    config_content: Option<&str>,
    dpi: u32,
) -> PrepareResult {
    let mut warnings = Vec::new();
    let resolved = resolve_includes(source, include_dirs, 0, &mut warnings);

    // Inject DPI and config after @startuml directive
    let mut config_block = format!("skinparam dpi {dpi}\n");
    if let Some(config) = config_content {
        config_block.push_str(config);
        config_block.push('\n');
    }

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

/// Load config file content from include directories.
///
/// Returns the content and optionally warns if not found.
#[must_use]
pub fn load_config_file(include_dirs: &[PathBuf], config_file: &str) -> Option<String> {
    include_dirs.iter().find_map(|dir| {
        let path = dir.join(config_file);
        std::fs::read_to_string(&path).ok()
    })
}

/// Load config file content, returning warnings if not found.
#[must_use]
pub fn load_config_file_with_warning(
    include_dirs: &[PathBuf],
    config_file: &str,
) -> (Option<String>, Vec<String>) {
    for dir in include_dirs {
        let path = dir.join(config_file);
        if let Ok(content) = std::fs::read_to_string(&path) {
            return (Some(content), Vec::new());
        }
    }

    let searched_paths: Vec<_> = include_dirs
        .iter()
        .map(|d| d.join(config_file).display().to_string())
        .collect();

    let warning = if searched_paths.is_empty() {
        format!(
            "Config file not found: '{}' (no include directories configured)",
            config_file
        )
    } else {
        format!(
            "Config file not found: '{}' (searched: {})",
            config_file,
            searched_paths.join(", ")
        )
    };

    (None, vec![warning])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_diagram_source() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None, DEFAULT_DPI);

        // DPI should be injected after @startuml
        assert_eq!(
            result.source,
            "@startuml\nskinparam dpi 192\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_prepare_diagram_source_with_config() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let config = "skinparam backgroundColor white";
        let result = prepare_diagram_source(source, &[], Some(config), DEFAULT_DPI);

        // Config should be injected after @startuml and dpi
        assert_eq!(
            result.source,
            "@startuml\nskinparam dpi 192\nskinparam backgroundColor white\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_prepare_diagram_source_custom_dpi() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None, 300);

        assert_eq!(
            result.source,
            "@startuml\nskinparam dpi 300\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_prepare_diagram_source_preserves_content_before_startuml() {
        let source = "' comment\n@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None, DEFAULT_DPI);

        // Content before @startuml should be preserved
        assert_eq!(
            result.source,
            "' comment\n@startuml\nskinparam dpi 192\nAlice -> Bob\n@enduml"
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_unresolved_include_generates_warning() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None, DEFAULT_DPI);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("not found"));
    }

    #[test]
    fn test_unresolved_include_with_dirs_shows_searched_paths() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let include_dirs = vec![PathBuf::from("/tmp/includes")];
        let result = prepare_diagram_source(source, &include_dirs, None, DEFAULT_DPI);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("/tmp/includes"));
    }

    #[test]
    fn test_stdlib_include_no_warning() {
        let source = "@startuml\n!include <tupadr3/common>\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None, DEFAULT_DPI);

        // Stdlib includes should not generate warnings
        assert!(result.warnings.is_empty());
        // Stdlib include should be preserved as-is
        assert!(result.source.contains("!include <tupadr3/common>"));
    }
}
