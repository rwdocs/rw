use std::{borrow::Cow, collections::HashSet};

use serde_yaml::{Mapping, Value};

use crate::diagnostic::{Diagnostic, DiagnosticSource, Severity};

#[derive(Debug, Default, PartialEq)]
pub(crate) struct MetaFields {
    pub kind: Option<String>,
    pub namespace: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub pages: Option<Vec<String>>,
    pub name: Option<String>,
}

impl MetaFields {
    /// Extract fields from one YAML source; a failing field drops only itself,
    /// while a source that fails to parse (invalid YAML, non-mapping root)
    /// contributes nothing and yields one `Severity::Error` diagnostic.
    /// Extraction order is fixed (kind, namespace, title, description, pages, name)
    /// so diagnostics come back deterministic.
    pub(crate) fn from_yaml_with_diagnostics(
        yaml: &str,
        source: DiagnosticSource,
    ) -> (Self, Vec<Diagnostic>) {
        let mut diagnostics = Vec::new();

        let value = match serde_yaml::from_str::<Value>(yaml) {
            Ok(Value::Null) => return (Self::default(), diagnostics),
            Ok(value) => value,
            Err(error) => {
                diagnostics.push(Diagnostic {
                    source,
                    field: None,
                    severity: Severity::Error,
                    message: format!("invalid YAML: {error}"),
                });
                return (Self::default(), diagnostics);
            }
        };

        let Some(mapping) = value.as_mapping() else {
            diagnostics.push(Diagnostic {
                source,
                field: None,
                severity: Severity::Error,
                message: format!("expected a mapping, found {}", yaml_type_label(&value)),
            });
            return (Self::default(), diagnostics);
        };

        let mapping = match normalize_known_keys(mapping) {
            Ok(mapping) => mapping,
            Err(field) => {
                diagnostics.push(Diagnostic {
                    source,
                    field: None,
                    severity: Severity::Error,
                    message: format!("duplicate field `{field}`"),
                });
                return (Self::default(), diagnostics);
            }
        };

        let fields = Self {
            kind: string_field(&mapping, "kind", source, &mut diagnostics),
            namespace: namespace_field(&mapping, source, &mut diagnostics),
            title: string_field(&mapping, "title", source, &mut diagnostics),
            description: string_field(&mapping, "description", source, &mut diagnostics),
            pages: pages_field(&mapping, source, &mut diagnostics),
            name: name_field(&mapping, source, &mut diagnostics),
        };
        (fields, diagnostics)
    }

    /// Merge `other` onto self. `other` fields win when Some.
    pub(crate) fn merge(mut self, other: Self) -> Self {
        self.kind = other.kind.or(self.kind);
        self.namespace = other.namespace.or(self.namespace);
        self.title = other.title.or(self.title);
        self.description = other.description.or(self.description);
        self.pages = other.pages.or(self.pages);
        self.name = other.name.or(self.name);
        self
    }
}

const KNOWN_KEYS: [&str; 6] = ["kind", "namespace", "title", "description", "pages", "name"];

fn normalize_known_keys(mapping: &Mapping) -> Result<Cow<'_, Mapping>, &'static str> {
    let mut seen = HashSet::new();
    let mut needs_normalization = false;

    for key in mapping.keys() {
        if let Some(known_key) = known_key(key) {
            if !seen.insert(known_key) {
                return Err(known_key);
            }
            needs_normalization |= matches!(key, Value::Tagged(_));
        }
    }

    if !needs_normalization {
        return Ok(Cow::Borrowed(mapping));
    }

    let normalized = mapping
        .iter()
        .map(|(key, value)| {
            let key =
                known_key(key).map_or_else(|| key.clone(), |key| Value::String(key.to_owned()));
            (key, value.clone())
        })
        .collect();
    Ok(Cow::Owned(normalized))
}

fn known_key(value: &Value) -> Option<&'static str> {
    let Value::String(key) = untag(value) else {
        return None;
    };
    KNOWN_KEYS.iter().copied().find(|known| *known == key)
}

fn untag(mut value: &Value) -> &Value {
    while let Value::Tagged(tagged) = value {
        value = &tagged.value;
    }
    value
}

fn string_field(
    mapping: &Mapping,
    key: &str,
    source: DiagnosticSource,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let value = mapping.get(key)?;
    let Some(text) = scalar_to_string(value) else {
        diagnostics.push(Diagnostic {
            source,
            field: Some(key.to_owned()),
            severity: Severity::Warning,
            message: format!("expected a string, found {}", yaml_type_label(value)),
        });
        return None;
    };
    Some(text)
}

/// Coerce a scalar YAML value to its string form, matching what typed
/// deserialization produced before this parser: strings stay as-is, numbers
/// and booleans are stringified, and a tagged value forwards to its inner
/// scalar. Anything else — null, a list, a mapping, or a tagged non-scalar —
/// yields `None`.
fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Tagged(tagged) => scalar_to_string(&tagged.value),
        Value::Null | Value::Sequence(_) | Value::Mapping(_) => None,
    }
}

fn name_field(
    mapping: &Mapping,
    source: DiagnosticSource,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let raw = string_field(mapping, "name", source, diagnostics)?;
    // Declared names share Namespace's identifier grammar, not its inheritance.
    if raw.parse::<rw_sections::Namespace>().is_ok() {
        return Some(raw);
    }
    diagnostics.push(Diagnostic {
        source,
        field: Some("name".to_owned()),
        severity: Severity::Warning,
        message: format!("invalid name {raw:?}: must be 1-63 characters, start and end with a letter or digit, and contain only letters, digits, '-', '_', or '.'"),
    });
    None
}

fn namespace_field(
    mapping: &Mapping,
    source: DiagnosticSource,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let raw = string_field(mapping, "namespace", source, diagnostics)?;
    match raw.parse::<rw_sections::Namespace>() {
        Ok(_) => Some(raw),
        Err(error) => {
            diagnostics.push(Diagnostic {
                source,
                field: Some("namespace".to_owned()),
                severity: Severity::Warning,
                message: format!("{error}"),
            });
            None
        }
    }
}

fn pages_field(
    mapping: &Mapping,
    source: DiagnosticSource,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<String>> {
    let value = mapping.get("pages")?;
    let Value::Sequence(items) = untag(value) else {
        diagnostics.push(Diagnostic {
            source,
            field: Some("pages".to_owned()),
            severity: Severity::Warning,
            message: format!("expected a list, found {}", yaml_type_label(value)),
        });
        return None;
    };
    // Any entry that fails scalar coercion drops the whole field: a partially
    // recovered list would silently reorder navigation.
    let mut pages = Vec::with_capacity(items.len());
    for item in items {
        let Some(page) = scalar_to_string(item) else {
            diagnostics.push(Diagnostic {
                source,
                field: Some("pages".to_owned()),
                severity: Severity::Warning,
                message: format!("expected string entries, found {}", yaml_type_label(item)),
            });
            return None;
        };
        pages.push(page);
    }
    Some(pages)
}

fn yaml_type_label(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Sequence(_) => "a list",
        Value::Mapping(_) => "a mapping",
        Value::Tagged(_) => "a tagged value",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{Diagnostic, DiagnosticSource, Severity};

    #[test]
    fn parse_valid_yaml() {
        let (fields, _) = extract("title: My Page\nkind: service");
        assert_eq!(fields.title.as_deref(), Some("My Page"));
        assert_eq!(fields.kind.as_deref(), Some("service"));
    }

    #[test]
    fn parse_empty_string_returns_default() {
        let (fields, _) = extract("");
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
        let (fields, _) = extract("pages:\n  - getting-started\n  - configuration");
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
        let (fields, _) = extract("title: My Page");
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

    // --- from_yaml_with_diagnostics ---

    fn extract(yaml: &str) -> (MetaFields, Vec<Diagnostic>) {
        MetaFields::from_yaml_with_diagnostics(yaml, DiagnosticSource::Sidecar)
    }

    #[test]
    fn wrong_typed_title_keeps_sibling_fields() {
        let (fields, diagnostics) = extract("title: [a, b]\nkind: domain");
        assert_eq!(fields.kind.as_deref(), Some("domain"));
        assert_eq!(fields.title, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].field.as_deref(), Some("title"));
        assert_eq!(diagnostics[0].severity, Severity::Warning);
    }

    #[test]
    fn malformed_yaml_is_one_source_level_error() {
        let (fields, diagnostics) = extract(": : invalid: [unclosed");
        assert_eq!(fields, MetaFields::default());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].field, None);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn non_mapping_root_is_one_source_level_error() {
        let (fields, diagnostics) = extract("- a\n- b");
        assert_eq!(fields, MetaFields::default());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert_eq!(diagnostics[0].field, None);
    }

    #[test]
    fn invalid_namespace_is_dropped_with_warning() {
        let (fields, diagnostics) = extract("namespace: bad/value");
        assert_eq!(fields.namespace, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].field.as_deref(), Some("namespace"));
        assert_eq!(diagnostics[0].severity, Severity::Warning);
    }

    #[test]
    fn valid_namespace_is_kept() {
        let (fields, diagnostics) = extract("namespace: payments");
        assert_eq!(fields.namespace.as_deref(), Some("payments"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn pages_with_non_scalar_entry_drops_whole_field() {
        let (fields, diagnostics) = extract("pages:\n  - a\n  - b: c");
        assert_eq!(fields.pages, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].field.as_deref(), Some("pages"));
    }

    #[test]
    fn scalar_pages_is_dropped_with_warning() {
        let (fields, diagnostics) = extract("pages: everything");
        assert_eq!(fields.pages, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].field.as_deref(), Some("pages"));
    }

    #[test]
    fn diagnostics_follow_fixed_field_order() {
        let (_, diagnostics) = extract("description: [y]\ntitle: {a: b}\nkind: [x]");
        let fields: Vec<_> = diagnostics.iter().map(|d| d.field.as_deref()).collect();
        assert_eq!(
            fields,
            vec![Some("kind"), Some("title"), Some("description")]
        );
    }

    #[test]
    fn empty_and_null_sources_are_silent() {
        let (fields, diagnostics) = extract("");
        assert_eq!(fields, MetaFields::default());
        assert!(diagnostics.is_empty());
        let (fields, diagnostics) = extract("null");
        assert_eq!(fields, MetaFields::default());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn unknown_keys_stay_silent() {
        let (fields, diagnostics) = extract("title: T\nfuture_attr: whatever");
        assert_eq!(fields.title.as_deref(), Some("T"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn tagged_known_key_is_recognized() {
        let (fields, diagnostics) = extract("? !foo kind\n: domain\ntitle: Billing");
        assert_eq!(fields.kind.as_deref(), Some("domain"));
        assert_eq!(fields.title.as_deref(), Some("Billing"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn tagged_pages_sequence_is_recognized() {
        let (fields, diagnostics) = extract("pages: !foo [overview, api]\ntitle: Guide");
        assert_eq!(
            fields.pages,
            Some(vec!["overview".to_owned(), "api".to_owned()])
        );
        assert_eq!(fields.title.as_deref(), Some("Guide"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn plain_and_tagged_known_key_collision_is_source_error() {
        let (fields, diagnostics) = extract("kind: service\n? !foo kind\n: domain\ntitle: Billing");
        assert_eq!(fields, MetaFields::default());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].field, None);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn tagged_unknown_key_stays_silent() {
        let (fields, diagnostics) = extract("? !foo future_attr\n: whatever\ntitle: Billing");
        assert_eq!(fields.title.as_deref(), Some("Billing"));
        assert!(diagnostics.is_empty());
    }

    // --- scalar coercion parity with the old typed parser ---

    #[test]
    fn numeric_and_bool_scalars_coerce_to_strings() {
        let (fields, diagnostics) = extract("title: 42\nkind: true");
        assert_eq!(fields.title.as_deref(), Some("42"));
        assert_eq!(fields.kind.as_deref(), Some("true"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn float_scalars_coerce_to_strings() {
        let (fields, diagnostics) = extract("title: 3.14");
        assert_eq!(fields.title.as_deref(), Some("3.14"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn tagged_scalars_coerce_to_their_inner_value() {
        let (fields, diagnostics) = extract("title: !foo bar");
        assert_eq!(fields.title.as_deref(), Some("bar"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn tagged_non_scalar_still_drops_with_warning() {
        let (fields, diagnostics) = extract("title: !foo [a]");
        assert_eq!(fields.title, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].field.as_deref(), Some("title"));
        assert_eq!(diagnostics[0].severity, Severity::Warning);
    }

    #[test]
    fn pages_entries_coerce_scalars_to_strings() {
        let (fields, diagnostics) = extract("pages:\n  - a\n  - 2");
        assert_eq!(fields.pages, Some(vec!["a".to_owned(), "2".to_owned()]));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn null_title_is_dropped_with_warning() {
        let (fields, diagnostics) = extract("title: null");
        assert_eq!(fields.title, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].field.as_deref(), Some("title"));
        assert_eq!(diagnostics[0].severity, Severity::Warning);
    }
}
