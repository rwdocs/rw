//! Tabs post-processor for transforming `<rw-tabs>` to accessible HTML.
//!
//! Transforms the intermediate `<rw-tabs>` / `<rw-tab>` elements created by
//! [`TabsPreprocessor`](super::TabsPreprocessor) into fully accessible HTML
//! with ARIA attributes.

use std::collections::HashMap;

use crate::code_block::{CodeBlockProcessor, ProcessResult};
use crate::state::escape_html;

use super::TabsGroup;

/// Post-processor that transforms `<rw-tabs>` elements to accessible HTML.
///
/// # Output HTML Structure
///
/// ```html
/// <div class="tabs" id="tabs-0">
///   <div class="tabs-buttons" role="tablist">
///     <button role="tab" id="tab-0-0" aria-controls="panel-0-0"
///             aria-selected="true" tabindex="0">macOS</button>
///     <button role="tab" id="tab-0-1" aria-controls="panel-0-1"
///             aria-selected="false" tabindex="-1">Linux</button>
///   </div>
///   <div role="tabpanel" id="panel-0-0" aria-labelledby="tab-0-0">
///     <!-- content -->
///   </div>
///   <div role="tabpanel" id="panel-0-1" aria-labelledby="tab-0-1" hidden>
///     <!-- content -->
///   </div>
/// </div>
/// ```
pub struct TabsProcessor {
    /// Map from group ID to group metadata.
    groups: HashMap<usize, TabsGroup>,
    /// Warnings collected during processing.
    warnings: Vec<String>,
}

impl TabsProcessor {
    /// Create a new processor with the given tab groups.
    #[must_use]
    pub fn new(groups: Vec<TabsGroup>) -> Self {
        Self {
            groups: groups.into_iter().map(|g| (g.id, g)).collect(),
            warnings: Vec::new(),
        }
    }

    /// Transform a single `<rw-tabs>` block to accessible HTML.
    fn transform_tabs(&mut self, group_id: usize, inner_content: &str) -> String {
        let Some(group) = self.groups.get(&group_id) else {
            self.warnings.push(format!(
                "tabs group {group_id} not found in metadata, passing through"
            ));
            return format!(r#"<rw-tabs data-id="{group_id}">{inner_content}</rw-tabs>"#);
        };

        let mut output = String::with_capacity(inner_content.len() + 512);

        // Container div
        output.push_str(&format!(r#"<div class="tabs" id="tabs-{group_id}">"#));

        // Tab buttons
        output.push_str(r#"<div class="tabs-buttons" role="tablist">"#);
        for (idx, tab) in group.tabs.iter().enumerate() {
            let selected = idx == 0;
            let tab_id = format!("tab-{group_id}-{}", tab.id);
            let panel_id = format!("panel-{group_id}-{}", tab.id);

            output.push_str(&format!(
                r#"<button role="tab" id="{tab_id}" aria-controls="{panel_id}" aria-selected="{selected}" tabindex="{}">{}</button>"#,
                if selected { "0" } else { "-1" },
                escape_html(&tab.label)
            ));
        }
        output.push_str("</div>");

        // Tab panels - parse inner content to extract panels
        let panels = parse_tab_panels(inner_content);
        for (idx, (panel_tab_id, content)) in panels.iter().enumerate() {
            let tab = group.tabs.iter().find(|t| t.id == *panel_tab_id);
            if tab.is_none() {
                self.warnings.push(format!(
                    "tab {panel_tab_id} not found in group {group_id} metadata"
                ));
            }

            let hidden = if idx == 0 { "" } else { " hidden" };
            let tab_id = format!("tab-{group_id}-{panel_tab_id}");
            let panel_id = format!("panel-{group_id}-{panel_tab_id}");
            output.push_str(&format!(
                r#"<div role="tabpanel" id="{panel_id}" aria-labelledby="{tab_id}"{hidden}>{content}</div>"#
            ));
        }

        output.push_str("</div>");
        output
    }
}

impl CodeBlockProcessor for TabsProcessor {
    fn process(
        &mut self,
        _language: &str,
        _attrs: &HashMap<String, String>,
        _source: &str,
        _index: usize,
    ) -> ProcessResult {
        // TabsProcessor doesn't handle code blocks, only post-processing
        ProcessResult::PassThrough
    }

    fn post_process(&mut self, html: &mut String) {
        // Find and replace all <rw-tabs> elements
        let mut result = String::with_capacity(html.len());
        let mut remaining = html.as_str();

        while let Some(start) = remaining.find("<rw-tabs") {
            // Add content before the tag
            result.push_str(&remaining[..start]);

            // Parse the opening tag to get the data-id
            let tag_end = remaining[start..].find('>').map(|i| start + i + 1);
            let Some(tag_end) = tag_end else {
                // Malformed tag, pass through
                result.push_str(&remaining[start..]);
                break;
            };

            let opening_tag = &remaining[start..tag_end];
            let group_id = parse_data_id(opening_tag);

            // Find the closing tag
            let close_tag = "</rw-tabs>";
            let close_start = remaining[tag_end..].find(close_tag).map(|i| tag_end + i);
            let Some(close_start) = close_start else {
                // No closing tag, pass through
                result.push_str(&remaining[start..]);
                break;
            };

            let inner_content = &remaining[tag_end..close_start];
            let close_end = close_start + close_tag.len();

            if let Some(id) = group_id {
                let transformed = self.transform_tabs(id, inner_content);
                result.push_str(&transformed);
            } else {
                self.warnings
                    .push("rw-tabs element without data-id".to_string());
                result.push_str(&remaining[start..close_end]);
            }

            remaining = &remaining[close_end..];
        }

        // Add any remaining content
        result.push_str(remaining);

        *html = result;
    }

    fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// Parse data-id attribute from an opening tag.
fn parse_data_id(tag: &str) -> Option<usize> {
    let data_id_start = tag.find("data-id=\"")?;
    let value_start = data_id_start + 9;
    let value_end = tag[value_start..].find('"').map(|i| value_start + i)?;
    tag[value_start..value_end].parse().ok()
}

/// Parse `<rw-tab>` elements from inner content.
///
/// Returns a vector of (tab_id, content) tuples.
fn parse_tab_panels(content: &str) -> Vec<(usize, String)> {
    let mut panels = Vec::new();
    let mut remaining = content;

    while let Some(start) = remaining.find("<rw-tab") {
        // Parse opening tag
        let tag_end = remaining[start..].find('>').map(|i| start + i + 1);
        let Some(tag_end) = tag_end else {
            break;
        };

        let opening_tag = &remaining[start..tag_end];
        let tab_id = parse_data_id(opening_tag);

        // Find closing tag
        let close_tag = "</rw-tab>";
        let close_start = remaining[tag_end..].find(close_tag).map(|i| tag_end + i);
        let Some(close_start) = close_start else {
            break;
        };

        let panel_content = &remaining[tag_end..close_start];
        let close_end = close_start + close_tag.len();

        if let Some(id) = tab_id {
            panels.push((id, panel_content.to_string()));
        }

        remaining = &remaining[close_end..];
    }

    panels
}

#[cfg(test)]
mod tests {
    use super::super::TabMetadata;
    use super::*;

    fn create_test_groups() -> Vec<TabsGroup> {
        vec![TabsGroup {
            id: 0,
            tabs: vec![
                TabMetadata {
                    id: 0,
                    label: "macOS".to_string(),
                    line: 2,
                },
                TabMetadata {
                    id: 1,
                    label: "Linux".to_string(),
                    line: 5,
                },
            ],
        }]
    }

    #[test]
    fn test_parse_data_id() {
        assert_eq!(parse_data_id(r#"<rw-tabs data-id="0">"#), Some(0));
        assert_eq!(parse_data_id(r#"<rw-tabs data-id="42">"#), Some(42));
        assert_eq!(parse_data_id(r#"<rw-tabs>"#), None);
        assert_eq!(parse_data_id(r#"<rw-tabs data-id="abc">"#), None);
    }

    #[test]
    fn test_parse_tab_panels() {
        let content =
            r#"<rw-tab data-id="0">Content A</rw-tab><rw-tab data-id="1">Content B</rw-tab>"#;
        let panels = parse_tab_panels(content);

        assert_eq!(panels.len(), 2);
        assert_eq!(panels[0], (0, "Content A".to_string()));
        assert_eq!(panels[1], (1, "Content B".to_string()));
    }

    #[test]
    fn test_transform_tabs() {
        let mut processor = TabsProcessor::new(create_test_groups());

        let input = r#"<rw-tab data-id="0"><p>Install with Homebrew.</p></rw-tab><rw-tab data-id="1"><p>Install with apt.</p></rw-tab>"#;
        let output = processor.transform_tabs(0, input);

        // Check container
        assert!(output.contains(r#"<div class="tabs" id="tabs-0">"#));
        assert!(output.ends_with("</div>"));

        // Check tab buttons
        assert!(output.contains(r#"<div class="tabs-buttons" role="tablist">"#));
        assert!(output.contains(r#"<button role="tab" id="tab-0-0" aria-controls="panel-0-0" aria-selected="true" tabindex="0">macOS</button>"#));
        assert!(output.contains(r#"<button role="tab" id="tab-0-1" aria-controls="panel-0-1" aria-selected="false" tabindex="-1">Linux</button>"#));

        // Check panels
        assert!(output.contains(
            r#"<div role="tabpanel" id="panel-0-0" aria-labelledby="tab-0-0"><p>Install with Homebrew.</p></div>"#
        ));
        assert!(output.contains(
            r#"<div role="tabpanel" id="panel-0-1" aria-labelledby="tab-0-1" hidden><p>Install with apt.</p></div>"#
        ));
    }

    #[test]
    fn test_post_process() {
        let groups = create_test_groups();
        let mut processor = TabsProcessor::new(groups);

        let mut html = r#"<p>Before</p><rw-tabs data-id="0"><rw-tab data-id="0">A</rw-tab><rw-tab data-id="1">B</rw-tab></rw-tabs><p>After</p>"#.to_string();
        processor.post_process(&mut html);

        assert!(html.contains("<p>Before</p>"));
        assert!(html.contains("<p>After</p>"));
        assert!(html.contains(r#"<div class="tabs" id="tabs-0">"#));
        assert!(!html.contains("<rw-tabs"));
        assert!(!html.contains("<rw-tab"));
    }

    #[test]
    fn test_multiple_tab_groups() {
        let groups = vec![
            TabsGroup {
                id: 0,
                tabs: vec![TabMetadata {
                    id: 0,
                    label: "A".to_string(),
                    line: 1,
                }],
            },
            TabsGroup {
                id: 1,
                tabs: vec![TabMetadata {
                    id: 1,
                    label: "B".to_string(),
                    line: 5,
                }],
            },
        ];
        let mut processor = TabsProcessor::new(groups);

        let mut html = r#"<rw-tabs data-id="0"><rw-tab data-id="0">Content A</rw-tab></rw-tabs><rw-tabs data-id="1"><rw-tab data-id="1">Content B</rw-tab></rw-tabs>"#.to_string();
        processor.post_process(&mut html);

        assert!(html.contains(r#"id="tabs-0""#));
        assert!(html.contains(r#"id="tabs-1""#));
    }

    #[test]
    fn test_missing_group_warning() {
        let mut processor = TabsProcessor::new(vec![]);

        let mut html =
            r#"<rw-tabs data-id="99"><rw-tab data-id="0">Content</rw-tab></rw-tabs>"#.to_string();
        processor.post_process(&mut html);

        assert!(processor.warnings().iter().any(|w| w.contains("99")));
    }

    #[test]
    fn test_html_escaping_in_labels() {
        let groups = vec![TabsGroup {
            id: 0,
            tabs: vec![TabMetadata {
                id: 0,
                label: "<script>".to_string(),
                line: 1,
            }],
        }];
        let mut processor = TabsProcessor::new(groups);

        let input = r#"<rw-tab data-id="0">Content</rw-tab>"#;
        let output = processor.transform_tabs(0, input);

        assert!(output.contains("&lt;script&gt;"));
        assert!(!output.contains("<script>"));
    }

    #[test]
    fn test_process_returns_passthrough() {
        let mut processor = TabsProcessor::new(vec![]);
        let result = processor.process("rust", &HashMap::new(), "fn main() {}", 0);
        assert_eq!(result, ProcessResult::PassThrough);
    }

    #[test]
    fn test_content_with_html_inside_tabs() {
        let groups = vec![TabsGroup {
            id: 0,
            tabs: vec![TabMetadata {
                id: 0,
                label: "Code".to_string(),
                line: 1,
            }],
        }];
        let mut processor = TabsProcessor::new(groups);

        let mut html = r#"<rw-tabs data-id="0"><rw-tab data-id="0"><pre><code class="language-rust">fn main() {}</code></pre></rw-tab></rw-tabs>"#.to_string();
        processor.post_process(&mut html);

        assert!(html.contains(r#"<pre><code class="language-rust">fn main() {}</code></pre>"#));
    }

    #[test]
    fn test_no_warnings_for_valid_input() {
        let groups = create_test_groups();
        let mut processor = TabsProcessor::new(groups);

        let mut html = r#"<rw-tabs data-id="0"><rw-tab data-id="0">A</rw-tab><rw-tab data-id="1">B</rw-tab></rw-tabs>"#.to_string();
        processor.post_process(&mut html);

        assert!(processor.warnings().is_empty());
    }
}
