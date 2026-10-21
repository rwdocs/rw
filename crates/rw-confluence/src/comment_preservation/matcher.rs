//! Tree matching using text similarity.

#![allow(clippy::cast_precision_loss)] // Intentional f64 conversion for similarity ratio

use std::cell::OnceCell;
use std::collections::HashMap;

use super::tree::TreeNode;

/// Similarity threshold for matching nodes (80%).
const SIMILARITY_THRESHOLD: f64 = 0.8;

/// Hand each old node and the new node it matches to `on_match`, children
/// before their parents, so `on_match` may change the new node's subtree.
pub fn match_trees<'a>(
    old_tree: &'a TreeNode,
    new_tree: &mut TreeNode,
    on_match: &mut impl FnMut(&'a TreeNode, &mut TreeNode),
) {
    match_children(&old_tree.children, &mut new_tree.children, on_match);
}

fn match_children<'a>(
    old_children: &'a [TreeNode],
    new_children: &mut [TreeNode],
    on_match: &mut impl FnMut(&'a TreeNode, &mut TreeNode),
) {
    let new_signatures: Vec<String> = new_children.iter().map(TreeNode::text_signature).collect();
    let mut matched_new: Vec<bool> = vec![false; new_children.len()];

    for old_child in old_children.iter().filter(|c| !c.is_comment_marker()) {
        let old_signature = old_child.text_signature();
        let pattern = Pattern::new(&old_signature);
        let mut best_score = SIMILARITY_THRESHOLD;
        let mut best_idx: Option<usize> = None;

        for (idx, new_child) in new_children.iter().enumerate() {
            if matched_new[idx] || new_child.tag != old_child.tag {
                continue;
            }

            let score = pattern.similarity(&new_signatures[idx]);
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
            match_children(
                &old_child.children,
                &mut new_children[idx].children,
                on_match,
            );
            on_match(old_child, &mut new_children[idx]);
        }
    }
}

/// A text prepared for LCS similarity against many others.
///
/// The LCS is bit-parallel (Hyyrö 2004): one bit per char of this text, so
/// each char of the other text costs a few word operations.
struct Pattern<'a> {
    text: &'a str,
    len: usize,
    /// Built on first use: most old nodes find an exact match first.
    masks: OnceCell<HashMap<char, Vec<u64>>>,
}

impl<'a> Pattern<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            len: text.chars().count(),
            masks: OnceCell::new(),
        }
    }

    /// Twice the LCS over the combined length, in chars.
    fn similarity(&self, other: &str) -> f64 {
        if self.text.is_empty() || other.is_empty() {
            return 0.0;
        }
        if self.text == other {
            return 1.0;
        }

        let other_len = other.chars().count();
        let max_len = self.len.max(other_len);
        let min_len = self.len.min(other_len);
        if (min_len as f64 / max_len as f64) < SIMILARITY_THRESHOLD {
            return min_len as f64 / max_len as f64;
        }

        (2.0 * self.lcs_length(other) as f64) / (self.len + other_len) as f64
    }

    fn lcs_length(&self, other: &str) -> usize {
        let words = self.len.div_ceil(64);
        let masks = self.masks.get_or_init(|| {
            let mut masks: HashMap<char, Vec<u64>> = HashMap::new();
            for (i, c) in self.text.chars().enumerate() {
                masks.entry(c).or_insert_with(|| vec![0; words])[i / 64] |= 1 << (i % 64);
            }
            masks
        });

        // A zero bit marks a char of this text in the LCS so far. Bits past
        // `len` never match, so they stay one.
        let mut row = vec![u64::MAX; words];
        for c in other.chars() {
            let Some(mask) = masks.get(&c) else {
                continue;
            };
            let mut carry = false;
            for (v, &m) in row.iter_mut().zip(mask) {
                let (sum, c1) = v.overflowing_add(*v & m);
                let (sum, c2) = sum.overflowing_add(u64::from(carry));
                carry = c1 || c2;
                *v = sum | (*v & !m);
            }
        }
        row.iter().map(|v| v.count_zeros() as usize).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comment_preservation::parser::parse;

    fn count_matches(old_html: &str, new_html: &str) -> usize {
        let old_tree = parse(old_html).unwrap();
        let mut new_tree = parse(new_html).unwrap();
        let mut count = 0;
        match_trees(&old_tree, &mut new_tree, &mut |_, _| count += 1);
        count
    }

    #[test]
    fn test_match_identical_trees() {
        assert_eq!(count_matches("<p>Hello</p>", "<p>Hello</p>"), 1);
    }

    #[test]
    fn test_match_different_text() {
        assert_eq!(
            count_matches("<p>Hello World</p>", "<p>Completely different</p>"),
            0
        );
    }

    #[test]
    fn test_match_ignores_comment_markers_in_old() {
        let old_html =
            r#"<p><ac:inline-comment-marker ac:ref="x">marked</ac:inline-comment-marker> text</p>"#;

        // p should match
        assert_eq!(count_matches(old_html, "<p>marked text</p>"), 1);
    }

    #[test]
    fn test_similarity_identical() {
        assert!((Pattern::new("hello").similarity("hello") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_similarity_empty() {
        assert!((Pattern::new("").similarity("hello")).abs() < f64::EPSILON);
        assert!((Pattern::new("hello").similarity("")).abs() < f64::EPSILON);
    }

    fn lcs_length_dp(chars1: &[char], chars2: &[char]) -> usize {
        let mut prev = vec![0; chars2.len() + 1];
        for &c1 in chars1 {
            let mut curr = vec![0; chars2.len() + 1];
            for (j, &c2) in chars2.iter().enumerate() {
                curr[j + 1] = if c1 == c2 {
                    prev[j] + 1
                } else {
                    prev[j + 1].max(curr[j])
                };
            }
            prev = curr;
        }
        prev[chars2.len()]
    }

    #[test]
    fn test_lcs_length_matches_dp_across_word_boundaries() {
        let alphabet: Vec<char> = "abcdeя語 ".chars().collect();
        let mut seed: u64 = 1;
        let mut random_text = |len: usize| -> String {
            (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                    alphabet[(seed >> 33) as usize % alphabet.len()]
                })
                .collect()
        };

        for len1 in [0, 1, 63, 64, 65, 127, 128, 129, 300] {
            for len2 in [0, 1, 64, 65, 200] {
                let text1 = random_text(len1);
                let text2 = random_text(len2);
                let chars1: Vec<char> = text1.chars().collect();
                let chars2: Vec<char> = text2.chars().collect();
                assert_eq!(
                    Pattern::new(&text1).lcs_length(&text2),
                    lcs_length_dp(&chars1, &chars2),
                    "{len1}x{len2}"
                );
            }
        }
    }

    #[test]
    fn test_similarity_partial() {
        let sim = Pattern::new("hello world").similarity("hello there");
        assert!(sim > 0.5);
        assert!(sim < 1.0);
    }
}
