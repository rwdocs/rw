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
    let mut transferred_refs = HashSet::new();
    let mut unmatched_comments = Vec::new();
    let mut transferred_count = 0;

    // Inserting a marker can reallocate a node's `children`, moving every node
    // in it — all of them below the target. Walking `matches` (parents first)
    // backwards looks up each target before any insertion can move it.
    for &(old_node, new_ptr) in matches.iter().rev() {
        let markers = old_node.comment_markers();
        if markers.is_empty() {
            continue;
        }

        tracing::debug!(count = markers.len(), tag = %old_node.tag, "Transferring markers");

        for marker in markers {
            let ref_id = marker.marker_ref().unwrap_or("").to_owned();
            if transfer_marker_to_ptr(new_tree, new_ptr, marker) {
                transferred_refs.insert(ref_id);
                transferred_count += 1;
            }
        }
    }

    // Markers whose parents were not matched (global fallback)
    for marker in find_all_markers(old_tree) {
        let ref_id = marker.marker_ref().unwrap_or("").to_owned();
        if transferred_refs.contains(&ref_id) {
            continue;
        }

        let text_preview: String = marker.text.chars().take(50).collect();
        tracing::debug!(marker_text = %text_preview, "Parent node not matched for marker");

        if try_global_insert(new_tree, marker) {
            tracing::info!(marker_text = %text_preview, "Fallback: inserted marker via global search");
            transferred_refs.insert(ref_id);
            transferred_count += 1;
        } else {
            tracing::warn!(marker_text = %text_preview, "Could not place marker");
            unmatched_comments.push(UnmatchedComment {
                ref_id,
                text: marker.text.clone(),
            });
        }
    }

    tracing::info!(count = transferred_count, "Transferred comment markers");
    unmatched_comments
}

fn transfer_marker_to_ptr(
    new_tree: &mut TreeNode,
    target_ptr: *const TreeNode,
    marker: &TreeNode,
) -> bool {
    let marker_text = marker.text.trim();
    if marker_text.is_empty() {
        tracing::warn!("Empty comment marker text, skipping");
        return false;
    }

    find_node_mut_by_ptr(new_tree, target_ptr)
        .is_some_and(|target| insert_marker_by_text(target, clone_marker(marker), marker_text))
}

fn try_global_insert(tree: &mut TreeNode, marker: &TreeNode) -> bool {
    let marker_text = marker.text.trim();
    if marker_text.is_empty() {
        return false;
    }

    search_and_insert(tree, clone_marker(marker), marker_text)
}

/// Copy a marker for insertion, without children; the caller sets its tail.
fn clone_marker(marker: &TreeNode) -> TreeNode {
    TreeNode {
        tag: marker.tag.clone(),
        text: marker.text.clone(),
        attrs: marker.attrs.clone(),
        ..TreeNode::default()
    }
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

/// Split text at marker position and return (before, after).
fn split_at_marker(text: &str, marker_text: &str) -> Option<(String, String)> {
    text.find(marker_text).map(|idx| {
        let before = text[..idx].to_string();
        let after = text[idx + marker_text.len()..].to_string();
        (before, after)
    })
}

/// Insert marker by finding matching text in node.
fn insert_marker_by_text(node: &mut TreeNode, mut marker: TreeNode, marker_text: &str) -> bool {
    // Check if marker text appears in node's direct text
    if let Some((before, after)) = split_at_marker(&node.text, marker_text) {
        node.text = before;
        marker.tail = after;
        node.children.insert(0, marker);
        tracing::debug!(tag = %node.tag, "Inserted marker in direct text");
        return true;
    }

    // Check children for matching text (in their content or tail)
    for i in 0..node.children.len() {
        let child = &node.children[i];
        if child.is_comment_marker() {
            continue;
        }

        // Check if marker text appears in this child's tail
        if let Some((before, after)) = split_at_marker(&child.tail, marker_text) {
            node.children[i].tail = before;
            marker.tail = after;
            node.children.insert(i + 1, marker);
            tracing::debug!(tag = %node.children[i].tag, "Inserted marker in tail");
            return true;
        }

        // Check if marker text is in this child's subtree (excluding tail)
        let child_content = get_child_content(&node.children[i]);
        if child_content.contains(marker_text) {
            return insert_marker_by_text(&mut node.children[i], marker, marker_text);
        }
    }

    let text_preview: String = marker_text.chars().take(50).collect();
    tracing::debug!(marker_text = %text_preview, "Could not find position for marker");
    false
}

/// Get text content of a child node (excluding tail).
fn get_child_content(node: &TreeNode) -> String {
    let mut content = node.text.clone();
    for child in &node.children {
        content.push_str(&child.text_signature());
    }
    content
}

/// Recursively search tree and insert marker when text is found.
fn search_and_insert(node: &mut TreeNode, mut marker: TreeNode, marker_text: &str) -> bool {
    if node.is_comment_marker() {
        return false;
    }

    // Check if marker text appears in this node's direct text
    if let Some((before, after)) = split_at_marker(&node.text, marker_text) {
        node.text = before;
        marker.tail = after;
        node.children.insert(0, marker);
        return true;
    }

    // Check children's tails
    for i in 0..node.children.len() {
        if node.children[i].is_comment_marker() {
            continue;
        }

        if let Some((before, after)) = split_at_marker(&node.children[i].tail, marker_text) {
            node.children[i].tail = before;
            marker.tail = after;
            node.children.insert(i + 1, marker);
            return true;
        }
    }

    // Recurse into children
    for child in &mut node.children {
        if search_and_insert(child, clone_marker(&marker), marker_text) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comment_preservation::parser::parse;

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
