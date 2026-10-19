//! Tree matching using text similarity.

#![allow(clippy::cast_precision_loss)] // Intentional f64 conversion for similarity ratio

use std::ptr;

use super::tree::TreeNode;

/// Similarity threshold for matching nodes (80%).
const SIMILARITY_THRESHOLD: f64 = 0.8;

/// An old node and the new node it matched.
///
/// The new side is only an address: the new tree is mutated once matching is
/// done, so it cannot stay borrowed. It is compared, never dereferenced.
pub type Match<'a> = (&'a TreeNode, *const TreeNode);

/// Find matching nodes between trees, parents before their children.
#[must_use]
pub fn find_matches<'a>(old_tree: &'a TreeNode, new_tree: &TreeNode) -> Vec<Match<'a>> {
    let mut matches = Vec::new();
    match_children(&old_tree.children, &new_tree.children, &mut matches);
    matches
}

fn match_children<'a>(
    old_children: &'a [TreeNode],
    new_children: &[TreeNode],
    matches: &mut Vec<Match<'a>>,
) {
    let new_signatures: Vec<String> = new_children.iter().map(TreeNode::text_signature).collect();
    let mut matched_new: Vec<bool> = vec![false; new_children.len()];

    for old_child in old_children.iter().filter(|c| !c.is_comment_marker()) {
        let old_signature = old_child.text_signature();
        let mut best_score = SIMILARITY_THRESHOLD;
        let mut best_idx: Option<usize> = None;

        for (idx, new_child) in new_children.iter().enumerate() {
            if matched_new[idx] || new_child.tag != old_child.tag {
                continue;
            }

            let score = text_similarity(&old_signature, &new_signatures[idx]);
            if score > best_score {
                best_score = score;
                best_idx = Some(idx);
                // Nothing scores higher, and a later tie would not win.
                if score >= 1.0 {
                    break;
                }
            }
        }

        if let Some(idx) = best_idx {
            matched_new[idx] = true;
            matches.push((old_child, ptr::from_ref(&new_children[idx])));
            match_children(&old_child.children, &new_children[idx].children, matches);
        }
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
    use crate::comment_preservation::parser::parse;

    #[test]
    fn test_match_identical_trees() {
        let old_tree = parse("<p>Hello</p>").unwrap();
        let new_tree = parse("<p>Hello</p>").unwrap();

        let matches = find_matches(&old_tree, &new_tree);

        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_match_different_text() {
        let old_tree = parse("<p>Hello World</p>").unwrap();
        let new_tree = parse("<p>Completely different</p>").unwrap();

        let matches = find_matches(&old_tree, &new_tree);

        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_match_ignores_comment_markers_in_old() {
        let old_html =
            r#"<p><ac:inline-comment-marker ac:ref="x">marked</ac:inline-comment-marker> text</p>"#;
        let new_html = "<p>marked text</p>";

        let old_tree = parse(old_html).unwrap();
        let new_tree = parse(new_html).unwrap();

        let matches = find_matches(&old_tree, &new_tree);

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
