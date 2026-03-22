//! Section reference types and utilities for RW.
//!
//! Provides the core types for identifying documentation sections and
//! matching URL paths to sections.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

/// Section identity.
///
/// Represents a documentation section with a kind (e.g., `"domain"`, `"system"`)
/// and a name (last path segment, e.g., `"billing"`). Used in navigation items,
/// breadcrumbs, scope info, and for annotating internal links with
/// `data-section-ref` attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Section {
    /// Section kind (e.g., `"component"`, `"domain"`).
    pub kind: String,
    /// Section name — last path segment (e.g., `"billing"`).
    pub name: String,
}

impl fmt::Display for Section {
    /// Formats as a section reference string (e.g., `"domain:default/billing"`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:default/{}", self.kind, self.name)
    }
}

/// Error returned when parsing a section ref string fails.
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

    /// Parses a section ref string (e.g., `"domain:default/billing"`).
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

/// Map of section root paths to section identities.
///
/// Provides prefix-based matching for resolving internal links to their
/// containing section. Keys are root paths without leading slashes
/// (e.g., `"domains/billing"`).
#[derive(Debug, Default)]
pub struct Sections {
    map: HashMap<String, Section>,
}

impl Sections {
    /// Create from a `HashMap`.
    #[must_use]
    pub fn new(map: HashMap<String, Section>) -> Self {
        Self { map }
    }

    /// Whether the map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Look up a section by exact path.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&Section> {
        self.map.get(path)
    }

    /// Find the deepest section matching a resolved href path.
    ///
    /// Returns the matching `Section` and the remainder path within that section
    /// (empty string for exact matches), or `None` if no section prefix matches.
    ///
    /// Matching is segment-aware: prefix `"domains/bill"` does NOT match path
    /// `/domains/billing`. The section key has no leading slash; the href has one.
    #[must_use]
    pub fn find(&self, href: &str) -> Option<(&Section, String)> {
        let path = href.strip_prefix('/').unwrap_or(href);

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
            path.to_owned()
        } else if path.len() > prefix.len() {
            path[prefix.len() + 1..].to_owned()
        } else {
            String::new()
        };

        Some((section, remainder))
    }

    /// Find the scope path (without leading slash) for a section ref string.
    ///
    /// The ref format is `"kind:default/name"` (e.g., `"domain:default/billing"`).
    /// Returns `None` if the ref is malformed or no section matches.
    /// Linear scan — the map is small.
    #[must_use]
    pub fn find_by_ref(&self, ref_string: &str) -> Option<&str> {
        let target: Section = ref_string.parse().ok()?;
        self.map
            .iter()
            .find(|(_, section)| **section == target)
            .map(|(path, _)| path.as_str())
    }

    /// Resolve an href to section ref attributes for link annotation.
    ///
    /// Given an internal absolute href (e.g., `/domains/billing/api#section`),
    /// finds the matching section and returns `(ref_string, section_path)` suitable
    /// for `data-section-ref` and `data-section-path` HTML attributes.
    ///
    /// Returns `None` for external links (not starting with `/`) or links not
    /// matching any section. Fragments are stripped before matching.
    #[must_use]
    pub fn resolve_ref(&self, href: &str) -> Option<(String, String)> {
        if !href.starts_with('/') {
            return None;
        }

        let path = match href.find('#') {
            Some(pos) => &href[..pos],
            None => href,
        };

        let (section, remainder) = self.find(path)?;

        Some((section.to_string(), remainder))
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
        let (section, path) = sections.find("/domains/billing").unwrap();
        assert_eq!(section.to_string(), "domain:default/billing");
        assert_eq!(path, "");
    }

    #[test]
    fn find_prefix_match_with_remainder() {
        let sections = billing();
        let (section, path) = sections.find("/domains/billing/use-cases").unwrap();
        assert_eq!(section.to_string(), "domain:default/billing");
        assert_eq!(path, "use-cases");
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
        let (section, path) = sections.find("/domains/billing/systems/pay/api").unwrap();
        assert_eq!(section.to_string(), "system:default/pay");
        assert_eq!(path, "api");
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
    fn resolve_ref_skips_external_links() {
        assert!(billing().resolve_ref("https://example.com").is_none());
    }

    #[test]
    fn resolve_ref_skips_fragment_only() {
        assert!(billing().resolve_ref("#section").is_none());
    }

    #[test]
    fn resolve_ref_strips_fragment() {
        let (ref_str, path) = billing()
            .resolve_ref("/domains/billing/api#endpoints")
            .unwrap();
        assert_eq!(ref_str, "domain:default/billing");
        assert_eq!(path, "api");
    }

    #[test]
    fn resolve_ref_no_match() {
        assert!(billing().resolve_ref("/other/path").is_none());
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
