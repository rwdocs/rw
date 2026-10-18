//! Environment variable expansion for configuration strings.

use std::env;

use crate::ConfigError;

/// Look up `name` in the process environment. A non-UTF-8 value counts as unset.
pub(crate) fn env_lookup(name: &str) -> Option<String> {
    env::var(name).ok()
}

/// Expand variable references in a string, resolving names through `lookup`.
///
/// Supports:
/// - `${VAR}` - expands to the value of VAR, errors if unset
/// - `${VAR:-default}` - expands to VAR if set, otherwise uses default
/// - `$${` - a literal `${`
///
/// Every other `$`, including bare `$VAR`, is kept as written. An
/// unterminated `${` is an error.
pub(crate) fn expand_vars(
    value: &str,
    field: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<String, ConfigError> {
    let error = |message: String| ConfigError::EnvVar {
        field: field.to_owned(),
        message,
    };

    let mut expanded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        let (literal, reference) = rest.split_at(start);
        let reference = &reference[2..];
        if let Some(literal) = literal.strip_suffix('$') {
            expanded.push_str(literal);
            expanded.push_str("${");
            rest = reference;
            continue;
        }
        expanded.push_str(literal);

        let Some((body, after)) = reference.split_once('}') else {
            return Err(error("unterminated ${".to_owned()));
        };
        let (name, default) = match body.split_once(":-") {
            Some((name, default)) if !name.is_empty() => (name, Some(default)),
            _ => (body, None),
        };
        match (lookup(name), default) {
            (Some(val), _) => expanded.push_str(&val),
            (None, Some(default)) => expanded.push_str(default),
            (None, None) => return Err(error(format!("${{{name}}} not set"))),
        }
        rest = after;
    }
    expanded.push_str(rest);
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::collections::HashMap;

    use super::*;

    fn expand(value: &str, vars: &[(&str, &str)]) -> Result<String, ConfigError> {
        let vars: HashMap<&str, &str> = vars.iter().copied().collect();
        expand_vars(value, "test.field", |name| {
            vars.get(name).map(|&val| val.to_owned())
        })
    }

    #[test]
    fn test_expand_simple_var() {
        let result = expand("${VAR}", &[("VAR", "hello")]).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_expand_with_default_uses_value() {
        let result = expand("${VAR:-world}", &[("VAR", "hello")]).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_expand_with_default_uses_default() {
        let result = expand("${UNSET:-default}", &[]).unwrap();
        assert_eq!(result, "default");
    }

    #[test]
    fn test_expand_missing_var_error() {
        let err = expand("x/${MISSING}", &[]).unwrap_err();
        assert_matches!(err, ConfigError::EnvVar { .. });
        assert_eq!(
            err.to_string(),
            "Environment variable error in test.field: ${MISSING} not set"
        );
    }

    #[test]
    fn test_expand_literal_unchanged() {
        let result = expand("literal string", &[]).unwrap();
        assert_eq!(result, "literal string");
    }

    #[test]
    fn test_expand_embedded_var() {
        let result = expand("https://${HOST}/api", &[("HOST", "example.com")]).unwrap();
        assert_eq!(result, "https://example.com/api");
    }

    #[test]
    fn test_expand_multiple_vars() {
        let vars = [("USER", "admin"), ("PASS", "secret")];
        let result = expand("${USER}:${PASS}", &vars).unwrap();
        assert_eq!(result, "admin:secret");
    }

    #[test]
    fn test_expanded_value_not_rescanned() {
        let result = expand("${OUTER}", &[("OUTER", "${INNER}"), ("INNER", "x")]).unwrap();
        assert_eq!(result, "${INNER}");
    }

    #[test]
    fn test_bare_dollar_not_expanded() {
        let result = expand("$VAR", &[("VAR", "oops")]).unwrap();
        assert_eq!(result, "$VAR");
    }

    #[test]
    fn test_url_with_dollar_not_expanded() {
        let result = expand("https://example.com/$path", &[]).unwrap();
        assert_eq!(result, "https://example.com/$path");
    }

    #[test]
    fn test_bare_dollar_beside_braced_var_not_expanded() {
        for vars in [
            &[("HOST", "host")][..],
            &[("HOST", "host"), ("path", "oops")],
        ] {
            let result = expand("https://${HOST}/$path", vars).unwrap();
            assert_eq!(result, "https://host/$path", "{vars:?}");
        }
    }

    #[test]
    fn test_dollar_escapes() {
        let cases = [
            ("$", "$"),
            ("$$", "$$"),
            ("${HOST}$", "host$"),
            ("${HOST}/a$$b", "host/a$$b"),
            ("$${", "${"),
            ("$${A", "${A"),
            ("a$${HOST}b", "a${HOST}b"),
            ("$${A}${HOST}", "${A}host"),
            ("${HOST}$${HOST}", "host${HOST}"),
            // Only the `$` directly before `${` escapes it; earlier ones stay.
            ("$$${HOST}", "$${HOST}"),
            ("$$$${HOST}", "$$${HOST}"),
        ];
        for (input, expected) in cases {
            let result = expand(input, &[("HOST", "host")]).unwrap();
            assert_eq!(result, expected, "{input}");
        }
    }

    #[test]
    fn test_unterminated_brace_error() {
        let err = expand("https://${HOST", &[("HOST", "host")]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Environment variable error in test.field: unterminated ${"
        );
    }

    #[test]
    fn test_empty_default_and_empty_value() {
        let result = expand("[${UNSET:-}][${EMPTY:-fallback}]", &[("EMPTY", "")]).unwrap();
        assert_eq!(result, "[][]");
    }
}
