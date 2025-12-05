//! PlantUML diagram extraction as an iterator adapter over pulldown-cmark events.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};

/// Information about an extracted PlantUML diagram.
#[derive(Debug, Clone)]
pub struct DiagramInfo {
    /// Original source code from markdown
    pub source: String,
    /// Zero-based index of this diagram
    pub index: usize,
}

/// Iterator adapter that extracts PlantUML diagrams from a pulldown-cmark event stream.
///
/// This filter:
/// - Intercepts code blocks with `plantuml` language
/// - Collects their source code into `DiagramInfo` structs
/// - Emits `{{DIAGRAM_N}}` placeholder as `Event::Html`
/// - Passes through all other events unchanged
///
/// # Example
///
/// ```ignore
/// use pulldown_cmark::Parser;
/// use docstage_core::PlantUmlFilter;
///
/// let markdown = "# Title\n\n```plantuml\n@startuml\nA -> B\n@enduml\n```";
/// let parser = Parser::new(markdown);
/// let filter = PlantUmlFilter::new(parser);
///
/// // Consume events (e.g., pass to a renderer)
/// let events: Vec<_> = filter.collect();
///
/// // After iteration, get the extracted diagrams
/// // Note: need to use filter before collecting to get diagrams
/// ```
pub struct PlantUmlFilter<'a, I: Iterator<Item = Event<'a>>> {
    iter: I,
    diagrams: Vec<DiagramInfo>,
    state: FilterState,
}

#[derive(Debug, Default)]
enum FilterState {
    #[default]
    Normal,
    InPlantUml {
        source: String,
    },
}

impl<'a, I: Iterator<Item = Event<'a>>> PlantUmlFilter<'a, I> {
    /// Create a new PlantUML filter wrapping the given event iterator.
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            diagrams: Vec::new(),
            state: FilterState::Normal,
        }
    }

    /// Get a reference to the diagrams extracted so far.
    pub fn diagrams(&self) -> &[DiagramInfo] {
        &self.diagrams
    }

    /// Consume the filter and return the collected diagrams.
    pub fn into_diagrams(self) -> Vec<DiagramInfo> {
        self.diagrams
    }
}

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for PlantUmlFilter<'a, I> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let event = self.iter.next()?;

            match (&mut self.state, event) {
                // Start of a plantuml code block
                (
                    FilterState::Normal,
                    Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))),
                ) if is_plantuml(&lang) => {
                    self.state = FilterState::InPlantUml {
                        source: String::new(),
                    };
                    // Don't emit the Start event, continue to collect content
                    continue;
                }

                // Text inside plantuml block - collect it
                (FilterState::InPlantUml { source }, Event::Text(text)) => {
                    source.push_str(&text);
                    continue;
                }

                // End of plantuml block - emit placeholder
                (FilterState::InPlantUml { .. }, Event::End(TagEnd::CodeBlock)) => {
                    // Take the state and reset to Normal
                    let old_state = std::mem::take(&mut self.state);
                    if let FilterState::InPlantUml { source } = old_state {
                        let index = self.diagrams.len();
                        self.diagrams.push(DiagramInfo { source, index });

                        // Emit placeholder as Html event (passes through unchanged)
                        let placeholder = format!("{{{{DIAGRAM_{}}}}}", index);
                        return Some(Event::Html(CowStr::Boxed(placeholder.into_boxed_str())));
                    }
                    unreachable!()
                }

                // Any other event while in plantuml block (shouldn't happen normally)
                (FilterState::InPlantUml { source }, other) => {
                    // Handle unexpected events - just collect text representation
                    if let Event::SoftBreak | Event::HardBreak = other {
                        source.push('\n');
                    }
                    continue;
                }

                // Normal event - pass through
                (FilterState::Normal, event) => {
                    return Some(event);
                }
            }
        }
    }
}

fn is_plantuml(lang: &str) -> bool {
    lang.split_whitespace()
        .next()
        .map(|l| l == "plantuml")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Parser;

    #[test]
    fn test_extracts_single_diagram() {
        let markdown = "# Title\n\n```plantuml\n@startuml\nAlice -> Bob\n@enduml\n```\n\nText";
        let parser = Parser::new(markdown);
        let mut filter = PlantUmlFilter::new(parser);

        let events: Vec<_> = filter.by_ref().collect();

        // Check diagrams
        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].index, 0);
        assert!(diagrams[0].source.contains("Alice -> Bob"));

        // Check placeholder is in events
        let has_placeholder = events
            .iter()
            .any(|e| matches!(e, Event::Html(s) if s.contains("{{DIAGRAM_0}}")));
        assert!(has_placeholder);
    }

    #[test]
    fn test_extracts_multiple_diagrams() {
        let markdown = r#"
```plantuml
@startuml
A -> B
@enduml
```

Some text

```plantuml
@startuml
C -> D
@enduml
```
"#;
        let parser = Parser::new(markdown);
        let mut filter = PlantUmlFilter::new(parser);

        let events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert_eq!(diagrams.len(), 2);
        assert!(diagrams[0].source.contains("A -> B"));
        assert!(diagrams[1].source.contains("C -> D"));

        // Check both placeholders
        let placeholder_count = events
            .iter()
            .filter(|e| matches!(e, Event::Html(s) if s.contains("DIAGRAM_")))
            .count();
        assert_eq!(placeholder_count, 2);
    }

    #[test]
    fn test_passes_through_other_code_blocks() {
        let markdown = "```rust\nfn main() {}\n```";
        let parser = Parser::new(markdown);
        let mut filter = PlantUmlFilter::new(parser);

        let events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert!(diagrams.is_empty());

        // Should have Start(CodeBlock), Text, End(CodeBlock)
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Start(Tag::CodeBlock(_))))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Text(s) if s.contains("fn main")))
        );
    }

    #[test]
    fn test_no_diagrams() {
        let markdown = "# Just text\n\nNo diagrams here.";
        let parser = Parser::new(markdown);
        let mut filter = PlantUmlFilter::new(parser);

        let _events: Vec<_> = filter.by_ref().collect();

        let diagrams = filter.into_diagrams();
        assert!(diagrams.is_empty());
    }
}
