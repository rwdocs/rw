use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct MetaFields {
    #[serde(alias = "type")]
    pub kind: Option<String>,
    pub namespace: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub pages: Option<Vec<String>>,
}

impl MetaFields {
    /// Parse YAML string into `MetaFields`. Returns default on invalid YAML.
    pub(crate) fn from_yaml(yaml: &str) -> Self {
        serde_yaml::from_str(yaml).unwrap_or_default()
    }

    /// Merge `other` onto self. `other` fields win when Some.
    pub(crate) fn merge(mut self, other: Self) -> Self {
        self.kind = other.kind.or(self.kind);
        self.namespace = other.namespace.or(self.namespace);
        self.title = other.title.or(self.title);
        self.description = other.description.or(self.description);
        self.pages = other.pages.or(self.pages);
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
    fn parse_invalid_yaml_returns_default() {
        let fields = MetaFields::from_yaml(": : invalid: [unclosed");
        assert!(fields.title.is_none());
        assert!(fields.kind.is_none());
    }

    #[test]
    fn parse_empty_string_returns_default() {
        let fields = MetaFields::from_yaml("");
        assert!(fields.title.is_none());
        assert!(fields.kind.is_none());
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

    #[test]
    fn parse_namespace() {
        let fields = MetaFields::from_yaml("namespace: payments");
        assert_eq!(fields.namespace.as_deref(), Some("payments"));
    }

    #[test]
    fn merge_overlay_namespace_wins() {
        let base = MetaFields {
            namespace: Some("base-ns".to_owned()),
            ..Default::default()
        };
        let overlay = MetaFields {
            namespace: Some("overlay-ns".to_owned()),
            ..Default::default()
        };
        assert_eq!(base.merge(overlay).namespace.as_deref(), Some("overlay-ns"));
    }

    #[test]
    fn merge_base_namespace_when_overlay_none() {
        let base = MetaFields {
            namespace: Some("base-ns".to_owned()),
            ..Default::default()
        };
        let merged = base.merge(MetaFields::default());
        assert_eq!(merged.namespace.as_deref(), Some("base-ns"));
    }
}
