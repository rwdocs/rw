//! Section namespace type, validator, and error.
//!
//! [`Namespace`] is a validated wrapper around `String` that guarantees its
//! inner value satisfies the Backstage catalog namespace charset (1–63 chars,
//! starts and ends with an ASCII letter or digit, otherwise only letters,
//! digits, `-`, `_`, or `.`). [`validate_namespace`] is the cheap
//! allocation-free check used by `Namespace::from_str` and by storage
//! backends that want to validate without constructing a `Namespace`.

use std::fmt;
use std::str::FromStr;

/// A validated section namespace (Backstage catalog namespace charset).
///
/// Constructing a [`Namespace`] guarantees the inner value satisfies
/// [`validate_namespace`]: 1–63 characters, starts and ends with an ASCII
/// letter or digit, and otherwise contains only ASCII letters, digits, `-`,
/// `_`, or `.`. Build one via [`FromStr`] (`"payments".parse::<Namespace>()`),
/// [`TryFrom<String>`], or [`Default`] for `"default"`.
///
/// Serializes transparently as a plain string; deserializing validates via
/// `try_from = "String"` — invalid values fail deserialization rather than
/// silently corrupting downstream code.
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct Namespace(String);

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

impl AsRef<str> for Namespace {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for Namespace {
    type Err = InvalidNamespace;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_namespace(s)?;
        Ok(Self(s.to_owned()))
    }
}

impl TryFrom<String> for Namespace {
    type Error = InvalidNamespace;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        validate_namespace(&s)?;
        Ok(Self(s))
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

/// Error returned by [`validate_namespace`] for a malformed namespace value.
///
/// A valid namespace is 1–63 characters, starts and ends with an ASCII letter
/// or digit, and otherwise contains only ASCII letters, digits, `-`, `_`, or
/// `.` — the same charset Backstage allows for catalog namespaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidNamespace {
    namespace: String,
}

impl fmt::Display for InvalidNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid namespace {:?}: must be 1-63 characters, start and end \
             with a letter or digit, and contain only letters, digits, \
             '-', '_', or '.'",
            self.namespace
        )
    }
}

impl std::error::Error for InvalidNamespace {}

/// Validate a string against the Backstage namespace charset.
///
/// Crate-internal: external callers go through [`Namespace::from_str`] (or
/// the equivalent `TryFrom<String>`) — that gives the same validation plus a
/// validated value, without exposing a separate "validate without
/// constructing" API surface.
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
