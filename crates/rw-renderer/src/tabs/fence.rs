//! Code fence tracking for directive parsing.
//!
//! Tracks whether we're inside a fenced code block to skip directive syntax
//! (`:::`) that appears within code blocks.

/// Tracks code fence state during line-by-line processing.
///
/// Code fences in `CommonMark` can use backticks or tildes (three or more).
/// The closing fence must use the same character and be at least as long
/// as the opening fence.
#[derive(Debug, Default)]
pub(crate) struct FenceTracker {
    /// Character used for the current fence (backtick or tilde).
    fence_char: Option<char>,
    /// Length of the opening fence (minimum length for closing).
    fence_len: usize,
}

impl FenceTracker {
    /// Create a new fence tracker.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Check if currently inside a fenced code block.
    pub(crate) fn in_fence(&self) -> bool {
        self.fence_char.is_some()
    }

    /// Update fence state based on a line.
    ///
    /// Call this for each line to track fence state. Returns `true` if
    /// the line is a fence marker (opening or closing).
    pub(crate) fn update(&mut self, line: &str) -> bool {
        let trimmed = line.trim_start();

        if let Some(fence_char) = self.fence_char {
            // Check for closing fence
            if is_fence_line(trimmed, fence_char, self.fence_len) {
                self.fence_char = None;
                self.fence_len = 0;
                return true;
            }
            false
        } else {
            // Check for opening fence
            if let Some((ch, len)) = detect_fence(trimmed) {
                self.fence_char = Some(ch);
                self.fence_len = len;
                return true;
            }
            false
        }
    }
}

/// Detect if a line starts a code fence.
///
/// Returns the fence character and length if found.
fn detect_fence(trimmed: &str) -> Option<(char, usize)> {
    let first = trimmed.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }

    let count = trimmed.chars().take_while(|&c| c == first).count();
    if count >= 3 {
        Some((first, count))
    } else {
        None
    }
}

/// Check if a line is a valid closing fence.
///
/// The closing fence must:
/// - Use the same character as opening
/// - Be at least as long as opening
/// - Contain only fence characters (optionally followed by whitespace)
fn is_fence_line(trimmed: &str, expected_char: char, min_len: usize) -> bool {
    let first = match trimmed.chars().next() {
        Some(c) if c == expected_char => c,
        _ => return false,
    };

    let count = trimmed.chars().take_while(|&c| c == first).count();
    if count < min_len {
        return false;
    }

    // After fence chars, only whitespace is allowed
    trimmed[count..].chars().all(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_fence_initially() {
        let tracker = FenceTracker::new();
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_backtick_fence() {
        let mut tracker = FenceTracker::new();

        assert!(tracker.update("```rust"));
        assert!(tracker.in_fence());

        assert!(!tracker.update("fn main() {}"));
        assert!(tracker.in_fence());

        assert!(tracker.update("```"));
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_tilde_fence() {
        let mut tracker = FenceTracker::new();

        assert!(tracker.update("~~~python"));
        assert!(tracker.in_fence());

        assert!(!tracker.update("print('hello')"));
        assert!(tracker.in_fence());

        assert!(tracker.update("~~~"));
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_longer_closing_fence() {
        let mut tracker = FenceTracker::new();

        assert!(tracker.update("```"));
        assert!(tracker.in_fence());

        // Longer closing fence is valid
        assert!(tracker.update("````"));
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_shorter_fence_not_closing() {
        let mut tracker = FenceTracker::new();

        assert!(tracker.update("````"));
        assert!(tracker.in_fence());

        // Shorter fence doesn't close
        assert!(!tracker.update("```"));
        assert!(tracker.in_fence());

        // Same length closes
        assert!(tracker.update("````"));
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_mixed_fence_chars() {
        let mut tracker = FenceTracker::new();

        assert!(tracker.update("```"));
        assert!(tracker.in_fence());

        // Wrong char doesn't close
        assert!(!tracker.update("~~~"));
        assert!(tracker.in_fence());

        assert!(tracker.update("```"));
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_indented_fence() {
        let mut tracker = FenceTracker::new();

        // Indented fence should be detected
        assert!(tracker.update("   ```rust"));
        assert!(tracker.in_fence());

        assert!(tracker.update("  ```"));
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_fence_with_trailing_whitespace() {
        let mut tracker = FenceTracker::new();

        assert!(tracker.update("```  "));
        assert!(tracker.in_fence());

        assert!(tracker.update("```  "));
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_regular_line_no_fence() {
        let mut tracker = FenceTracker::new();

        assert!(!tracker.update("This is a regular line"));
        assert!(!tracker.in_fence());

        assert!(!tracker.update("::: tabs"));
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_two_backticks_not_fence() {
        let mut tracker = FenceTracker::new();

        assert!(!tracker.update("``inline code``"));
        assert!(!tracker.in_fence());
    }

    #[test]
    fn test_multiple_fences() {
        let mut tracker = FenceTracker::new();

        // First fence
        assert!(tracker.update("```"));
        assert!(tracker.in_fence());
        assert!(tracker.update("```"));
        assert!(!tracker.in_fence());

        // Second fence
        assert!(tracker.update("~~~"));
        assert!(tracker.in_fence());
        assert!(tracker.update("~~~"));
        assert!(!tracker.in_fence());
    }
}
