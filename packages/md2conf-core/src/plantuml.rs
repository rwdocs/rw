//! PlantUML diagram processing utilities.

use regex::Regex;
use std::sync::LazyLock;

static INCLUDE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^!include\s+(.+)$").unwrap());

const DEFAULT_DPI: u32 = 192;

/// Resolve PlantUML !include directives in diagram source.
pub fn resolve_includes(source: &str, include_dirs: &[String], depth: usize) -> String {
    if depth > 10 {
        return source.to_string();
    }

    INCLUDE_PATTERN
        .replace_all(source, |caps: &regex::Captures| {
            let include_path = caps.get(1).unwrap().as_str().trim();

            // Skip stdlib includes
            if include_path.starts_with('<') && include_path.ends_with('>') {
                return caps.get(0).unwrap().as_str().to_string();
            }

            // Try to resolve from include directories
            for dir in include_dirs {
                let full_path = std::path::Path::new(dir).join(include_path);
                if let Ok(content) = std::fs::read_to_string(&full_path) {
                    return resolve_includes(&content, include_dirs, depth + 1);
                }
            }

            // Keep original if not found
            caps.get(0).unwrap().as_str().to_string()
        })
        .into_owned()
}

/// Prepare PlantUML source for rendering.
///
/// Resolves includes, prepends DPI setting and optional config content.
pub fn prepare_diagram_source(
    source: &str,
    include_dirs: &[String],
    config_content: Option<&str>,
) -> String {
    let resolved = resolve_includes(source, include_dirs, 0);

    // Prepend DPI and config
    let mut final_source = format!("skinparam dpi {}\n", DEFAULT_DPI);
    if let Some(config) = config_content {
        final_source.push_str(config);
        final_source.push('\n');
    }
    final_source.push_str(&resolved);
    final_source
}

/// Load config file content from include directories.
pub fn load_config_file(include_dirs: &[String], config_file: &str) -> Option<String> {
    include_dirs.iter().find_map(|dir| {
        let path = std::path::Path::new(dir).join(config_file);
        std::fs::read_to_string(&path).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_diagram_source() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        assert!(result.contains("skinparam dpi 192"));
        assert!(result.contains("Alice -> Bob"));
    }

    #[test]
    fn test_prepare_diagram_source_with_config() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let config = "skinparam backgroundColor white";
        let result = prepare_diagram_source(source, &[], Some(config));

        assert!(result.contains("skinparam dpi 192"));
        assert!(result.contains("skinparam backgroundColor white"));
        assert!(result.contains("Alice -> Bob"));
    }
}
