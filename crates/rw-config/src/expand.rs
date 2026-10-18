//! Environment variable expansion for configuration strings.

use std::env;

use crate::ConfigError;

/// Expand environment variable references in a string.
///
/// Supports:
/// - `${VAR}` - expands to the value of VAR, errors if unset
/// - `${VAR:-default}` - expands to VAR if set, otherwise uses default
/// - `$${` - a literal `${`
///
/// Every other `$`, including bare `$VAR`, is kept as written. An
/// unterminated `${` is an error.
pub(crate) fn expand_env(value: &str, field: &str) -> Result<String, ConfigError> {
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
        match (env::var(name), default) {
            (Ok(val), _) => expanded.push_str(&val),
            (Err(_), Some(default)) => expanded.push_str(default),
            (Err(_), None) => return Err(error(format!("${{{name}}} not set"))),
        }
        rest = after;
    }
    expanded.push_str(rest);
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    #[test]
    fn test_expand_simple_var() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("TEST_VAR_SIMPLE", "hello");
        }
        let result = expand_env("${TEST_VAR_SIMPLE}", "test.field").unwrap();
        assert_eq!(result, "hello");
        unsafe {
            std::env::remove_var("TEST_VAR_SIMPLE");
        }
    }

    #[test]
    fn test_expand_with_default_uses_value() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("TEST_VAR_DEFAULT", "hello");
        }
        let result = expand_env("${TEST_VAR_DEFAULT:-world}", "test.field").unwrap();
        assert_eq!(result, "hello");
        unsafe {
            std::env::remove_var("TEST_VAR_DEFAULT");
        }
    }

    #[test]
    fn test_expand_with_default_uses_default() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::remove_var("UNSET_VAR_TEST");
        }
        let result = expand_env("${UNSET_VAR_TEST:-default}", "test.field").unwrap();
        assert_eq!(result, "default");
    }

    #[test]
    fn test_expand_missing_var_error() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::remove_var("MISSING_VAR_TEST");
        }
        let result = expand_env("${MISSING_VAR_TEST}", "test.field");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_matches!(err, ConfigError::EnvVar { .. });
        assert!(err.to_string().contains("MISSING_VAR_TEST"));
        assert!(err.to_string().contains("test.field"));
    }

    #[test]
    fn test_expand_literal_unchanged() {
        let result = expand_env("literal string", "test.field").unwrap();
        assert_eq!(result, "literal string");
    }

    #[test]
    fn test_expand_embedded_var() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("HOST_TEST", "example.com");
        }
        let result = expand_env("https://${HOST_TEST}/api", "test.url").unwrap();
        assert_eq!(result, "https://example.com/api");
        unsafe {
            std::env::remove_var("HOST_TEST");
        }
    }

    #[test]
    fn test_expand_multiple_vars() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("USER_TEST", "admin");
            std::env::set_var("PASS_TEST", "secret");
        }
        let result = expand_env("${USER_TEST}:${PASS_TEST}", "test.creds").unwrap();
        assert_eq!(result, "admin:secret");
        unsafe {
            std::env::remove_var("USER_TEST");
            std::env::remove_var("PASS_TEST");
        }
    }

    #[test]
    fn test_bare_dollar_not_expanded() {
        // $VAR without braces should not be expanded
        let result = expand_env("$VAR", "test.field").unwrap();
        assert_eq!(result, "$VAR");
    }

    #[test]
    fn test_url_with_dollar_not_expanded() {
        // URLs with dollar signs should work unchanged
        let result = expand_env("https://example.com/$path", "test.url").unwrap();
        assert_eq!(result, "https://example.com/$path");
    }

    #[test]
    fn test_unset_bare_dollar_beside_braced_var_not_expanded() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("HOST_BESIDE_UNSET_TEST", "host");
            std::env::remove_var("UNSET_BARE_TEST");
        }
        let result = expand_env(
            "https://${HOST_BESIDE_UNSET_TEST}/$UNSET_BARE_TEST",
            "test.url",
        );
        assert_eq!(result.unwrap(), "https://host/$UNSET_BARE_TEST");
        unsafe {
            std::env::remove_var("HOST_BESIDE_UNSET_TEST");
        }
    }

    #[test]
    fn test_set_bare_dollar_beside_braced_var_not_expanded() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("HOST_BESIDE_SET_TEST", "host");
            std::env::set_var("SET_BARE_TEST", "oops");
        }
        let result = expand_env("https://${HOST_BESIDE_SET_TEST}/$SET_BARE_TEST", "test.url");
        assert_eq!(result.unwrap(), "https://host/$SET_BARE_TEST");
        unsafe {
            std::env::remove_var("HOST_BESIDE_SET_TEST");
            std::env::remove_var("SET_BARE_TEST");
        }
    }

    #[test]
    fn test_double_dollar_beside_braced_var_kept() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("HOST_BESIDE_DOUBLE_TEST", "host");
        }
        let result = expand_env("${HOST_BESIDE_DOUBLE_TEST}/a$$b", "test.url").unwrap();
        assert_eq!(result, "host/a$$b");
        unsafe {
            std::env::remove_var("HOST_BESIDE_DOUBLE_TEST");
        }
    }

    #[test]
    fn test_escaped_brace_is_literal() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::set_var("ESCAPED_BRACE_TEST", "oops");
        }
        let result = expand_env("a$${ESCAPED_BRACE_TEST}b", "test.field").unwrap();
        assert_eq!(result, "a${ESCAPED_BRACE_TEST}b");
        unsafe {
            std::env::remove_var("ESCAPED_BRACE_TEST");
        }
    }

    #[test]
    fn test_unterminated_brace_error() {
        let err = expand_env("https://${UNTERMINATED_TEST", "test.url").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Environment variable error in test.url: unterminated ${"
        );
    }

    #[test]
    fn test_missing_var_error_message() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::remove_var("MISSING_VAR_MESSAGE_TEST");
        }
        let err = expand_env("x/${MISSING_VAR_MESSAGE_TEST}", "test.field").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Environment variable error in test.field: ${MISSING_VAR_MESSAGE_TEST} not set"
        );
    }

    #[test]
    fn test_empty_default_and_empty_value() {
        // SAFETY: test runs single-threaded per test function
        unsafe {
            std::env::remove_var("UNSET_EMPTY_DEFAULT_TEST");
            std::env::set_var("SET_EMPTY_VALUE_TEST", "");
        }
        let result = expand_env(
            "[${UNSET_EMPTY_DEFAULT_TEST:-}][${SET_EMPTY_VALUE_TEST:-fallback}]",
            "test.field",
        )
        .unwrap();
        assert_eq!(result, "[][]");
        unsafe {
            std::env::remove_var("SET_EMPTY_VALUE_TEST");
        }
    }
}
