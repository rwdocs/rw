//! Comment marker transfer from old tree to new tree.

use std::collections::HashSet;
use std::ptr;

use super::UnmatchedComment;
use super::matcher::Match;
use super::tree::TreeNode;

/// Transfer markers from matched old nodes to their new counterparts, then
/// place the rest by searching the whole new tree for their text.
///
/// Returns the markers that found no place.
pub fn transfer(
    matches: &[Match<'_>],
    new_tree: &mut TreeNode,
    old_tree: &TreeNode,
) -> Vec<UnmatchedComment> {
    // By identity, not `ac:ref`: a ref can be missing, or shared by the pieces
    // of one comment split across formatting.
    let mut transferred: HashSet<*const TreeNode> = HashSet::new();

    // Inserting a marker can reallocate a node's `children`, moving every node
    // in it — all of them below the target. Walking `matches` (parents first)
    // backwards looks up each target before any insertion can move it.
    for &(old_node, new_ptr) in matches.iter().rev() {
        let markers = old_node.comment_markers();
        if markers.is_empty() {
            continue;
        }
        let Some(target) = find_node_mut_by_ptr(new_tree, new_ptr) else {
            continue;
        };
        for marker in markers {
            if place(target, marker, insert_in_matched) {
                transferred.insert(ptr::from_ref(marker));
            }
        }
    }

    let mut unmatched_comments = Vec::new();
    for marker in find_all_markers(old_tree) {
        if !transferred.contains(&ptr::from_ref(marker))
            && !place(new_tree, marker, insert_anywhere)
        {
            unmatched_comments.push(UnmatchedComment {
                ref_id: marker.marker_ref().unwrap_or_default().to_owned(),
                text: marker.text.clone(),
            });
        }
    }
    unmatched_comments
}

/// Inserts a marker around the given text somewhere in a subtree, handing the
/// marker back if the text is not there.
type Insert = fn(&mut TreeNode, TreeNode, &str) -> Result<(), TreeNode>;

/// Place a copy of `marker`, without children, around its text in `node`'s
/// subtree.
fn place(node: &mut TreeNode, marker: &TreeNode, insert: Insert) -> bool {
    // Trimmed, as the text it is placed around is found trimmed.
    let text = marker.text.trim();
    // Empty text would be found at the start of any text.
    if text.is_empty() {
        return false;
    }
    let copy = TreeNode {
        tag: marker.tag.clone(),
        text: text.to_owned(),
        attrs: marker.attrs.clone(),
        ..TreeNode::default()
    };
    insert(node, copy, text).is_ok()
}

/// Find a mutable node by its original pointer.
fn find_node_mut_by_ptr(node: &mut TreeNode, target_ptr: *const TreeNode) -> Option<&mut TreeNode> {
    if ptr::eq(node, target_ptr) {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_node_mut_by_ptr(child, target_ptr) {
            return Some(found);
        }
    }
    None
}

/// Find all comment markers in a tree.
fn find_all_markers(node: &TreeNode) -> Vec<&TreeNode> {
    let mut markers = Vec::new();
    find_markers_recursive(node, &mut markers);
    markers
}

fn find_markers_recursive<'a>(node: &'a TreeNode, markers: &mut Vec<&'a TreeNode>) {
    if node.is_comment_marker() {
        markers.push(node);
    }
    for child in &node.children {
        find_markers_recursive(child, markers);
    }
}

/// Insert at `node`'s own level, else in the first child whose content holds
/// `text`.
fn insert_in_matched(node: &mut TreeNode, marker: TreeNode, text: &str) -> Result<(), TreeNode> {
    let Err(marker) = insert_at_level(node, marker, text) else {
        return Ok(());
    };
    match node
        .children
        .iter_mut()
        .find(|child| !child.is_comment_marker() && child_content(child).contains(text))
    {
        Some(child) => insert_in_matched(child, marker, text),
        None => Err(marker),
    }
}

/// Insert at the first level, in document order, that holds `text`.
fn insert_anywhere(node: &mut TreeNode, marker: TreeNode, text: &str) -> Result<(), TreeNode> {
    let Err(mut marker) = insert_at_level(node, marker, text) else {
        return Ok(());
    };
    for child in node
        .children
        .iter_mut()
        .filter(|child| !child.is_comment_marker())
    {
        let Err(missed) = insert_anywhere(child, marker, text) else {
            return Ok(());
        };
        marker = missed;
    }
    Err(marker)
}

/// Insert `marker` around `text` in `node`'s own text or in the tail of one of
/// its children other than a marker.
fn insert_at_level(node: &mut TreeNode, mut marker: TreeNode, text: &str) -> Result<(), TreeNode> {
    if let Some(after) = cut_at(&mut node.text, text) {
        marker.tail = after;
        node.children.insert(0, marker);
        return Ok(());
    }
    for i in 0..node.children.len() {
        let child = &mut node.children[i];
        if child.is_comment_marker() {
            continue;
        }
        if let Some(after) = cut_at(&mut child.tail, text) {
            marker.tail = after;
            node.children.insert(i + 1, marker);
            return Ok(());
        }
    }
    Err(marker)
}

/// Truncate `s` before the first `text`, returning what followed `text`.
fn cut_at(s: &mut String, text: &str) -> Option<String> {
    let start = s.find(text)?;
    let after = s[start + text.len()..].to_owned();
    s.truncate(start);
    Some(after)
}

/// Get text content of a child node (excluding tail).
fn child_content(node: &TreeNode) -> String {
    let mut content = node.text.clone();
    for child in &node.children {
        content.push_str(&child.text_signature());
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comment_preservation::parser::parse;
    use crate::comment_preservation::serializer::serialize;

    /// Transfer with the two top-level elements matched to each other.
    fn transfer_first(old_html: &str, new_html: &str) -> (TreeNode, Vec<UnmatchedComment>) {
        let old_tree = parse(old_html).unwrap();
        let mut new_tree = parse(new_html).unwrap();
        let matches = [(&old_tree.children[0], ptr::from_ref(&new_tree.children[0]))];
        let unmatched = transfer(&matches, &mut new_tree, &old_tree);
        (new_tree, unmatched)
    }

    #[test]
    fn test_transfer_marker_in_direct_text() {
        let (new_tree, unmatched) = transfer_first(
            r#"<p><ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker> text</p>"#,
            "<p>marked text</p>",
        );

        assert_eq!(unmatched, [] as [UnmatchedComment; 0]);
        assert_eq!(new_tree.children[0].children.len(), 1);
        assert!(new_tree.children[0].children[0].is_comment_marker());
    }

    #[test]
    fn test_transfer_marker_in_child_tail() {
        let (new_tree, unmatched) = transfer_first(
            r#"<li><code>x</code> <ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker>, rest</li>"#,
            "<li><code>x</code> marked, rest</li>",
        );

        assert_eq!(unmatched, [] as [UnmatchedComment; 0]);
        // Should have code and marker as children
        assert_eq!(new_tree.children[0].children.len(), 2);
        assert_eq!(new_tree.children[0].children[0].tag, "code");
        assert!(new_tree.children[0].children[1].is_comment_marker());
        assert_eq!(new_tree.children[0].children[1].text, "marked");
    }

    /// The marker was a child of the paragraph, so the paragraph's own text is
    /// a closer fit than the same words inside an earlier child.
    #[test]
    fn test_transfer_prefers_the_matched_level_over_a_child() {
        let (new_tree, unmatched) = transfer_first(
            r#"<p><em>word</em><b>a</b> <ac:inline-comment-marker ac:ref="r">word</ac:inline-comment-marker></p>"#,
            "<p><em>word</em><b>a</b> word</p>",
        );

        assert_eq!(unmatched, [] as [UnmatchedComment; 0]);
        assert_eq!(
            serialize(&new_tree),
            r#"<p><em>word</em><b>a</b> <ac:inline-comment-marker ac:ref="r">word</ac:inline-comment-marker></p>"#
        );
    }

    /// A marker transferred with its parent must not stand in for a different
    /// marker that shares its `ac:ref` — here, none at all.
    #[test]
    fn test_transfer_places_each_marker_without_a_ref() {
        let (new_tree, unmatched) = transfer_first(
            "<p><ac:inline-comment-marker>alpha</ac:inline-comment-marker> one</p>\
             <p><ac:inline-comment-marker>beta</ac:inline-comment-marker> two</p>",
            "<p>alpha one</p><p>beta two</p>",
        );

        assert_eq!(unmatched, [] as [UnmatchedComment; 0]);
        let marked: Vec<&str> = new_tree
            .children
            .iter()
            .flat_map(TreeNode::comment_markers)
            .map(|marker| marker.text.as_str())
            .collect();
        assert_eq!(marked, ["alpha", "beta"]);
    }

    #[test]
    fn test_transfer_marker_not_found() {
        let (_, unmatched) = transfer_first(
            r#"<p><ac:inline-comment-marker ac:ref="abc">original</ac:inline-comment-marker></p>"#,
            "<p>completely different text</p>",
        );

        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].text, "original");
    }
}
