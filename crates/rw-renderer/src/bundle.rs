//! Markdown bundling for resolving external references.
//!
//! This module provides [`bundle_markdown`], which parses markdown using
//! pulldown-cmark to find code blocks and dispatches to registered
//! [`CodeBlockProcessor`]s for bundling (e.g., resolving `!include` directives).
//!
//! Unlike rendering, bundling returns modified markdown (not HTML),
//! preserving all formatting outside of processed code blocks.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::code_block::{CodeBlockProcessor, parse_fence_info};

/// Bundle markdown by resolving external references in code blocks.
///
/// Parses markdown with pulldown-cmark, finds fenced code blocks, and calls
/// [`CodeBlockProcessor::bundle`] on each processor. The first processor
/// returning `Some(resolved_source)` wins.
///
/// Returns modified markdown with code block contents replaced. Everything
/// outside code blocks (fences, language tags, surrounding text) is preserved
/// exactly.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use rw_renderer::{CodeBlockProcessor, ProcessResult, bundle_markdown};
///
/// struct UpperCaseProcessor;
///
/// impl CodeBlockProcessor for UpperCaseProcessor {
///     fn process(&mut self, _: &str, _: &HashMap<String, String>,
///                _: &str, _: usize) -> ProcessResult {
///         ProcessResult::PassThrough
///     }
///     fn bundle(&mut self, language: &str, source: &str) -> Option<String> {
///         if language == "upper" {
///             Some(source.to_uppercase())
///         } else {
///             None
///         }
///     }
/// }
///
/// let md = "```upper\nhello world\n```";
/// let mut proc = UpperCaseProcessor;
/// let result = bundle_markdown(md, &mut [&mut proc]);
/// assert!(result.contains("HELLO WORLD"));
/// ```
pub fn bundle_markdown(markdown: &str, processors: &mut [&mut dyn CodeBlockProcessor]) -> String {
    if processors.is_empty() {
        return markdown.to_owned();
    }

    let parser = Parser::new_ext(markdown, Options::empty());
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();

    let mut current_language: Option<String> = None;
    let mut content_range: Option<Range<usize>> = None;
    let mut content = String::new();

    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let (lang, _attrs) = parse_fence_info(&info);
                current_language = if lang.is_empty() { None } else { Some(lang) };
                content.clear();
                content_range = None;
            }
            Event::Text(text) if current_language.is_some() => {
                if content_range.is_none() {
                    content_range = Some(range.clone());
                } else if let Some(ref mut cr) = content_range {
                    cr.end = range.end;
                }
                content.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let (Some(lang), Some(cr)) = (&current_language, &content_range) {
                    // Strip trailing newline — pulldown-cmark includes the newline
                    // before the closing fence as part of the content text.
                    let (source, replace_range) = if let Some(stripped) = content.strip_suffix('\n')
                    {
                        (stripped, cr.start..cr.end - 1)
                    } else {
                        (content.as_str(), cr.clone())
                    };
                    for processor in processors.iter_mut() {
                        if let Some(resolved) = processor.bundle(lang, source) {
                            replacements.push((replace_range, resolved));
                            break;
                        }
                    }
                }
                current_language = None;
                content_range = None;
                content.clear();
            }
            _ => {}
        }
    }

    if replacements.is_empty() {
        return markdown.to_owned();
    }

    // Single-pass forward copy: splice in replacements while copying unchanged segments
    let mut result = String::with_capacity(markdown.len());
    let mut pos = 0;
    for (range, new_content) in &replacements {
        result.push_str(&markdown[pos..range.start]);
        result.push_str(new_content);
        pos = range.end;
    }
    result.push_str(&markdown[pos..]);
    result
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::code_block::ProcessResult;

    /// Test processor that uppercases content for "upper" language.
    struct UpperCaseProcessor;

    impl CodeBlockProcessor for UpperCaseProcessor {
        fn process(
            &mut self,
            _language: &str,
            _attrs: &HashMap<String, String>,
            _source: &str,
            _index: usize,
        ) -> ProcessResult {
            ProcessResult::PassThrough
        }

        fn bundle(&mut self, language: &str, source: &str) -> Option<String> {
            if language == "upper" {
                Some(source.to_uppercase())
            } else {
                None
            }
        }
    }

    /// Test processor that expands content for "expand" language.
    /// Each char becomes "char=X " to produce longer output.
    struct ExpandProcessor;

    impl CodeBlockProcessor for ExpandProcessor {
        fn process(
            &mut self,
            _language: &str,
            _attrs: &HashMap<String, String>,
            _source: &str,
            _index: usize,
        ) -> ProcessResult {
            ProcessResult::PassThrough
        }

        fn bundle(&mut self, language: &str, source: &str) -> Option<String> {
            if language == "expand" {
                Some(source.chars().map(|c| format!("[{c}]")).collect())
            } else {
                None
            }
        }
    }

    /// Test processor that reverses content for "reverse" language.
    struct ReverseProcessor;

    impl CodeBlockProcessor for ReverseProcessor {
        fn process(
            &mut self,
            _language: &str,
            _attrs: &HashMap<String, String>,
            _source: &str,
            _index: usize,
        ) -> ProcessResult {
            ProcessResult::PassThrough
        }

        fn bundle(&mut self, language: &str, source: &str) -> Option<String> {
            if language == "reverse" {
                Some(source.chars().rev().collect())
            } else {
                None
            }
        }
    }

    #[test]
    fn test_no_processors_returns_unchanged() {
        let md = "# Hello\n\n```rust\nfn main() {}\n```\n";
        let result = bundle_markdown(md, &mut []);
        assert_eq!(result, md);
    }

    #[test]
    fn test_no_code_blocks_returns_unchanged() {
        let md = "# Hello\n\nSome paragraph text.\n\n- List item\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(result, md);
    }

    #[test]
    fn test_non_matching_language_unchanged() {
        let md = "```rust\nfn main() {}\n```\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(result, md);
    }

    #[test]
    fn test_matching_language_content_replaced() {
        let md = "```upper\nhello world\n```\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(result, "```upper\nHELLO WORLD\n```\n");
    }

    #[test]
    fn test_surrounding_content_preserved() {
        let md = "# Title\n\nBefore.\n\n```upper\nhello\n```\n\nAfter.\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(
            result,
            "# Title\n\nBefore.\n\n```upper\nHELLO\n```\n\nAfter.\n"
        );
    }

    #[test]
    fn test_multiple_code_blocks_only_matching_replaced() {
        let md = "```rust\nlet x = 1;\n```\n\n```upper\nhello\n```\n\n```python\nprint(1)\n```\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(
            result,
            "```rust\nlet x = 1;\n```\n\n```upper\nHELLO\n```\n\n```python\nprint(1)\n```\n"
        );
    }

    #[test]
    fn test_multiple_processors_first_match_wins() {
        // Both processors handle "upper" — first one wins
        let md = "```upper\nhello\n```\n";
        let mut proc1 = UpperCaseProcessor;
        let mut proc2 = ReverseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc1, &mut proc2]);
        assert_eq!(result, "```upper\nHELLO\n```\n");
    }

    #[test]
    fn test_different_processors_different_blocks() {
        let md = "```upper\nhello\n```\n\n```reverse\nabc\n```\n";
        let mut proc1 = UpperCaseProcessor;
        let mut proc2 = ReverseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc1, &mut proc2]);
        assert_eq!(result, "```upper\nHELLO\n```\n\n```reverse\ncba\n```\n");
    }

    #[test]
    fn test_code_block_with_attributes_preserved() {
        let md = "```upper format=png\nhello\n```\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(result, "```upper format=png\nHELLO\n```\n");
    }

    #[test]
    fn test_four_backtick_fence() {
        let md = "````upper\nhello\n````\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(result, "````upper\nHELLO\n````\n");
    }

    #[test]
    fn test_indented_code_block_not_processed() {
        // Indented code blocks (4 spaces) are not fenced, should be skipped
        let md = "    indented code\n    more code\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(result, md);
    }

    #[test]
    fn test_empty_code_block() {
        let md = "```upper\n```\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        // Empty code block — processor gets empty string
        assert_eq!(result, "```upper\n```\n");
    }

    #[test]
    fn test_code_block_no_language() {
        let md = "```\nsome content\n```\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(result, md);
    }

    #[test]
    fn test_multiline_content() {
        let md = "```upper\nline one\nline two\nline three\n```\n";
        let mut proc = UpperCaseProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(result, "```upper\nLINE ONE\nLINE TWO\nLINE THREE\n```\n");
    }

    #[test]
    fn test_replacement_changes_content_length() {
        // "ab" (2 bytes) expands to "[a][b]" (6 bytes), "xy" to "[x][y]"
        let md = "```expand\nab\n```\n\nMiddle text.\n\n```expand\nxy\n```\n";
        let mut proc = ExpandProcessor;
        let result = bundle_markdown(md, &mut [&mut proc]);
        assert_eq!(
            result,
            "```expand\n[a][b]\n```\n\nMiddle text.\n\n```expand\n[x][y]\n```\n"
        );
    }
}
