//! PlantUML diagram extraction from markdown.

use regex::Regex;
use std::sync::LazyLock;

static PLANTUML_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ms)^```plantuml\s*\n(.*?)\n```").unwrap()
});

static INCLUDE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^!include\s+(.+)$").unwrap()
});

static H1_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^#\s+(.+)$").unwrap()
});

static HEADER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(#{2,6})\s+").unwrap()
});

/// Information about an extracted PlantUML diagram.
#[derive(Debug, Clone)]
pub struct DiagramInfo {
    /// Original source code from markdown
    pub source: String,
    /// Source with includes resolved and config prepended
    pub resolved_source: String,
    /// Zero-based index of this diagram
    pub index: usize,
}

/// Result of processing a document.
#[derive(Debug)]
pub struct ProcessedDocument {
    /// Markdown with diagrams replaced by placeholders
    pub markdown: String,
    /// Extracted diagrams
    pub diagrams: Vec<DiagramInfo>,
    /// Title extracted from first H1 heading
    pub title: Option<String>,
}

/// Extracts PlantUML diagrams from markdown.
pub struct PlantUmlExtractor {
    include_dirs: Vec<String>,
    config_content: Option<String>,
    dpi: u32,
}

impl PlantUmlExtractor {
    pub fn new(include_dirs: Vec<String>, config_file: Option<&str>, dpi: u32) -> Self {
        let config_content = config_file.and_then(|cf| {
            include_dirs.iter().find_map(|dir| {
                let path = std::path::Path::new(dir).join(cf);
                std::fs::read_to_string(&path).ok()
            })
        });

        Self {
            include_dirs,
            config_content,
            dpi,
        }
    }

    /// Process markdown, extracting diagrams and title.
    pub fn process(&self, markdown: &str) -> ProcessedDocument {
        let mut diagrams = Vec::new();
        let mut index = 0usize;

        // Extract PlantUML blocks and replace with placeholders
        let processed = PLANTUML_PATTERN.replace_all(markdown, |caps: &regex::Captures| {
            let source = caps.get(1).unwrap().as_str().to_string();
            let resolved = self.resolve_includes(&source, 0);

            // Prepend DPI and config
            let mut final_source = format!("skinparam dpi {}\n", self.dpi);
            if let Some(ref config) = self.config_content {
                final_source.push_str(config);
                final_source.push('\n');
            }
            final_source.push_str(&resolved);

            diagrams.push(DiagramInfo {
                source,
                resolved_source: final_source,
                index,
            });

            let placeholder = format!("{{{{DIAGRAM_{}}}}}", index);
            index += 1;
            placeholder
        });

        let mut processed = processed.into_owned();

        // Extract title from first H1
        let title = H1_PATTERN.captures(&processed).map(|caps| {
            caps.get(1).unwrap().as_str().trim().to_string()
        });

        // If we found a title, remove H1 and level up headers
        if title.is_some() {
            processed = H1_PATTERN.replace(&processed, "").trim_start().to_string();
            processed = HEADER_PATTERN
                .replace_all(&processed, |caps: &regex::Captures| {
                    let hashes = caps.get(1).unwrap().as_str();
                    format!("{} ", &hashes[1..])
                })
                .into_owned();
        }

        ProcessedDocument {
            markdown: processed,
            diagrams,
            title,
        }
    }

    fn resolve_includes(&self, source: &str, depth: usize) -> String {
        if depth > 10 {
            return source.to_string();
        }

        INCLUDE_PATTERN.replace_all(source, |caps: &regex::Captures| {
            let include_path = caps.get(1).unwrap().as_str().trim();

            // Skip stdlib includes
            if include_path.starts_with('<') && include_path.ends_with('>') {
                return caps.get(0).unwrap().as_str().to_string();
            }

            // Try to resolve from include directories
            for dir in &self.include_dirs {
                let full_path = std::path::Path::new(dir).join(include_path);
                if let Ok(content) = std::fs::read_to_string(&full_path) {
                    return self.resolve_includes(&content, depth + 1);
                }
            }

            // Keep original if not found
            caps.get(0).unwrap().as_str().to_string()
        }).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_plantuml() {
        let markdown = r#"# Title

Some text

```plantuml
@startuml
Alice -> Bob
@enduml
```

More text
"#;
        let extractor = PlantUmlExtractor::new(vec![], None, 192);
        let result = extractor.process(markdown);

        assert_eq!(result.title, Some("Title".to_string()));
        assert_eq!(result.diagrams.len(), 1);
        assert!(result.diagrams[0].resolved_source.contains("skinparam dpi 192"));
        assert!(result.markdown.contains("{{DIAGRAM_0}}"));
        assert!(!result.markdown.contains("# Title"));
    }

    #[test]
    fn test_header_level_up() {
        let markdown = "# Title\n\n## Section\n\n### Subsection";
        let extractor = PlantUmlExtractor::new(vec![], None, 192);
        let result = extractor.process(markdown);

        assert!(result.markdown.contains("# Section"));
        assert!(result.markdown.contains("## Subsection"));
    }
}
