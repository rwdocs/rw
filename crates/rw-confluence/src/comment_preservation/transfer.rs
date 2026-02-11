//! Comment marker transfer from old tree to new tree.

#![allow(clippy::unused_self)] // Methods take &self for API consistency

use std::collections::{HashMap, HashSet};

use super::UnmatchedComment;
use super::tree::TreeNode;

/// Transfer comment markers from old tree to new tree based on matches.
pub struct CommentMarkerTransfer {
    unmatched_comments: Vec<UnmatchedComment>,
    transferred_refs: HashSet<String>,
}

impl CommentMarkerTransfer {
    /// Create a new transfer tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            unmatched_comments: Vec::new(),
            transferred_refs: HashSet::new(),
        }
    }

    /// Transfer markers from matched old nodes to new nodes.
    ///
    /// Falls back to global text search for markers whose parent nodes
    /// were not matched.
    pub fn transfer(
        &mut self,
        matches: &HashMap<*const TreeNode, *const TreeNode>,
        new_tree: &mut TreeNode,
        old_tree: &TreeNode,
    ) {
        let mut transferred_count = 0;

        // Build mapping: old_ptr -> old_node (for looking up original nodes)
        let old_nodes_by_ptr = collect_nodes_by_ptr(old_tree);

        // Phase 1: Transfer markers from matched nodes
        for (&old_ptr, &new_ptr) in matches {
            let Some(old_node) = old_nodes_by_ptr.get(&old_ptr) else {
                continue;
            };

            let markers = old_node.comment_markers();
            if markers.is_empty() {
                continue;
            }

            tracing::debug!(count = markers.len(), tag = %old_node.tag, "Transferring markers");

            for marker in markers {
                let ref_id = marker.marker_ref().unwrap_or("").to_owned();
                if self.transfer_marker_to_ptr(new_tree, new_ptr, marker) {
                    self.transferred_refs.insert(ref_id);
                    transferred_count += 1;
                }
            }
        }

        // Phase 2: Handle markers whose parents were not matched (global fallback)
        let all_old_markers = find_all_markers(old_tree);
        for marker in all_old_markers {
            let ref_id = marker.marker_ref().unwrap_or("").to_owned();
            if self.transferred_refs.contains(&ref_id) {
                continue;
            }

            // Try global text search
            let text_preview: String = marker.text.chars().take(50).collect();
            tracing::debug!(marker_text = %text_preview, "Parent node not matched for marker");

            if self.try_global_insert(new_tree, marker) {
                let text_preview: String = marker.text.chars().take(30).collect();
                tracing::info!(marker_text = %text_preview, "Fallback: inserted marker via global search");
                self.transferred_refs.insert(ref_id);
                transferred_count += 1;
            } else {
                let text_preview: String = marker.text.chars().take(50).collect();
                tracing::warn!(marker_text = %text_preview, "Could not place marker");
                self.unmatched_comments.push(UnmatchedComment {
                    ref_id,
                    text: marker.text.clone(),
                });
            }
        }

        tracing::info!(count = transferred_count, "Transferred comment markers");
    }

    /// Get comments that couldn't be placed.
    #[must_use]
    pub fn into_unmatched_comments(self) -> Vec<UnmatchedComment> {
        self.unmatched_comments
    }

    fn transfer_marker_to_ptr(
        &self,
        new_tree: &mut TreeNode,
        target_ptr: *const TreeNode,
        marker: &TreeNode,
    ) -> bool {
        let marker_text = marker.text.trim();
        if marker_text.is_empty() {
            tracing::warn!("Empty comment marker text, skipping");
            return false;
        }

        // Find the mutable node by pointer
        if let Some(target_node) = find_node_mut_by_ptr(new_tree, target_ptr) {
            let new_marker = clone_marker(marker);
            return insert_marker_by_text(target_node, new_marker, marker_text);
        }

        false
    }

    fn try_global_insert(&self, tree: &mut TreeNode, marker: &TreeNode) -> bool {
        let marker_text = marker.text.trim();
        if marker_text.is_empty() {
            return false;
        }

        let new_marker = clone_marker(marker);
        search_and_insert(tree, new_marker, marker_text)
    }
}

impl Default for CommentMarkerTransfer {
    fn default() -> Self {
        Self::new()
    }
}

/// Clone a marker node for insertion (without children).
fn clone_marker(marker: &TreeNode) -> TreeNode {
    TreeNode::new(&marker.tag)
        .with_text(&marker.text)
        .with_tail(&marker.tail)
        .with_attrs(marker.attrs.clone())
}

/// Collect all nodes into a map by pointer.
fn collect_nodes_by_ptr(node: &TreeNode) -> HashMap<*const TreeNode, &TreeNode> {
    let mut map = HashMap::new();
    collect_nodes_recursive(node, &mut map);
    map
}

fn collect_nodes_recursive<'a>(
    node: &'a TreeNode,
    map: &mut HashMap<*const TreeNode, &'a TreeNode>,
) {
    map.insert(std::ptr::from_ref::<TreeNode>(node), node);
    for child in &node.children {
        collect_nodes_recursive(child, map);
    }
}

/// Find a mutable node by its original pointer.
fn find_node_mut_by_ptr(node: &mut TreeNode, target_ptr: *const TreeNode) -> Option<&mut TreeNode> {
    if std::ptr::eq(node, target_ptr) {
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
            if insert_marker_by_text(&mut node.children[i], marker, marker_text) {
                return true;
            }
            return false;
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
    use crate::comment_preservation::parser::ConfluenceXmlParser;

    #[test]
    fn test_transfer_marker_in_direct_text() {
        let parser = ConfluenceXmlParser::new();
        let old_html = r#"<p><ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker> text</p>"#;
        let new_html = "<p>marked text</p>";

        let old_tree = parser.parse(old_html).unwrap();
        let mut new_tree = parser.parse(new_html).unwrap();

        // Manually match p nodes
        let old_p = &old_tree.children[0];
        let new_p = &new_tree.children[0];
        let mut matches = HashMap::new();
        matches.insert(
            std::ptr::from_ref::<TreeNode>(old_p),
            std::ptr::from_ref::<TreeNode>(new_p),
        );

        let mut transfer = CommentMarkerTransfer::new();
        transfer.transfer(&matches, &mut new_tree, &old_tree);

        assert!(transfer.unmatched_comments.is_empty());
        assert_eq!(new_tree.children[0].children.len(), 1);
        assert!(new_tree.children[0].children[0].is_comment_marker());
    }

    #[test]
    fn test_transfer_marker_in_child_tail() {
        let parser = ConfluenceXmlParser::new();
        let old_html = r#"<li><code>x</code> <ac:inline-comment-marker ac:ref="abc">marked</ac:inline-comment-marker>, rest</li>"#;
        let new_html = "<li><code>x</code> marked, rest</li>";

        let old_tree = parser.parse(old_html).unwrap();
        let mut new_tree = parser.parse(new_html).unwrap();

        let old_li = &old_tree.children[0];
        let new_li = &new_tree.children[0];
        let mut matches = HashMap::new();
        matches.insert(
            std::ptr::from_ref::<TreeNode>(old_li),
            std::ptr::from_ref::<TreeNode>(new_li),
        );

        let mut transfer = CommentMarkerTransfer::new();
        transfer.transfer(&matches, &mut new_tree, &old_tree);

        assert!(transfer.unmatched_comments.is_empty());
        // Should have code and marker as children
        assert_eq!(new_tree.children[0].children.len(), 2);
        assert_eq!(new_tree.children[0].children[0].tag, "code");
        assert!(new_tree.children[0].children[1].is_comment_marker());
        assert_eq!(new_tree.children[0].children[1].text, "marked");
    }

    #[test]
    fn test_transfer_marker_not_found() {
        let parser = ConfluenceXmlParser::new();
        let old_html =
            r#"<p><ac:inline-comment-marker ac:ref="abc">original</ac:inline-comment-marker></p>"#;
        let new_html = "<p>completely different text</p>";

        let old_tree = parser.parse(old_html).unwrap();
        let mut new_tree = parser.parse(new_html).unwrap();

        let old_p = &old_tree.children[0];
        let new_p = &new_tree.children[0];
        let mut matches = HashMap::new();
        matches.insert(
            std::ptr::from_ref::<TreeNode>(old_p),
            std::ptr::from_ref::<TreeNode>(new_p),
        );

        let mut transfer = CommentMarkerTransfer::new();
        transfer.transfer(&matches, &mut new_tree, &old_tree);

        assert_eq!(transfer.unmatched_comments.len(), 1);
        assert_eq!(transfer.unmatched_comments[0].text, "original");
    }
}
