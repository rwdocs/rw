use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct MetaFields {
    #[serde(alias = "type")]
    pub kind: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub vars: HashMap<String, serde_json::Value>,
    pub pages: Option<Vec<String>>,
}

impl MetaFields {
    /// Parse YAML string into `MetaFields`. Returns default on invalid YAML.
    pub(crate) fn from_yaml(yaml: &str) -> Self {
        serde_yaml::from_str(yaml).unwrap_or_default()
    }

    /// Merge `other` onto self. `other` fields win when Some; vars merged at key level.
    pub(crate) fn merge(mut self, other: Self) -> Self {
        self.kind = other.kind.or(self.kind);
        self.title = other.title.or(self.title);
        self.description = other.description.or(self.description);
        self.pages = other.pages.or(self.pages);
        for (key, value) in other.vars {
            self.vars.insert(key, value);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_yaml() {
        let yaml = "title: My Page\nkind: service";
        let fields = MetaFields::from_yaml(yaml);
        assert_eq!(fields.title.as_deref(), Some("My Page"));
        assert_eq!(fields.kind.as_deref(), Some("service"));
    }

    #[test]
    fn parse_type_alias() {
        let yaml = "type: domain";
        let fields = MetaFields::from_yaml(yaml);
        assert_eq!(fields.kind.as_deref(), Some("domain"));
    }

    #[test]
    fn parse_with_vars() {
        let yaml = "vars:\n  owner: team-a\n  priority: 1";
        let fields = MetaFields::from_yaml(yaml);
        assert_eq!(
            fields.vars.get("owner").and_then(|v| v.as_str()),
            Some("team-a")
        );
        assert_eq!(
            fields
                .vars
                .get("priority")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
    }

    #[test]
    fn parse_invalid_yaml_returns_default() {
        let fields = MetaFields::from_yaml(": : invalid: [unclosed");
        assert!(fields.title.is_none());
        assert!(fields.kind.is_none());
        assert!(fields.vars.is_empty());
    }

    #[test]
    fn parse_empty_string_returns_default() {
        let fields = MetaFields::from_yaml("");
        assert!(fields.title.is_none());
        assert!(fields.kind.is_none());
        assert!(fields.vars.is_empty());
    }

    #[test]
    fn merge_overlay_title_wins() {
        let base = MetaFields {
            title: Some("Base Title".to_owned()),
            ..Default::default()
        };
        let overlay = MetaFields {
            title: Some("Overlay Title".to_owned()),
            ..Default::default()
        };
        let merged = base.merge(overlay);
        assert_eq!(merged.title.as_deref(), Some("Overlay Title"));
    }

    #[test]
    fn merge_base_title_when_overlay_none() {
        let base = MetaFields {
            title: Some("Base Title".to_owned()),
            ..Default::default()
        };
        let overlay = MetaFields {
            title: None,
            ..Default::default()
        };
        let merged = base.merge(overlay);
        assert_eq!(merged.title.as_deref(), Some("Base Title"));
    }

    #[test]
    fn merge_vars_key_level() {
        let mut base_vars = HashMap::new();
        base_vars.insert(
            "a".to_owned(),
            serde_json::Value::String("base-a".to_owned()),
        );
        base_vars.insert(
            "b".to_owned(),
            serde_json::Value::String("base-b".to_owned()),
        );

        let mut overlay_vars = HashMap::new();
        overlay_vars.insert(
            "a".to_owned(),
            serde_json::Value::String("overlay-a".to_owned()),
        );
        overlay_vars.insert(
            "c".to_owned(),
            serde_json::Value::String("overlay-c".to_owned()),
        );

        let base = MetaFields {
            vars: base_vars,
            ..Default::default()
        };
        let overlay = MetaFields {
            vars: overlay_vars,
            ..Default::default()
        };
        let merged = base.merge(overlay);

        assert_eq!(
            merged.vars.get("a").and_then(|v| v.as_str()),
            Some("overlay-a"),
            "overlay key wins"
        );
        assert_eq!(
            merged.vars.get("b").and_then(|v| v.as_str()),
            Some("base-b"),
            "base-only key preserved"
        );
        assert_eq!(
            merged.vars.get("c").and_then(|v| v.as_str()),
            Some("overlay-c"),
            "overlay-only key present"
        );
    }

    #[test]
    fn parse_pages() {
        let yaml = "pages:\n  - getting-started\n  - configuration";
        let fields = MetaFields::from_yaml(yaml);
        assert_eq!(
            fields.pages,
            Some(vec![
                "getting-started".to_owned(),
                "configuration".to_owned()
            ])
        );
    }

    #[test]
    fn parse_no_pages_returns_none() {
        let yaml = "title: My Page";
        let fields = MetaFields::from_yaml(yaml);
        assert!(fields.pages.is_none());
    }

    #[test]
    fn merge_overlay_pages_wins() {
        let base = MetaFields {
            pages: Some(vec!["a".to_owned(), "b".to_owned()]),
            ..Default::default()
        };
        let overlay = MetaFields {
            pages: Some(vec!["x".to_owned()]),
            ..Default::default()
        };
        let merged = base.merge(overlay);
        assert_eq!(merged.pages, Some(vec!["x".to_owned()]));
    }

    #[test]
    fn merge_base_pages_when_overlay_none() {
        let base = MetaFields {
            pages: Some(vec!["a".to_owned()]),
            ..Default::default()
        };
        let overlay = MetaFields::default();
        let merged = base.merge(overlay);
        assert_eq!(merged.pages, Some(vec!["a".to_owned()]));
    }

    #[test]
    fn merge_all_fields() {
        let base = MetaFields {
            title: Some("Base Title".to_owned()),
            kind: Some("service".to_owned()),
            description: Some("Base desc".to_owned()),
            ..Default::default()
        };
        let overlay = MetaFields {
            title: None,
            kind: None,
            description: Some("Overlay desc".to_owned()),
            ..Default::default()
        };
        let merged = base.merge(overlay);
        assert_eq!(
            merged.title.as_deref(),
            Some("Base Title"),
            "base title preserved"
        );
        assert_eq!(
            merged.kind.as_deref(),
            Some("service"),
            "base kind preserved"
        );
        assert_eq!(
            merged.description.as_deref(),
            Some("Overlay desc"),
            "overlay description wins"
        );
    }
}
