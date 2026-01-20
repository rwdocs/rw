//! Shared state structs for markdown rendering.
//!
//! These structs track context during event processing and are shared
//! between HTML and Confluence backends.

use std::collections::HashMap;

use pulldown_cmark::Alignment;

/// State for tracking code block rendering.
#[derive(Default)]
pub struct CodeBlockState {
    /// Whether we're inside a code block.
    active: bool,
    /// Language of current code block (e.g., "rust", "python").
    language: Option<String>,
    /// Buffer for code block content.
    buffer: String,
}

impl CodeBlockState {
    /// Start a new code block with optional language.
    pub fn start(&mut self, language: Option<String>) {
        self.active = true;
        self.language = language;
        self.buffer.clear();
    }

    /// End the current code block and return (language, content).
    pub fn end(&mut self) -> (Option<String>, String) {
        self.active = false;
        (self.language.take(), std::mem::take(&mut self.buffer))
    }

    /// Check if we're inside a code block.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Append text to the code block buffer.
    pub fn push_str(&mut self, text: &str) {
        self.buffer.push_str(text);
    }

    /// Append a newline to the code block buffer.
    pub fn push_newline(&mut self) {
        self.buffer.push('\n');
    }
}

/// State for tracking table rendering.
#[derive(Default)]
pub struct TableState {
    /// Whether we're inside the table header row.
    in_head: bool,
    /// Column alignments for current table.
    alignments: Vec<Alignment>,
    /// Current column index in table row.
    cell_index: usize,
}

impl TableState {
    /// Start a new table with column alignments.
    pub fn start(&mut self, alignments: Vec<Alignment>) {
        self.alignments = alignments;
        self.in_head = false;
        self.cell_index = 0;
    }

    /// Start the table header row.
    pub fn start_head(&mut self) {
        self.in_head = true;
        self.cell_index = 0;
    }

    /// End the table header row.
    pub fn end_head(&mut self) {
        self.in_head = false;
    }

    /// Start a new table row.
    pub fn start_row(&mut self) {
        self.cell_index = 0;
    }

    /// Move to the next cell.
    pub fn next_cell(&mut self) {
        self.cell_index += 1;
    }

    /// Check if we're in the table header.
    pub fn is_in_head(&self) -> bool {
        self.in_head
    }

    /// Get the alignment style for the current cell.
    pub fn current_alignment_style(&self) -> &'static str {
        match self.alignments.get(self.cell_index) {
            Some(Alignment::Left) => r#" style="text-align:left""#,
            Some(Alignment::Center) => r#" style="text-align:center""#,
            Some(Alignment::Right) => r#" style="text-align:right""#,
            Some(Alignment::None) | None => "",
        }
    }
}

/// State for tracking image alt text capture.
#[derive(Default)]
pub struct ImageState {
    /// Whether we're inside an image tag.
    active: bool,
    /// Buffer for alt text.
    alt_text: String,
}

impl ImageState {
    /// Start capturing image alt text.
    pub fn start(&mut self) {
        self.active = true;
        self.alt_text.clear();
    }

    /// End image capture and return the alt text.
    pub fn end(&mut self) -> String {
        self.active = false;
        std::mem::take(&mut self.alt_text)
    }

    /// Check if we're inside an image.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Append text to the alt text buffer.
    pub fn push_str(&mut self, text: &str) {
        self.alt_text.push_str(text);
    }
}

/// Table of contents entry.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TocEntry {
    /// Heading level (1-6).
    pub level: u8,
    /// Heading text.
    pub title: String,
    /// Anchor ID for linking.
    pub id: String,
}

/// State for tracking heading and title extraction.
#[allow(clippy::struct_excessive_bools)]
pub struct HeadingState {
    /// Whether to extract title from first H1.
    extract_title: bool,
    /// Whether to skip first H1 in output (Confluence mode).
    title_as_metadata: bool,
    /// Extracted title from first H1.
    title: Option<String>,
    /// Whether we've seen the first H1.
    seen_first_h1: bool,
    /// Whether we're currently inside the first H1 (to capture its text).
    in_first_h1: bool,
    /// Current heading level being processed (None if not in a heading).
    current_level: Option<u8>,
    /// Buffer for heading plain text (for table of contents and slug).
    text: String,
    /// Buffer for heading HTML (with inline formatting).
    html: String,
    /// Table of contents entries.
    toc: Vec<TocEntry>,
    /// Counter for generating unique heading IDs.
    id_counts: HashMap<String, usize>,
}

impl HeadingState {
    /// Create a new heading state.
    ///
    /// # Arguments
    ///
    /// * `extract_title` - Whether to extract title from first H1
    /// * `title_as_metadata` - Whether to skip first H1 in output (Confluence mode)
    pub fn new(extract_title: bool, title_as_metadata: bool) -> Self {
        Self {
            extract_title,
            title_as_metadata,
            title: None,
            seen_first_h1: false,
            in_first_h1: false,
            current_level: None,
            text: String::new(),
            html: String::new(),
            toc: Vec::new(),
            id_counts: HashMap::new(),
        }
    }

    /// Check if we're currently inside any heading (not counting skipped H1).
    pub fn is_active(&self) -> bool {
        self.current_level.is_some()
    }

    /// Check if we're inside the first H1 being captured for title.
    pub fn is_in_first_h1(&self) -> bool {
        self.in_first_h1
    }

    /// Start tracking a heading.
    ///
    /// Returns `true` if the heading should be rendered, `false` if it should be skipped.
    pub fn start_heading(&mut self, level: u8) -> bool {
        // First H1 with title extraction and metadata mode - skip rendering
        if self.extract_title && self.title_as_metadata && level == 1 && !self.seen_first_h1 {
            self.in_first_h1 = true;
            self.text.clear();
            return false;
        }

        self.current_level = Some(level);
        self.text.clear();
        self.html.clear();
        true
    }

    /// Get the adjusted heading level for output.
    ///
    /// When title extraction is enabled in metadata mode and we've seen the first H1,
    /// all subsequent headings are leveled up (H2→H1, H3→H2, etc.).
    pub fn adjusted_level(&self, level: u8) -> u8 {
        if self.title_as_metadata && self.seen_first_h1 && level > 1 {
            level - 1
        } else {
            level
        }
    }

    /// Complete the first H1 and save as title.
    pub fn complete_first_h1(&mut self) {
        self.title = Some(self.text.trim().to_string());
        self.text.clear();
        self.in_first_h1 = false;
        self.seen_first_h1 = true;
    }

    /// Complete heading and generate table of contents entry.
    /// Returns (level, id, text, html) or None if not in a heading.
    pub fn complete_heading(&mut self) -> Option<(u8, String, String, String)> {
        let level = self.current_level.take()?;
        let text = std::mem::take(&mut self.text);
        let html = std::mem::take(&mut self.html);

        // Generate unique ID
        let id = self.generate_id(&text);

        // Extract title from first H1 (but still render it in HTML mode)
        let is_title =
            self.extract_title && !self.title_as_metadata && level == 1 && self.title.is_none();
        if is_title {
            self.title = Some(text.trim().to_string());
            self.seen_first_h1 = true;
        }

        // Adjusted level for output
        let adjusted = self.adjusted_level(level);

        // Add to ToC (but not the page title)
        if !is_title {
            self.toc.push(TocEntry {
                level: adjusted,
                title: text.trim().to_string(),
                id: id.clone(),
            });
        }

        Some((adjusted, id, text, html))
    }

    /// Generate a unique ID for a heading.
    fn generate_id(&mut self, text: &str) -> String {
        let base_id = slugify(text);
        let count = self.id_counts.entry(base_id.clone()).or_default();
        let id = match *count {
            0 => base_id,
            n => format!("{base_id}-{n}"),
        };
        *count += 1;
        id
    }

    /// Append text to heading buffers.
    pub fn push_text(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// Append HTML to heading html buffer.
    pub fn push_html(&mut self, html: &str) {
        self.html.push_str(html);
    }

    /// Get the heading HTML buffer reference.
    pub fn html_buffer(&mut self) -> &mut String {
        &mut self.html
    }

    /// Take the extracted title.
    pub fn take_title(&mut self) -> Option<String> {
        self.title.take()
    }

    /// Take the table of contents entries.
    pub fn take_toc(&mut self) -> Vec<TocEntry> {
        std::mem::take(&mut self.toc)
    }
}

/// Convert text to URL-safe slug.
///
/// Converts to lowercase, replaces whitespace/dashes/underscores with single dashes,
/// and removes other non-alphanumeric characters.
#[must_use]
pub fn slugify(text: &str) -> String {
    let mut result = String::new();
    let mut last_was_dash = true; // Prevents leading dash

    for c in text.trim().chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && (c.is_whitespace() || c == '-' || c == '_') {
            result.push('-');
            last_was_dash = true;
        }
    }

    // Remove trailing dash if present
    if result.ends_with('-') {
        result.pop();
    }

    result
}

/// Escape HTML special characters.
#[must_use]
pub fn escape_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#x27;"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("What's New?"), "whats-new");
        assert_eq!(slugify("  Spaces  "), "spaces");
        assert_eq!(slugify("Multiple   Spaces"), "multiple-spaces");
        assert_eq!(slugify("kebab-case"), "kebab-case");
        assert_eq!(slugify("snake_case"), "snake-case");
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html(r#""quoted""#), "&quot;quoted&quot;");
        assert_eq!(escape_html("it's"), "it&#x27;s");
    }

    #[test]
    fn test_code_block_state() {
        let mut state = CodeBlockState::default();
        assert!(!state.is_active());

        state.start(Some("rust".to_string()));
        assert!(state.is_active());

        state.push_str("fn main() {}");
        let (lang, content) = state.end();
        assert_eq!(lang, Some("rust".to_string()));
        assert_eq!(content, "fn main() {}");
        assert!(!state.is_active());
    }

    #[test]
    fn test_table_state() {
        let mut state = TableState::default();
        state.start(vec![Alignment::Left, Alignment::Center, Alignment::Right]);

        state.start_head();
        assert!(state.is_in_head());
        assert_eq!(
            state.current_alignment_style(),
            r#" style="text-align:left""#
        );

        state.next_cell();
        assert_eq!(
            state.current_alignment_style(),
            r#" style="text-align:center""#
        );

        state.next_cell();
        assert_eq!(
            state.current_alignment_style(),
            r#" style="text-align:right""#
        );

        state.end_head();
        assert!(!state.is_in_head());
    }

    #[test]
    fn test_image_state() {
        let mut state = ImageState::default();
        assert!(!state.is_active());

        state.start();
        assert!(state.is_active());

        state.push_str("alt text");
        let alt = state.end();
        assert_eq!(alt, "alt text");
        assert!(!state.is_active());
    }

    #[test]
    fn test_heading_state_html_mode() {
        // HTML mode: extract_title=true, title_as_metadata=false
        // First H1 is extracted as title but still rendered
        let mut state = HeadingState::new(true, false);

        // First H1 should be rendered
        assert!(state.start_heading(1));
        state.push_text("My Title");
        let result = state.complete_heading();
        assert!(result.is_some());
        let (level, id, text, _html) = result.unwrap();
        assert_eq!(level, 1);
        assert_eq!(id, "my-title");
        assert_eq!(text, "My Title");

        // H2 should be rendered with same level
        assert!(state.start_heading(2));
        state.push_text("Section");
        let result = state.complete_heading();
        assert!(result.is_some());
        let (level, _id, _text, _html) = result.unwrap();
        assert_eq!(level, 2); // Not adjusted in HTML mode

        // Check title and ToC at the end (ToC should not include the title)
        assert_eq!(state.take_title(), Some("My Title".to_string()));
        assert_eq!(state.take_toc().len(), 1); // Only H2, not H1 title
    }

    #[test]
    fn test_heading_state_confluence_mode() {
        // Confluence mode: extract_title=true, title_as_metadata=true
        // First H1 is extracted as title and NOT rendered, levels are shifted
        let mut state = HeadingState::new(true, true);

        // First H1 should be skipped
        assert!(!state.start_heading(1));
        assert!(state.is_in_first_h1());
        state.push_text("My Title");
        state.complete_first_h1();

        // H2 should be adjusted to H1
        assert!(state.start_heading(2));
        state.push_text("Section");
        let result = state.complete_heading();
        assert!(result.is_some());
        let (level, _id, _text, _html) = result.unwrap();
        assert_eq!(level, 1); // Adjusted from H2 to H1

        // Check title at the end
        assert_eq!(state.take_title(), Some("My Title".to_string()));
    }
}
