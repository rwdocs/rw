//! Metadata inheritance logic for filesystem storage.
//!
//! Provides functions for building ancestor chains and merging metadata
//! from parent directories to child pages.

use rw_storage::Metadata;

/// Build ancestor chain for a URL path.
///
/// Returns ancestors from root to the path itself.
/// E.g., `"domain/billing/api"` → `["", "domain", "domain/billing", "domain/billing/api"]`
#[must_use]
pub(crate) fn build_ancestor_chain(path: &str) -> Vec<&str> {
    let mut ancestors = vec![""];

    if !path.is_empty() {
        for (i, _) in path.match_indices('/') {
            ancestors.push(&path[..i]);
        }
        ancestors.push(path);
    }

    ancestors
}

/// Merge child metadata with parent metadata following inheritance rules.
///
/// # Inheritance Rules
///
/// - `title`: Never inherited (child's value or `None`)
/// - `description`: Never inherited (child's value or `None`)
/// - `page_type`: Never inherited (child's value or `None`)
/// - `vars`: Deep merged (child values override parent keys)
#[must_use]
pub(crate) fn merge_metadata(parent: &Metadata, child: &Metadata) -> Metadata {
    // Vars: deep merge (parent first, child overrides)
    let mut vars = parent.vars.clone();
    vars.extend(child.vars.iter().map(|(k, v)| (k.clone(), v.clone())));

    Metadata {
        title: child.title.clone(),             // Never inherited
        description: child.description.clone(), // Never inherited
        page_type: child.page_type.clone(),     // Never inherited
        vars,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_build_ancestor_chain_empty_path() {
        let ancestors = build_ancestor_chain("");
        assert_eq!(ancestors, vec![""]);
    }

    #[test]
    fn test_build_ancestor_chain_single_segment() {
        let ancestors = build_ancestor_chain("guide");
        assert_eq!(ancestors, vec!["", "guide"]);
    }

    #[test]
    fn test_build_ancestor_chain_multi_segment() {
        let ancestors = build_ancestor_chain("domain/billing/api");
        assert_eq!(
            ancestors,
            vec!["", "domain", "domain/billing", "domain/billing/api"]
        );
    }

    #[test]
    fn test_merge_empty_parent_and_child() {
        let parent = Metadata::default();
        let child = Metadata::default();
        let merged = merge_metadata(&parent, &child);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_title_not_inherited() {
        let parent = Metadata {
            title: Some("Parent Title".to_string()),
            ..Default::default()
        };
        let child = Metadata::default();
        let merged = merge_metadata(&parent, &child);
        assert!(merged.title.is_none(), "title should not be inherited");
    }

    #[test]
    fn test_merge_title_child_wins() {
        let parent = Metadata {
            title: Some("Parent Title".to_string()),
            ..Default::default()
        };
        let child = Metadata {
            title: Some("Child Title".to_string()),
            ..Default::default()
        };
        let merged = merge_metadata(&parent, &child);
        assert_eq!(merged.title, Some("Child Title".to_string()));
    }

    #[test]
    fn test_merge_description_not_inherited() {
        let parent = Metadata {
            description: Some("Parent description".to_string()),
            ..Default::default()
        };
        let child = Metadata::default();
        let merged = merge_metadata(&parent, &child);
        assert!(
            merged.description.is_none(),
            "description should not be inherited"
        );
    }

    #[test]
    fn test_merge_page_type_not_inherited() {
        let parent = Metadata {
            page_type: Some("domain".to_string()),
            ..Default::default()
        };
        let child = Metadata::default();
        let merged = merge_metadata(&parent, &child);
        assert!(
            merged.page_type.is_none(),
            "page_type should not be inherited"
        );
    }

    #[test]
    fn test_merge_vars_deep_merged() {
        let mut parent_vars = HashMap::new();
        parent_vars.insert("key1".to_string(), serde_json::json!("parent1"));
        parent_vars.insert("key2".to_string(), serde_json::json!("parent2"));

        let mut child_vars = HashMap::new();
        child_vars.insert("key2".to_string(), serde_json::json!("child2"));
        child_vars.insert("key3".to_string(), serde_json::json!("child3"));

        let parent = Metadata {
            vars: parent_vars,
            ..Default::default()
        };
        let child = Metadata {
            vars: child_vars,
            ..Default::default()
        };

        let merged = merge_metadata(&parent, &child);

        assert_eq!(merged.vars.get("key1"), Some(&serde_json::json!("parent1")));
        assert_eq!(merged.vars.get("key2"), Some(&serde_json::json!("child2")));
        assert_eq!(merged.vars.get("key3"), Some(&serde_json::json!("child3")));
    }
}
