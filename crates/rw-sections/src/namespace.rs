//! Section namespace type, validator, and error.

use std::fmt;
use std::str::FromStr;

/// A validated section namespace (Backstage catalog namespace charset).
///
/// Constructing a [`Namespace`] guarantees the inner value satisfies
/// `validate_namespace`: 1–63 characters, starts and ends with an ASCII
/// letter or digit, and otherwise contains only ASCII letters, digits, `-`,
/// `_`, or `.`. Build one via [`FromStr`] (`"payments".parse::<Namespace>()`)
/// or [`Default`] for `"default"`.
///
/// Serializes as a plain string.
///
/// # Examples
///
/// ```
/// use rw_sections::Namespace;
///
/// let ns: Namespace = "payments".parse().unwrap();
/// assert_eq!(ns.to_string(), "payments");
/// assert_eq!(Namespace::default().to_string(), "default");
/// assert!("bad/value".parse::<Namespace>().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(into = "String"))]
pub struct Namespace(String);

impl Namespace {
    /// The namespace grammar, phrased to follow `"invalid {field} {value:?}: "`.
    pub const GRAMMAR: &str = "must be 1-63 characters, start and end with a letter or digit, \
         and contain only letters, digits, '-', '_', or '.'";
}

impl Default for Namespace {
    /// Returns the default namespace, `"default"`.
    fn default() -> Self {
        Self("default".to_owned())
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Namespace {
    type Err = InvalidNamespace;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_namespace(s)?;
        Ok(Self(s.to_owned()))
    }
}

impl From<Namespace> for String {
    fn from(n: Namespace) -> Self {
        n.0
    }
}

impl PartialEq<str> for Namespace {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Namespace {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Error returned by `validate_namespace` for a value outside the
/// [`Namespace`] grammar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid namespace {namespace:?}: {}", Namespace::GRAMMAR)]
pub struct InvalidNamespace {
    namespace: String,
}

/// Validate a string against the Backstage namespace charset.
fn validate_namespace(namespace: &str) -> Result<(), InvalidNamespace> {
    let invalid = || InvalidNamespace {
        namespace: namespace.to_owned(),
    };
    if namespace.is_empty() || namespace.len() > 63 {
        return Err(invalid());
    }
    let bytes = namespace.as_bytes();
    let edge_ok = |b: u8| b.is_ascii_alphanumeric();
    let mid_ok = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.');
    if !edge_ok(bytes[0]) || !edge_ok(bytes[bytes.len() - 1]) {
        return Err(invalid());
    }
    if !bytes.iter().all(|&b| mid_ok(b)) {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_namespace_accepts_valid() {
        for ns in [
            "default",
            "payments",
            "a",
            "x9",
            "a-b_c.d",
            "Production",
            "ns123",
        ] {
            assert!(validate_namespace(ns).is_ok(), "should accept {ns:?}");
        }
    }

    #[test]
    fn validate_namespace_rejects_invalid() {
        let too_long = "a".repeat(64);
        for ns in [
            "",
            too_long.as_str(),
            "-abc",
            "abc-",
            ".abc",
            "abc.",
            "a/b",
            "a:b",
            "a b",
            "café",
        ] {
            assert!(validate_namespace(ns).is_err(), "should reject {ns:?}");
        }
    }

    #[test]
    fn invalid_namespace_message_names_the_value() {
        let err = validate_namespace("bad/value").unwrap_err();
        assert!(err.to_string().contains("bad/value"));
    }
}
