//! Environment variable expansion for configuration strings.
//!
//! Supports:
//! - `${VAR}` - expands to the value of VAR, errors if unset
//! - `${VAR:-default}` - expands to VAR if set, otherwise uses default

use crate::ConfigError;

/// Expand environment variable references in a string.
///
/// Supports:
/// - `${VAR}` - expands to the value of VAR, errors if unset
/// - `${VAR:-default}` - expands to VAR if set, otherwise uses default
///
/// Returns the original string unchanged if no `${}` patterns are present.
/// Bare `$VAR` syntax is not expanded (only `${VAR}` with braces).
pub(crate) fn expand_env(value: &str, field: &str) -> Result<String, ConfigError> {
    // Fast path: no expansion needed
    if !value.contains("${") {
        return Ok(value.to_string());
    }

    shellexpand::env_with_context(value, |var| -> Result<Option<String>, LookupError> {
        match std::env::var(var) {
            Ok(val) => Ok(Some(val)),
            Err(_) => Err(LookupError {
                var_name: var.to_string(),
            }),
        }
    })
    .map(|cow| cow.into_owned())
    .map_err(|e| ConfigError::EnvVar {
        field: field.to_string(),
        message: format!("${{{0}}} not set", e.cause.var_name),
    })
}

/// Error returned when environment variable lookup fails.
struct LookupError {
    var_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(matches!(err, ConfigError::EnvVar { .. }));
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
}
