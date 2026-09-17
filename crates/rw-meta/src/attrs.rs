use std::collections::BTreeMap;

use serde_json::Value as Json;
use serde_yaml::Value as Yaml;

// Leave room in serde_json's nesting budget for the surrounding wire envelope:
// root, document collection, document, and attrs map.
const MAX_VALUE_NESTING_DEPTH: usize = 127 - 4;

pub(crate) fn from_yaml(value: &Yaml) -> Result<BTreeMap<String, Json>, String> {
    let Yaml::Mapping(values) = value else {
        return Err("expected an attrs mapping".to_owned());
    };
    values
        .iter()
        .map(|entry| to_json_entry(entry, MAX_VALUE_NESTING_DEPTH))
        .collect()
}

fn to_json_entry(
    (key, value): (&Yaml, &Yaml),
    remaining_containers: usize,
) -> Result<(String, Json), String> {
    let Yaml::String(key) = key else {
        return Err("expected string object keys".to_owned());
    };
    Ok((key.clone(), to_json(value, remaining_containers)?))
}

fn to_json(value: &Yaml, remaining_containers: usize) -> Result<Json, String> {
    let remaining_containers = if matches!(value, Yaml::Sequence(_) | Yaml::Mapping(_)) {
        remaining_containers
            .checked_sub(1)
            .ok_or_else(|| "attrs nesting exceeds the supported transport depth".to_owned())?
    } else {
        remaining_containers
    };
    match value {
        Yaml::Null => Ok(Json::Null),
        Yaml::Bool(value) => Ok(Json::Bool(*value)),
        Yaml::String(value) => Ok(Json::String(value.clone())),
        Yaml::Number(value) => {
            let number = if let Some(value) = value.as_i64() {
                Some(serde_json::Number::from(value))
            } else if let Some(value) = value.as_u64() {
                Some(serde_json::Number::from(value))
            } else {
                value.as_f64().and_then(serde_json::Number::from_f64)
            };
            number
                .map(Json::Number)
                .ok_or_else(|| "expected a finite JSON number".to_owned())
        }
        Yaml::Sequence(values) => values
            .iter()
            .map(|value| to_json(value, remaining_containers))
            .collect::<Result<Vec<_>, _>>()
            .map(Json::Array),
        Yaml::Mapping(values) => values
            .iter()
            .map(|entry| to_json_entry(entry, remaining_containers))
            .collect::<Result<serde_json::Map<String, Json>, String>>()
            .map(Json::Object),
        Yaml::Tagged(_) => Err("custom YAML tags are not supported in attrs".to_owned()),
    }
}
