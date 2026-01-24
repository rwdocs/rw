//! Tree matching using text similarity.

use std::collections::HashMap;

use super::tree::TreeNode;

/// Similarity threshold for matching nodes (80%).
const SIMILARITY_THRESHOLD: f64 = 0.8;

/// Match nodes between old and new trees.
pub struct TreeMatcher<'a> {
    old_tree: &'a TreeNode,
    new_tree: &'a TreeNode,
}

impl<'a> TreeMatcher<'a> {
    /// Create a new tree matcher.
    #[must_use]
    pub fn new(old_tree: &'a TreeNode, new_tree: &'a TreeNode) -> Self {
        Self { old_tree, new_tree }
    }

    /// Find matching nodes between trees.
    ///
    /// Returns a map from old node pointers to new node pointers.
    #[must_use]
    pub fn find_matches(&self) -> HashMap<*const TreeNode, *const TreeNode> {
        let mut matches = HashMap::new();
        self.match_children(
            &self.old_tree.children,
            &self.new_tree.children,
            &mut matches,
        );
        tracing::info!(count = matches.len(), "Matched nodes between trees");
        matches
    }

    fn match_children(
        &self,
        old_children: &'a [TreeNode],
        new_children: &'a [TreeNode],
        matches: &mut HashMap<*const TreeNode, *const TreeNode>,
    ) {
        // Filter out comment markers from old children
        let old_content: Vec<_> = old_children
            .iter()
            .filter(|c| !c.is_comment_marker())
            .collect();

        // Track which new children have been matched
        let mut matched_new: Vec<bool> = vec![false; new_children.len()];

        // For each old child, find the best matching new child
        for old_child in old_content {
            let mut best_score = SIMILARITY_THRESHOLD;
            let mut best_idx: Option<usize> = None;

            for (idx, new_child) in new_children.iter().enumerate() {
                if matched_new[idx] {
                    continue;
                }

                let score = self.get_match_score(old_child, new_child);
                if score > best_score {
                    best_score = score;
                    best_idx = Some(idx);
                }
            }

            if let Some(idx) = best_idx {
                matched_new[idx] = true;
                self.match_recursive(old_child, &new_children[idx], matches);
            }
        }
    }

    fn match_recursive(
        &self,
        old_node: &'a TreeNode,
        new_node: &'a TreeNode,
        matches: &mut HashMap<*const TreeNode, *const TreeNode>,
    ) {
        let score = self.get_match_score(old_node, new_node);

        if score < SIMILARITY_THRESHOLD {
            return;
        }

        if score < 1.0 {
            tracing::debug!(tag = %old_node.tag, similarity = score, "Partial match");
        }

        matches.insert(old_node as *const TreeNode, new_node as *const TreeNode);
        self.match_children(&old_node.children, &new_node.children, matches);
    }

    fn get_match_score(&self, old_node: &TreeNode, new_node: &TreeNode) -> f64 {
        // Don't match comment markers
        if old_node.is_comment_marker() {
            return -1.0;
        }

        // Tags must match
        if old_node.tag != new_node.tag {
            return -1.0;
        }

        let old_text = old_node.text_signature();
        let new_text = new_node.text_signature();

        text_similarity(&old_text, &new_text)
    }
}

/// Calculate text similarity ratio using longest common subsequence.
fn text_similarity(text1: &str, text2: &str) -> f64 {
    if text1.is_empty() || text2.is_empty() {
        return 0.0;
    }

    if text1 == text2 {
        return 1.0;
    }

    // Use char-based LCS for better Unicode support
    let chars1: Vec<char> = text1.chars().collect();
    let chars2: Vec<char> = text2.chars().collect();
    let len1 = chars1.len();
    let len2 = chars2.len();

    // Optimization: if lengths differ too much, skip LCS calculation
    let max_len = len1.max(len2);
    let min_len = len1.min(len2);
    if (min_len as f64 / max_len as f64) < SIMILARITY_THRESHOLD {
        return min_len as f64 / max_len as f64;
    }

    // Calculate LCS length using dynamic programming
    let lcs_len = lcs_length(&chars1, &chars2);

    // Similarity ratio based on LCS
    (2.0 * lcs_len as f64) / (len1 + len2) as f64
}

/// Calculate LCS length using space-optimized DP (two-row approach).
fn lcs_length(chars1: &[char], chars2: &[char]) -> usize {
    let len2 = chars2.len();

    let mut prev = vec![0usize; len2 + 1];
    let mut curr = vec![0usize; len2 + 1];

    for &c1 in chars1 {
        for (j, &c2) in chars2.iter().enumerate() {
            curr[j + 1] = if c1 == c2 {
                prev[j] + 1
            } else {
                prev[j + 1].max(curr[j])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }

    prev[len2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comment_preservation::parser::ConfluenceXmlParser;

    #[test]
    fn test_match_identical_trees() {
        let parser = ConfluenceXmlParser::new();
        let old_tree = parser.parse("<p>Hello</p>").unwrap();
        let new_tree = parser.parse("<p>Hello</p>").unwrap();

        let matcher = TreeMatcher::new(&old_tree, &new_tree);
        let matches = matcher.find_matches();

        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_match_different_text() {
        let parser = ConfluenceXmlParser::new();
        let old_tree = parser.parse("<p>Hello World</p>").unwrap();
        let new_tree = parser.parse("<p>Completely different</p>").unwrap();

        let matcher = TreeMatcher::new(&old_tree, &new_tree);
        let matches = matcher.find_matches();

        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_match_ignores_comment_markers_in_old() {
        let parser = ConfluenceXmlParser::new();
        let old_html =
            r#"<p><ac:inline-comment-marker ac:ref="x">marked</ac:inline-comment-marker> text</p>"#;
        let new_html = "<p>marked text</p>";

        let old_tree = parser.parse(old_html).unwrap();
        let new_tree = parser.parse(new_html).unwrap();

        let matcher = TreeMatcher::new(&old_tree, &new_tree);
        let matches = matcher.find_matches();

        // p should match
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_text_similarity_identical() {
        assert!((text_similarity("hello", "hello") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_text_similarity_empty() {
        assert!((text_similarity("", "hello")).abs() < f64::EPSILON);
        assert!((text_similarity("hello", "")).abs() < f64::EPSILON);
    }

    #[test]
    fn test_text_similarity_partial() {
        let sim = text_similarity("hello world", "hello there");
        assert!(sim > 0.5);
        assert!(sim < 1.0);
    }
}
