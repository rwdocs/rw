//! Section reference types and path-to-section resolution.
//!
//! A **section** is a named subtree of a documentation site. Each section has a
//! freeform [`kind`](Section::kind) (e.g., `"domain"`, `"system"`,
//! `"component"`) and a [`name`](Section::name) derived from the last segment
//! of its scope path (e.g., `"billing"` for path `domains/billing`). Sections
//! let you organize a flat directory of markdown files into a structured
//! hierarchy that other tools can consume programmatically — for example,
//! Backstage can map sections to catalog entities based on their kind.
//!
//! Every section has a canonical **ref string** of the form
//! `"kind:namespace/name"` (e.g., `"domain:default/billing"`). The namespace is
//! currently always `default`.
//!
//! The main entry point is [`Sections`], a map from scope paths to
//! [`Section`] values that supports prefix-based lookup.
//!
//! # Examples
//!
//! ```
//! use std::collections::HashMap;
//! use rw_sections::{Section, Sections};
//!
//! // Build a section map (typically done by rw-site from page metadata)
//! let sections = Sections::new(HashMap::from([
//!     ("domains/billing".to_owned(), Section {
//!         kind: "domain".to_owned(),
//!         name: "billing".to_owned(),
//!     }),
//! ]));
//!
//! // Find which section owns a given page path
//! let sp = sections.find("/domains/billing/api").unwrap();
//! assert_eq!(sp.section.to_string(), "domain:default/billing");
//! assert_eq!(sp.path, "api");
//! ```
//!
//! # Feature flags
//!
//! - **`serde`** — derives `serde::Serialize` on [`Section`] for JSON API
//!   responses.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

/// A named documentation section with a kind and name.
///
/// Represents one node in the section hierarchy. The [`kind`](Self::kind) is a
/// freeform label — any string is valid, though typical values include
/// `"domain"`, `"system"`, and `"component"`. The [`name`](Self::name) is
/// currently derived from the last segment of the section's scope path
/// (e.g., `"billing"` for scope path `domains/billing`).
///
/// Formats as a ref string via [`Display`](fmt::Display)
/// (e.g., `"domain:default/billing"`) and parses back via
/// [`FromStr`].
///
/// # Examples
///
/// ```
/// use rw_sections::Section;
///
/// // Parse a ref string
/// let section: Section = "domain:default/billing".parse()?;
/// assert_eq!(section.kind, "domain");
/// assert_eq!(section.name, "billing");
///
/// // Round-trips through Display
/// assert_eq!(section.to_string(), "domain:default/billing");
/// # Ok::<(), rw_sections::ParseSectionError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Section {
    /// Freeform label classifying this section (e.g., `"domain"`, `"system"`).
    pub kind: String,
    /// Section name, currently the last segment of the scope path (e.g., `"billing"`).
    pub name: String,
}

impl Section {
    /// Name used for sections rooted at the empty scope path.
    pub const ROOT_NAME: &str = "root";

    /// Returns the implicit root section (`section:default/root`).
    ///
    /// Used when a documentation site has pages at the root level that don't
    /// belong to any explicitly defined section.
    ///
    /// # Examples
    ///
    /// ```
    /// use rw_sections::Section;
    ///
    /// let root = Section::root();
    /// assert_eq!(root.kind, "section");
    /// assert_eq!(root.name, "root");
    /// assert_eq!(root.to_string(), "section:default/root");
    /// ```
    #[must_use]
    pub fn root() -> Self {
        Self {
            kind: "section".to_owned(),
            name: Self::ROOT_NAME.to_owned(),
        }
    }
}

impl fmt::Display for Section {
    /// Formats as `"kind:default/name"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:default/{}", self.kind, self.name)
    }
}

/// Error returned when parsing a [`Section`] from a ref string fails.
///
/// The expected format is `"kind:default/name"` where both `kind` and `name`
/// are non-empty.
///
/// # Examples
///
/// ```
/// use rw_sections::Section;
///
/// let err = "invalid".parse::<Section>().unwrap_err();
/// assert_eq!(err.to_string(), "invalid section ref: expected \"kind:default/name\"");
/// ```
#[derive(Debug)]
pub struct ParseSectionError;

impl fmt::Display for ParseSectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid section ref: expected \"kind:default/name\"")
    }
}

impl std::error::Error for ParseSectionError {}

impl FromStr for Section {
    type Err = ParseSectionError;

    /// Parses a ref string in `"kind:default/name"` format.
    ///
    /// # Errors
    ///
    /// Returns [`ParseSectionError`] if the string does not contain
    /// `:default/`, or if either the kind or name segment is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use rw_sections::Section;
    ///
    /// let section: Section = "component:default/auth".parse()?;
    /// assert_eq!(section.kind, "component");
    /// assert_eq!(section.name, "auth");
    /// # Ok::<(), rw_sections::ParseSectionError>(())
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (kind, name) = s
            .split_once(":default/")
            .filter(|(k, n)| !k.is_empty() && !n.is_empty())
            .ok_or(ParseSectionError)?;
        Ok(Self {
            kind: kind.to_owned(),
            name: name.to_owned(),
        })
    }
}

/// Result of [`Sections::find`] — a section match with the remaining path and
/// optional fragment.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use rw_sections::{Section, Sections};
///
/// let sections = Sections::new(HashMap::from([
///     ("domains/billing".to_owned(), Section {
///         kind: "domain".to_owned(),
///         name: "billing".to_owned(),
///     }),
/// ]));
///
/// let sp = sections.find("/domains/billing/api#endpoints").unwrap();
/// assert_eq!(sp.section.to_string(), "domain:default/billing");
/// assert_eq!(sp.path, "api");
/// assert_eq!(sp.fragment, Some("endpoints"));
/// ```
#[derive(Debug)]
pub struct SectionPath<'s, 'h> {
    /// The matched section.
    pub section: &'s Section,
    /// Path within the section (empty string for exact matches).
    pub path: &'h str,
    /// Fragment identifier, if present in the input href.
    pub fragment: Option<&'h str>,
}

/// Map from scope paths to [`Section`] values with prefix-based lookup.
///
/// Scope paths are stored without leading slashes (e.g., `"domains/billing"`).
/// Lookup methods accept href-style paths with leading slashes and perform
/// segment-aware prefix matching — `"domains/bill"` does **not** match
/// `"/domains/billing"`.
///
/// When multiple sections match a path, the deepest (longest prefix) wins.
///
/// Typically built by `rw-site` from page metadata and passed to the renderer
/// and diagram processor for link annotation.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use rw_sections::{Section, Sections};
///
/// let sections = Sections::new(HashMap::from([
///     ("domains/billing".to_owned(), Section {
///         kind: "domain".to_owned(),
///         name: "billing".to_owned(),
///     }),
///     ("domains/billing/systems/pay".to_owned(), Section {
///         kind: "system".to_owned(),
///         name: "pay".to_owned(),
///     }),
/// ]));
///
/// // Deepest match wins
/// let sp = sections.find("/domains/billing/systems/pay/api").unwrap();
/// assert_eq!(sp.section.to_string(), "system:default/pay");
/// assert_eq!(sp.path, "api");
///
/// // Reverse lookup: ref string → scope path
/// let path = sections.find_by_ref("domain:default/billing");
/// assert_eq!(path, Some("domains/billing"));
/// ```
#[derive(Debug, Default)]
pub struct Sections {
    map: HashMap<String, Section>,
}

impl Sections {
    /// Creates a [`Sections`] map from scope-path/section pairs.
    ///
    /// Keys are scope paths without leading slashes (e.g., `"domains/billing"`).
    /// Use an empty string key for the root section.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_sections::{Section, Sections};
    ///
    /// let sections = Sections::new(HashMap::from([
    ///     ("".to_owned(), Section {
    ///         kind: "section".to_owned(),
    ///         name: "root".to_owned(),
    ///     }),
    /// ]));
    /// assert!(!sections.is_empty());
    /// ```
    #[must_use]
    pub fn new(map: HashMap<String, Section>) -> Self {
        Self { map }
    }

    /// Returns `true` if no sections are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns the section at the given scope path, or `None`.
    ///
    /// The `path` must match a key exactly (no prefix matching). Use
    /// [`find`](Self::find) for prefix-based lookup from an href.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_sections::{Section, Sections};
    ///
    /// let sections = Sections::new(HashMap::from([
    ///     ("domains/billing".to_owned(), Section {
    ///         kind: "domain".to_owned(),
    ///         name: "billing".to_owned(),
    ///     }),
    /// ]));
    ///
    /// assert!(sections.get("domains/billing").is_some());
    /// assert!(sections.get("domains/billing/api").is_none());
    /// ```
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&Section> {
        self.map.get(path)
    }

    /// Finds the deepest section whose scope path is a prefix of `href`.
    ///
    /// Returns a [`SectionPath`] with the matching section, the path within
    /// that section, and an optional fragment. Returns `None` if no section
    /// matches.
    ///
    /// The `href` may have a leading slash (it is stripped before matching)
    /// and an optional `#fragment` (it is extracted into
    /// [`SectionPath::fragment`]). Matching is segment-aware: scope path
    /// `"domains/bill"` does **not** match href `"/domains/billing"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_sections::{Section, Sections};
    ///
    /// let sections = Sections::new(HashMap::from([
    ///     ("domains/billing".to_owned(), Section {
    ///         kind: "domain".to_owned(),
    ///         name: "billing".to_owned(),
    ///     }),
    /// ]));
    ///
    /// // Exact match — path is empty
    /// let sp = sections.find("/domains/billing").unwrap();
    /// assert_eq!(sp.path, "");
    /// assert!(sp.fragment.is_none());
    ///
    /// // Prefix match with fragment
    /// let sp = sections.find("/domains/billing/api#endpoints").unwrap();
    /// assert_eq!(sp.path, "api");
    /// assert_eq!(sp.fragment, Some("endpoints"));
    ///
    /// // No partial-segment match
    /// assert!(sections.find("/domains/bill").is_none());
    /// ```
    #[must_use]
    pub fn find<'h>(&self, href: &'h str) -> Option<SectionPath<'_, 'h>> {
        let without_slash = href.strip_prefix('/').unwrap_or(href);

        let (path, fragment) = match without_slash.find('#') {
            Some(pos) => (&without_slash[..pos], Some(&without_slash[pos + 1..])),
            None => (without_slash, None),
        };

        let mut best: Option<(&str, &Section)> = None;

        for (prefix, section) in &self.map {
            let matches = if prefix.is_empty() {
                true
            } else {
                path == prefix.as_str()
                    || (path.starts_with(prefix.as_str())
                        && path.as_bytes().get(prefix.len()) == Some(&b'/'))
            };

            if matches && best.as_ref().is_none_or(|(k, _)| prefix.len() > k.len()) {
                best = Some((prefix.as_str(), section));
            }
        }

        let (prefix, section) = best?;
        let remainder = if prefix.is_empty() {
            path
        } else if path.len() > prefix.len() {
            &path[prefix.len() + 1..]
        } else {
            ""
        };

        Some(SectionPath {
            section,
            path: remainder,
            fragment,
        })
    }

    /// Finds the scope path for a given ref string.
    ///
    /// Parses the ref string (e.g., `"domain:default/billing"`) and returns the
    /// scope path (e.g., `"domains/billing"`) of the matching section, or
    /// `None` if the ref is malformed or no section matches.
    ///
    /// This is a linear scan — the map is expected to be small.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_sections::{Section, Sections};
    ///
    /// let sections = Sections::new(HashMap::from([
    ///     ("domains/billing".to_owned(), Section {
    ///         kind: "domain".to_owned(),
    ///         name: "billing".to_owned(),
    ///     }),
    /// ]));
    ///
    /// assert_eq!(sections.find_by_ref("domain:default/billing"), Some("domains/billing"));
    /// assert_eq!(sections.find_by_ref("system:default/unknown"), None);
    /// ```
    #[must_use]
    pub fn find_by_ref(&self, ref_string: &str) -> Option<&str> {
        let target: Section = ref_string.parse().ok()?;
        self.map
            .iter()
            .find(|(_, section)| **section == target)
            .map(|(path, _)| path.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn billing() -> Sections {
        Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                name: "billing".to_owned(),
            },
        )]))
    }

    #[test]
    fn find_no_match() {
        assert!(billing().find("/other/path").is_none());
    }

    #[test]
    fn find_exact_match() {
        let sections = billing();
        let sp = sections.find("/domains/billing").unwrap();
        assert_eq!(sp.section.to_string(), "domain:default/billing");
        assert_eq!(sp.path, "");
        assert!(sp.fragment.is_none());
    }

    #[test]
    fn find_prefix_match_with_remainder() {
        let sections = billing();
        let sp = sections.find("/domains/billing/use-cases").unwrap();
        assert_eq!(sp.section.to_string(), "domain:default/billing");
        assert_eq!(sp.path, "use-cases");
        assert!(sp.fragment.is_none());
    }

    #[test]
    fn find_deepest_wins() {
        let sections = Sections::new(HashMap::from([
            (
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    name: "billing".to_owned(),
                },
            ),
            (
                "domains/billing/systems/pay".to_owned(),
                Section {
                    kind: "system".to_owned(),
                    name: "pay".to_owned(),
                },
            ),
        ]));
        let sp = sections.find("/domains/billing/systems/pay/api").unwrap();
        assert_eq!(sp.section.to_string(), "system:default/pay");
        assert_eq!(sp.path, "api");
    }

    #[test]
    fn find_no_partial_segment_match() {
        let sections = Sections::new(HashMap::from([(
            "domains/bill".to_owned(),
            Section {
                kind: "domain".to_owned(),
                name: "bill".to_owned(),
            },
        )]));
        assert!(sections.find("/domains/billing").is_none());
    }

    #[test]
    fn find_with_fragment() {
        let sections = billing();
        let sp = sections.find("/domains/billing/api#endpoints").unwrap();
        assert_eq!(sp.section.to_string(), "domain:default/billing");
        assert_eq!(sp.path, "api");
        assert_eq!(sp.fragment, Some("endpoints"));
    }

    #[test]
    fn find_fragment_only_path() {
        let sections = billing();
        let sp = sections.find("/domains/billing#overview").unwrap();
        assert_eq!(sp.path, "");
        assert_eq!(sp.fragment, Some("overview"));
    }

    #[test]
    fn parse_section_valid() {
        let section: Section = "domain:default/billing".parse().unwrap();
        assert_eq!(section.kind, "domain");
        assert_eq!(section.name, "billing");
    }

    #[test]
    fn parse_section_roundtrip() {
        let section = Section {
            kind: "system".to_owned(),
            name: "payments".to_owned(),
        };
        let parsed: Section = section.to_string().parse().unwrap();
        assert_eq!(parsed, section);
    }

    #[test]
    fn parse_section_invalid() {
        assert!("".parse::<Section>().is_err());
        assert!("domain".parse::<Section>().is_err());
        assert!(":default/".parse::<Section>().is_err());
        assert!("domain:default/".parse::<Section>().is_err());
        assert!(":default/billing".parse::<Section>().is_err());
    }

    #[test]
    fn find_by_ref_exact_match() {
        let sections = billing();
        let path = sections.find_by_ref("domain:default/billing");
        assert_eq!(path, Some("domains/billing"));
    }

    #[test]
    fn find_by_ref_no_match() {
        let sections = billing();
        assert!(sections.find_by_ref("system:default/unknown").is_none());
    }
}
