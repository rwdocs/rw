//! Section reference types and path-to-section resolution.
//!
//! A **section** is a named subtree of a documentation site. Each section has a
//! freeform [`kind`](Section::kind) (e.g., `"domain"`, `"system"`,
//! `"component"`) and a [`name`](Section::name) declared in metadata or derived
//! from its section root (e.g., `"billing"` for path `domains/billing`). Sections
//! let you organize a flat directory of markdown files into a structured
//! hierarchy that other tools can consume programmatically — for example,
//! Backstage can map sections to catalog entities based on their kind.
//!
//! Every section has a canonical **ref string** of the form
//! `"kind:namespace/name"` (e.g., `"domain:default/billing"`). The namespace
//! defaults to `"default"` and is set per page via the `namespace` metadata field.
//!
//! The main entry point is [`Sections`], a map from section roots to
//! [`Section`] values that supports prefix-based lookup.
//!
//! # Vocabulary
//!
//! | Term | Example | Description |
//! |------|---------|-------------|
//! | **ref** | `domain:default/billing` | Canonical section identity — `kind:namespace/name`. Serialized by [`Section`]'s `Display`, parsed by its `FromStr`. |
//! | **path** | `domains/billing/api` | Location of a section root or page. No leading slash. |
//! | **refpath** | `domain:billing::api#pricing` | Path expressed in ref terms — `[kind:]name::subpath#fragment`. Parsed by [`Sections::resolve_refpath`]. |
//!
//! # Examples
//!
//! ```
//! use std::collections::HashMap;
//! use rw_sections::{Namespace, Section, Sections};
//!
//! // Build a section map (typically done by rw-site from page metadata)
//! let sections = Sections::new(HashMap::from([
//!     ("domains/billing".to_owned(), Section {
//!         kind: "domain".to_owned(),
//!         namespace: Namespace::default(),
//!         name: "billing".to_owned(),
//!     }),
//! ]));
//!
//! // Find which section owns a given page path
//! let sp = sections.find_page("domains/billing/api").unwrap();
//! assert_eq!(sp.section.to_string(), "domain:default/billing");
//! assert_eq!(sp.path, "api");
//! ```
//!
//! # Feature flags
//!
//! - **`serde`** — derives `serde::Serialize` on [`Section`] and
//!   `serde::Serialize`/`serde::Deserialize` on [`SectionAnchor`].

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::iter::from_fn;
use std::str::FromStr;

mod namespace;
pub use namespace::{InvalidNamespace, Namespace};

/// A named documentation section with a kind and name.
///
/// Represents one node in the section hierarchy. The [`kind`](Self::kind) is a
/// freeform label, typically `"domain"`, `"system"`, or `"component"`, held
/// only to [`is_valid_kind`](Self::is_valid_kind) so the ref parses back. The
/// [`name`](Self::name) is the effective identity: a declared metadata name or
/// the final segment of its section root (e.g., `"billing"` for
/// `domains/billing`; `"root"` at home).
///
/// Formats as a ref string via [`Display`](fmt::Display)
/// (e.g., `"domain:default/billing"`) and parses back via
/// [`FromStr`].
///
/// # Examples
///
/// ```
/// use rw_sections::{Namespace, Section};
///
/// // Parse a ref string
/// let section: Section = "domain:default/billing".parse()?;
/// assert_eq!(section.kind, "domain");
/// assert_eq!(section.namespace, "default");
/// assert_eq!(section.name, "billing");
///
/// // Round-trips through Display
/// assert_eq!(section.to_string(), "domain:default/billing");
/// # Ok::<(), rw_sections::ParseSectionError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Section {
    /// Freeform label classifying this section (e.g., `"domain"`, `"system"`).
    pub kind: String,
    /// Section namespace (e.g., `"default"`, `"payments"`). Inherited down the
    /// page tree from `namespace` metadata.
    pub namespace: Namespace,
    /// Effective section name: declared metadata override or path-derived fallback.
    pub name: String,
}

/// Kind of the implicit root section, and so the kind a refpath without a
/// `kind:` prefix names.
const ROOT_SECTION_KIND: &str = "section";

impl Section {
    /// Name used for the section at the empty section root (the root section).
    pub const ROOT_NAME: &str = "root";

    /// Returns the implicit root section in `namespace` (`section:<namespace>/root`).
    ///
    /// Used when a documentation site has pages at the root level that don't
    /// belong to any explicitly defined section.
    ///
    /// # Examples
    ///
    /// ```
    /// use rw_sections::{Namespace, Section};
    ///
    /// let root = Section::root(Namespace::default());
    /// assert_eq!(root.kind, "section");
    /// assert_eq!(root.name, "root");
    /// assert_eq!(root.to_string(), "section:default/root");
    /// ```
    #[must_use]
    pub fn root(namespace: Namespace) -> Self {
        Self {
            kind: ROOT_SECTION_KIND.to_owned(),
            namespace,
            name: Self::ROOT_NAME.to_owned(),
        }
    }

    /// Returns whether `kind` survives a ref round-trip: the parser splits at
    /// the first `:` and rejects an empty kind.
    ///
    /// # Examples
    ///
    /// ```
    /// use rw_sections::Section;
    ///
    /// assert!(Section::is_valid_kind("team/squad"));
    /// assert!(!Section::is_valid_kind(""));
    /// assert!(!Section::is_valid_kind("a:b"));
    /// ```
    #[must_use]
    pub fn is_valid_kind(kind: &str) -> bool {
        !kind.is_empty() && !kind.contains(':')
    }
}

impl fmt::Display for Section {
    /// Formats as `"kind:namespace/name"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}/{}", self.kind, self.namespace, self.name)
    }
}

/// Error returned when parsing a [`Section`] from a ref string fails.
///
/// The expected format is `"kind:namespace/name"` where `kind`, `namespace`,
/// and `name` are all non-empty. When the namespace segment is present but
/// fails `validate_namespace`, the underlying [`InvalidNamespace`] is
/// carried as the cause, accessible via [`std::error::Error::source`].
///
/// # Examples
///
/// ```
/// use rw_sections::{Namespace, Section};
///
/// let err = "invalid".parse::<Section>().unwrap_err();
/// assert_eq!(err.to_string(), "invalid section ref: expected \"kind:namespace/name\"");
///
/// // Namespace charset violations carry the specific reason as the source.
/// let err = "domain:bad value/billing".parse::<Section>().unwrap_err();
/// let source = std::error::Error::source(&err).unwrap();
/// assert!(source.to_string().contains("bad value"));
/// ```
#[derive(Debug, thiserror::Error)]
pub enum ParseSectionError {
    /// The ref string did not match the `kind:namespace/name` shape, or its
    /// kind or name segment was empty. An empty *namespace* segment is reported
    /// as [`Namespace`](Self::Namespace) instead, since it fails the namespace
    /// charset check.
    #[error("invalid section ref: expected \"kind:namespace/name\"")]
    Format,

    /// The namespace segment was present but failed `validate_namespace` (this
    /// includes an empty namespace); the underlying [`InvalidNamespace`] is
    /// carried as the cause (accessible via [`std::error::Error::source`]).
    #[error("invalid section ref")]
    Namespace(#[from] InvalidNamespace),
}

impl FromStr for Section {
    type Err = ParseSectionError;

    /// Parses a ref string in `"kind:namespace/name"` format.
    ///
    /// # Errors
    ///
    /// Returns [`ParseSectionError`] if the string does not match
    /// `kind:namespace/name`, or if any of kind, namespace, or name is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use rw_sections::{Namespace, Section};
    ///
    /// let section: Section = "component:default/auth".parse()?;
    /// assert_eq!(section.kind, "component");
    /// assert_eq!(section.name, "auth");
    /// # Ok::<(), rw_sections::ParseSectionError>(())
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (kind, rest) = s.split_once(':').ok_or(ParseSectionError::Format)?;
        let (namespace, name) = rest.split_once('/').ok_or(ParseSectionError::Format)?;
        if !Self::is_valid_kind(kind) || name.is_empty() {
            return Err(ParseSectionError::Format);
        }
        let namespace: Namespace = namespace.parse()?;
        Ok(Self {
            kind: kind.to_owned(),
            namespace,
            name: name.to_owned(),
        })
    }
}

/// Result of [`Sections::find_page`] — a section match with the remaining path.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use rw_sections::{Namespace, Section, Sections};
///
/// let sections = Sections::new(HashMap::from([
///     ("domains/billing".to_owned(), Section {
///         kind: "domain".to_owned(),
///         namespace: Namespace::default(),
///         name: "billing".to_owned(),
///     }),
/// ]));
///
/// let sp = sections.find_page("domains/billing/api").unwrap();
/// assert_eq!(sp.section.to_string(), "domain:default/billing");
/// assert_eq!(sp.path, "api");
/// ```
#[derive(Debug)]
pub struct SectionPath<'s, 'h> {
    /// The matched section.
    pub section: &'s Section,
    /// Path within the section (empty for an exact match).
    pub path: &'h str,
}

/// One section yielded by the internal enclosing walk, with the target's path
/// relative to it. Private: the walk backs every [`Sections`] lookup, and each
/// exposes only the piece its callers need.
#[derive(Debug)]
struct Enclosing<'s, 'h> {
    /// The enclosing section.
    section: &'s Section,
    /// The enclosing section's own root (no leading slash; `""` for the root).
    section_root: &'s str,
    /// The target path relative to this section (empty for an exact match; the
    /// full path when only the root encloses it).
    path: &'h str,
}

/// One rung of a path's section ancestry: a section ref and the target path
/// expressed relative to that section's root.
///
/// [`Sections::anchors`] returns these deepest-first (root last), so a caller
/// that can only resolve some of the refs can take the first one it resolves
/// and append its `subpath`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct SectionAnchor {
    /// Section ref string, e.g. `"domain:default/billing"`.
    pub section_ref: String,
    /// Target path relative to this section's root; empty when the target *is*
    /// the section root.
    pub subpath: String,
}

/// Section-root → [`Section`] index with segment-aware prefix lookup and an O(1)
/// reverse (ref → root) index.
///
/// Section roots and page paths have no leading slash (e.g., `"domains/billing"`).
/// Lookups match whole segments — section root `"domains/bill"` does **not**
/// enclose page `"domains/billing"`.
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
/// use rw_sections::{Namespace, Section, Sections};
///
/// let sections = Sections::new(HashMap::from([
///     ("domains/billing".to_owned(), Section {
///         kind: "domain".to_owned(),
///         namespace: Namespace::default(),
///         name: "billing".to_owned(),
///     }),
///     ("domains/billing/systems/pay".to_owned(), Section {
///         kind: "system".to_owned(),
///         namespace: Namespace::default(),
///         name: "pay".to_owned(),
///     }),
/// ]));
///
/// // Deepest match wins
/// let sp = sections.find_page("domains/billing/systems/pay/api").unwrap();
/// assert_eq!(sp.section.to_string(), "system:default/pay");
/// assert_eq!(sp.path, "api");
///
/// // Reverse lookup: ref string → section root
/// let path = sections.find_by_ref("domain:default/billing");
/// assert_eq!(path, Some("domains/billing"));
/// ```
#[derive(Debug, Default)]
pub struct Sections {
    /// Section root → section (no leading slash; `""` is the root section).
    /// Exact lookup, enumeration, and the enclosing-section walk all read this.
    by_path: HashMap<String, Section>,
    /// Canonical section → section root, derived from `by_path` at construction.
    /// Makes [`find_by_ref`](Self::find_by_ref) O(1). Never serialized; rebuilt
    /// whenever a `Sections` is constructed (including cache reload).
    by_ref: HashMap<Section, String>,
}

/// Builds the `section → section root` reverse index from `by_path`.
///
/// On a duplicate identifier the lexicographically-first section root wins, so the
/// index is deterministic regardless of `HashMap` iteration order. A collision
/// is logged, not fatal — section identifiers are assumed unique but not
/// validated at load.
fn build_ref_index(by_path: &HashMap<String, Section>) -> HashMap<Section, String> {
    let mut by_ref: HashMap<Section, String> = HashMap::with_capacity(by_path.len());
    for (path, section) in by_path {
        match by_ref.entry(section.clone()) {
            Entry::Vacant(slot) => {
                slot.insert(path.clone());
            }
            Entry::Occupied(mut slot) => {
                // Keep the lexicographically-smaller path; report the loser.
                let ignored = if path < slot.get() {
                    std::mem::replace(slot.get_mut(), path.clone())
                } else {
                    path.clone()
                };
                tracing::warn!(
                    section_ref = %section,
                    kept = %slot.get(),
                    ignored = %ignored,
                    "duplicate section identifier; keeping the lexicographically-first section root"
                );
            }
        }
    }
    by_ref
}

/// Splits a `#fragment` off at the first `#`. A trailing `#` with nothing
/// after it yields `None`.
fn split_fragment(input: &str) -> (&str, Option<&str>) {
    match input.split_once('#') {
        Some((head, fragment)) => (head, Some(fragment).filter(|f| !f.is_empty())),
        None => (input, None),
    }
}

/// The remainder of `path` after a section `prefix`: `path` itself for the root
/// prefix (`""`), empty for an exact match, otherwise the tail after the joining
/// `/`. `prefix` must be a segment-aligned prefix of `path` (as produced by
/// [`Sections::enclosing_iter`]), so the byte slice always lands on a boundary.
fn relative_to<'h>(path: &'h str, prefix: &str) -> &'h str {
    if prefix.is_empty() {
        path
    } else if path.len() > prefix.len() {
        &path[prefix.len() + 1..]
    } else {
        ""
    }
}

impl Sections {
    /// Creates a [`Sections`] map from section-root/section pairs.
    ///
    /// Keys are section roots without leading slashes (e.g., `"domains/billing"`).
    /// Use an empty string key for the root section.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_sections::{Namespace, Section, Sections};
    ///
    /// let sections = Sections::new(HashMap::from([
    ///     ("".to_owned(), Section {
    ///         kind: "section".to_owned(),
    ///         namespace: Namespace::default(),
    ///         name: "root".to_owned(),
    ///     }),
    /// ]));
    /// assert!(!sections.is_empty());
    /// ```
    #[must_use]
    pub fn new(map: HashMap<String, Section>) -> Self {
        let by_ref = build_ref_index(&map);
        Self {
            by_path: map,
            by_ref,
        }
    }

    /// Creates a [`Sections`] that always resolves.
    ///
    /// If `map` has no entry at the empty section root, inserts the implicit
    /// root section (`section:<root_namespace>/root`) so
    /// [`find_page`](Self::find_page) returns a match for *every* path — a page outside any explicit section
    /// resolves to the root rather than `None`. An explicit root already in
    /// `map` is kept.
    ///
    /// This is the constructor rw uses for the live site: both the section-ref
    /// API and the renderer's link annotation rely on `find_page` being total.
    /// [`new`](Self::new) builds a map without that guarantee, for tests and
    /// any caller that wants a rootless map.
    #[must_use]
    pub fn with_implicit_root(
        mut map: HashMap<String, Section>,
        root_namespace: Namespace,
    ) -> Self {
        map.entry(String::new())
            .or_insert_with(|| Section::root(root_namespace));
        Self::new(map)
    }

    /// Returns `true` if no sections are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }

    /// Returns the section at the given section root, or `None`.
    ///
    /// The `path` must match a key exactly (no prefix matching). Use
    /// [`find_page`](Self::find_page) for the section enclosing a page.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_sections::{Namespace, Section, Sections};
    ///
    /// let sections = Sections::new(HashMap::from([
    ///     ("domains/billing".to_owned(), Section {
    ///         kind: "domain".to_owned(),
    ///         namespace: Namespace::default(),
    ///         name: "billing".to_owned(),
    ///     }),
    /// ]));
    ///
    /// assert!(sections.get("domains/billing").is_some());
    /// assert!(sections.get("domains/billing/api").is_none());
    /// ```
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&Section> {
        self.by_path.get(path)
    }

    /// Returns an iterator over the `(section root, section)` entries, in
    /// arbitrary order.
    ///
    /// For a single lookup use [`get`](Self::get) (exact) or
    /// [`find_page`](Self::find_page) (deepest enclosing). Reach for this only when you
    /// need to visit every entry, e.g. building a secondary index.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Section)> {
        self.by_path
            .iter()
            .map(|(path, section)| (path.as_str(), section))
    }

    /// Finds the deepest section enclosing the page at `path` (no leading
    /// slash), or `None` if no section encloses it.
    ///
    /// A page path is not an href: `#`, `?`, and `%` are part of the name.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_sections::{Namespace, Section, Sections};
    ///
    /// let sections = Sections::new(HashMap::from([
    ///     ("billing".to_owned(), Section {
    ///         kind: "domain".to_owned(),
    ///         namespace: Namespace::default(),
    ///         name: "billing".to_owned(),
    ///     }),
    /// ]));
    ///
    /// // Exact match — path is empty
    /// assert_eq!(sections.find_page("billing").unwrap().path, "");
    ///
    /// assert_eq!(sections.find_page("billing/c#/intro").unwrap().path, "c#/intro");
    ///
    /// // No partial-segment match
    /// assert!(sections.find_page("bill").is_none());
    /// ```
    #[must_use]
    pub fn find_page<'p>(&self, path: &'p str) -> Option<SectionPath<'_, 'p>> {
        let innermost = self.enclosing_iter(path).next()?;
        Some(SectionPath {
            section: innermost.section,
            path: innermost.path,
        })
    }

    /// Iterates the sections enclosing the page at `path`, innermost first and
    /// the root (`""`) last. O(D): walks `path`'s
    /// segment-aligned prefixes from longest to shortest with an O(1) `get` at
    /// each depth, then the root. Only whole `/`-segments are trimmed, so
    /// matching is inherently segment-aware (`domains/bill` never matches
    /// `domains/billing`) and the first yield is the deepest match.
    fn enclosing_iter<'s, 'h>(&'s self, path: &'h str) -> impl Iterator<Item = Enclosing<'s, 'h>> {
        // Candidate sequence: path, its segment-prefixes (longest→shortest),
        // then "" (root) — unless path is already "" (root handled as the path).
        let mut candidate = Some(path);
        from_fn(move || {
            while let Some(cur) = candidate {
                candidate = if cur.is_empty() {
                    None
                } else {
                    Some(cur.rsplit_once('/').map_or("", |(parent, _)| parent))
                };
                if let Some((key, section)) = self.by_path.get_key_value(cur) {
                    return Some(Enclosing {
                        section,
                        section_root: key.as_str(),
                        path: relative_to(path, key.as_str()),
                    });
                }
            }
            None
        })
    }

    /// The sections strictly enclosing the section at `section_root`, nearest-first
    /// with the root last. Excludes a section sitting exactly at `section_root`, so
    /// for a section root this yields its ancestors — the root section has none.
    /// O(D).
    ///
    /// `section_root` is a section root (no leading slash), like
    /// [`get`](Self::get) — not an href. The yielded sections borrow the index,
    /// so nothing is cloned; collect or map as the caller needs.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_sections::{Namespace, Section, Sections};
    ///
    /// let sections = Sections::with_implicit_root(
    ///     HashMap::from([
    ///         ("billing".to_owned(), Section {
    ///             kind: "domain".to_owned(), namespace: Namespace::default(), name: "billing".to_owned(),
    ///         }),
    ///         ("billing/pay".to_owned(), Section {
    ///             kind: "system".to_owned(), namespace: Namespace::default(), name: "pay".to_owned(),
    ///         }),
    ///     ]),
    ///     Namespace::default(),
    /// );
    /// let refs: Vec<String> = sections.ancestors("billing/pay").map(ToString::to_string).collect();
    /// assert_eq!(refs, ["domain:default/billing", "section:default/root"]);
    /// ```
    pub fn ancestors<'s, 'p>(
        &'s self,
        section_root: &'p str,
    ) -> impl Iterator<Item = &'s Section> + use<'s, 'p> {
        self.enclosing_iter(section_root)
            .filter(move |enclosing| enclosing.section_root != section_root)
            .map(|enclosing| enclosing.section)
    }

    /// The nearest section strictly enclosing the section at `section_root`, paired
    /// with that ancestor's own section root — the caller needs the path to build a
    /// URL or look up a title, which the [`Section`] alone can't supply. O(D).
    /// Returns `None` when nothing encloses `section_root` (the root, or a rootless
    /// map).
    ///
    /// `section_root` is a section root (no leading slash), like
    /// [`get`](Self::get) — not an href.
    #[must_use]
    pub fn parent<'s>(&'s self, section_root: &str) -> Option<(&'s Section, &'s str)> {
        self.enclosing_iter(section_root)
            .find(|enclosing| enclosing.section_root != section_root)
            .map(|enclosing| (enclosing.section, enclosing.section_root))
    }

    /// The section-ancestry chain for the page at `path` (no leading slash):
    /// every enclosing section (innermost first, root last) paired with `path`
    /// relative to that section. Generalizes [`find_page`](Self::find_page)
    /// (whose match is this chain's first entry) to the full ordered ancestry.
    /// When the map was built with
    /// [`with_implicit_root`](Self::with_implicit_root) the chain always ends
    /// with the root anchor, so a deepest-first walk over it can never fail. O(D).
    #[must_use]
    pub fn anchors(&self, path: &str) -> Vec<SectionAnchor> {
        self.enclosing_iter(path)
            .map(|enclosing| SectionAnchor {
                section_ref: enclosing.section.to_string(),
                subpath: enclosing.path.to_owned(),
            })
            .collect()
    }

    /// The ancestry chains for the given section refs, keyed by ref — each ref's
    /// own chain (innermost first, root last), exactly what
    /// [`anchors`](Self::anchors) returns for that section's own root. A ref that
    /// doesn't resolve to a section is skipped.
    ///
    /// Builds ancestry for only the given refs rather than the whole site: each
    /// ref costs an O(1) [`find_by_ref`](Self::find_by_ref) plus an O(D)
    /// [`anchors`](Self::anchors).
    #[must_use]
    pub fn ancestry_for<'r>(
        &self,
        refs: impl IntoIterator<Item = &'r str>,
    ) -> BTreeMap<String, Vec<SectionAnchor>> {
        refs.into_iter()
            .filter_map(|ref_string| {
                let root = self.find_by_ref(ref_string)?;
                Some((ref_string.to_owned(), self.anchors(root)))
            })
            .collect()
    }

    /// Finds the section root for a given ref string.
    ///
    /// Parses the ref string (e.g., `"domain:default/billing"`) and returns the
    /// section root (e.g., `"domains/billing"`) of the matching section, or
    /// `None` if the ref is malformed or no section matches.
    ///
    /// This is an O(1) average lookup via a reverse index built at construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use rw_sections::{Namespace, Section, Sections};
    ///
    /// let sections = Sections::new(HashMap::from([
    ///     ("domains/billing".to_owned(), Section {
    ///         kind: "domain".to_owned(),
    ///         namespace: Namespace::default(),
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
        self.by_ref.get(&target).map(String::as_str)
    }

    /// Parse and resolve a refpath to a concrete href and section path.
    ///
    /// `input` is a refpath (e.g., `"domain:billing::overview#tokens"`).
    /// `base_path` is the current page's path (leading slash optional), used to
    /// determine the current section for targets that start with `::`
    /// (current-section links).
    ///
    /// Returns the constructed href and a [`SectionPath`] borrowing from
    /// `self` (section) and `input` (subpath).
    ///
    /// A target without a section name (`::page`, `::#heading`, even a bare
    /// `#heading`) resolves against the current section, so a caller that
    /// treats `#heading` as same-page must catch it first.
    ///
    /// Returns `None` if the target cannot be resolved (unknown section), or
    /// `base_path` is missing for current-section links.
    #[must_use]
    pub fn resolve_refpath<'h>(
        &self,
        input: &'h str,
        base_path: Option<&str>,
    ) -> Option<(String, SectionPath<'_, 'h>)> {
        let target = ParsedRefpath::parse(input);
        let base = base_path.and_then(|bp| self.find_page(bp.strip_prefix('/').unwrap_or(bp)));

        // Through `by_ref` even for the current section: a duplicated
        // identifier must resolve to the same section root as a named link.
        let (section, section_root) = if target.name.is_some() {
            let named = target.section(base.map(|sp| &sp.section.namespace))?;
            self.by_ref.get_key_value(&named)?
        } else {
            self.by_ref.get_key_value(base?.section)?
        };

        let mut href = format!("/{section_root}");
        if let Some(subpath) = target.subpath {
            if !section_root.is_empty() {
                href.push('/');
            }
            href.push_str(subpath);
        }
        if let Some(fragment) = target.fragment {
            href.push('#');
            href.push_str(fragment);
        }

        Some((
            href,
            SectionPath {
                section,
                path: target.subpath.unwrap_or(""),
            },
        ))
    }
}

/// A parsed refpath broken into components.
///
/// See the [module-level vocabulary](crate) for what "refpath" means.
///
/// Format: `[kind:][[namespace/]name][::subpath[#fragment]]`
///
/// # Examples
///
/// - `domain:billing::overview#tokens` → kind="domain", name="billing", subpath="overview", fragment="tokens"
/// - `billing::deep/page` → name="billing", subpath="deep/page"
/// - `::overview` → current section, subpath="overview"
/// - `::#heading` → current section root, fragment="heading"
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedRefpath<'a> {
    /// Section kind (e.g., `"domain"`). `None` defaults to `"section"`.
    kind: Option<&'a str>,
    /// Section namespace. `None` defaults to the current page's namespace.
    namespace: Option<&'a str>,
    /// Section name. `None` means current section (target started with `::`).
    name: Option<&'a str>,
    /// Page subpath within the section. `None` means section root.
    subpath: Option<&'a str>,
    /// Fragment identifier (heading anchor). `None` means no fragment.
    fragment: Option<&'a str>,
}

impl<'a> ParsedRefpath<'a> {
    /// Parse a section target string.
    ///
    /// Parsing rules:
    /// 1. Split off `#fragment` from the end
    /// 2. Split on first `::` → left is ref, right is subpath
    /// 3. If target starts with `::` → current section (`name` is `None`)
    /// 4. Parse ref: split on `:` → kind and name-with-namespace
    /// 5. If no `:` → entire string is the name, kind is `None`
    /// 6. Parse name-with-namespace: split on `/` → namespace/name or just name
    #[must_use]
    pub fn parse(input: &'a str) -> Self {
        let (input, fragment) = split_fragment(input);

        let (section_ref, subpath) = match input.find("::") {
            Some(pos) => {
                let sub = &input[pos + 2..];
                (&input[..pos], if sub.is_empty() { None } else { Some(sub) })
            }
            None => (input, None),
        };

        if section_ref.is_empty() {
            return Self {
                kind: None,
                namespace: None,
                name: None,
                subpath,
                fragment,
            };
        }

        let (kind, name_part) = match section_ref.find(':') {
            Some(pos) => (Some(&section_ref[..pos]), &section_ref[pos + 1..]),
            None => (None, section_ref),
        };

        let (namespace, name) = match name_part.find('/') {
            Some(pos) => (Some(&name_part[..pos]), &name_part[pos + 1..]),
            None => (None, name_part),
        };

        Self {
            kind,
            namespace,
            name: if name.is_empty() { None } else { Some(name) },
            subpath,
            fragment,
        }
    }

    /// The section this refpath names. Kind defaults to `"section"`; namespace
    /// defaults to `current_namespace`, or `"default"` when that is `None`.
    ///
    /// Returns `None` for a current-section target (no `name`) — the caller
    /// resolves those itself — and for an invalid namespace.
    fn section(&self, current_namespace: Option<&Namespace>) -> Option<Section> {
        let name = self.name?;
        let kind = self.kind.unwrap_or(ROOT_SECTION_KIND);
        let namespace = match self.namespace {
            Some(namespace) => namespace.parse().ok()?,
            None => current_namespace.cloned().unwrap_or_default(),
        };
        Some(Section {
            kind: kind.to_owned(),
            namespace,
            name: name.to_owned(),
        })
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
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )]))
    }

    #[test]
    fn find_page_no_match() {
        assert!(billing().find_page("other/path").is_none());
    }

    #[test]
    fn find_page_exact_match() {
        let sections = billing();
        let sp = sections.find_page("domains/billing").unwrap();
        assert_eq!(sp.section.to_string(), "domain:default/billing");
        assert_eq!(sp.path, "");
    }

    #[test]
    fn find_page_prefix_match_with_remainder() {
        let sections = billing();
        let sp = sections.find_page("domains/billing/use-cases").unwrap();
        assert_eq!(sp.section.to_string(), "domain:default/billing");
        assert_eq!(sp.path, "use-cases");
    }

    fn nested_sections() -> Sections {
        Sections::with_implicit_root(
            HashMap::from([
                (
                    "domains/billing".to_owned(),
                    Section {
                        kind: "domain".to_owned(),
                        namespace: Namespace::default(),
                        name: "billing".to_owned(),
                    },
                ),
                (
                    "domains/billing/systems/pay".to_owned(),
                    Section {
                        kind: "system".to_owned(),
                        namespace: Namespace::default(),
                        name: "pay".to_owned(),
                    },
                ),
            ]),
            Namespace::default(),
        )
    }

    #[test]
    fn find_page_handles_degenerate_paths_without_panic() {
        let sections = nested_sections();
        // None of these match an explicit section, so they resolve to the
        // implicit root — and the byte-index slicing must not panic on empty,
        // slash-only, doubled-slash, or trailing-slash inputs.
        for path in ["", "/", "//", "a//b", "a/"] {
            assert_eq!(
                sections.find_page(path).unwrap().section.to_string(),
                "section:default/root",
                "path {path:?} should resolve to the root section"
            );
        }
    }

    #[test]
    fn ancestors_excludes_self_and_ends_at_root() {
        let sections = nested_sections();

        // Nested section: ancestors are billing then root, self (pay) excluded.
        let refs: Vec<String> = sections
            .ancestors("domains/billing/systems/pay")
            .map(ToString::to_string)
            .collect();
        assert_eq!(refs, ["domain:default/billing", "section:default/root"]);

        // Top-level section: only the root.
        let refs: Vec<String> = sections
            .ancestors("domains/billing")
            .map(ToString::to_string)
            .collect();
        assert_eq!(refs, ["section:default/root"]);

        // The root section has no ancestors.
        assert_eq!(sections.ancestors("").count(), 0);
    }

    #[test]
    fn parent_is_nearest_enclosing_with_section_root() {
        let sections = nested_sections();

        // A nested section's parent is billing, carrying its section root.
        let (section, section_root) = sections.parent("domains/billing/systems/pay").unwrap();
        assert_eq!(section.to_string(), "domain:default/billing");
        assert_eq!(section_root, "domains/billing");

        // A top-level section's parent is the root (empty section root).
        let (section, section_root) = sections.parent("domains/billing").unwrap();
        assert_eq!(section.to_string(), "section:default/root");
        assert_eq!(section_root, "");

        // The root has no parent.
        assert!(sections.parent("").is_none());
    }

    #[test]
    fn anchors_are_deepest_first_root_last_with_relative_subpaths() {
        let sections = nested_sections();

        // For a page: each enclosing section with the path relative to it,
        // innermost first, root last.
        let anchors = sections.anchors("domains/billing/systems/pay/api");
        assert_eq!(
            anchors,
            vec![
                SectionAnchor {
                    section_ref: "system:default/pay".to_owned(),
                    subpath: "api".to_owned(),
                },
                SectionAnchor {
                    section_ref: "domain:default/billing".to_owned(),
                    subpath: "systems/pay/api".to_owned(),
                },
                SectionAnchor {
                    section_ref: "section:default/root".to_owned(),
                    subpath: "domains/billing/systems/pay/api".to_owned(),
                },
            ]
        );

        // For a section's own root the innermost anchor has an empty subpath.
        assert_eq!(
            sections.anchors("domains/billing")[0],
            SectionAnchor {
                section_ref: "domain:default/billing".to_owned(),
                subpath: String::new(),
            }
        );
    }

    #[test]
    fn ancestry_for_returns_only_requested_refs_chains() {
        let sections = nested_sections();
        // Only the requested refs get chains; unrelated sections are absent.
        let map = sections.ancestry_for(["system:default/pay", "section:default/root"]);
        assert_eq!(map.len(), 2);
        assert!(!map.contains_key("domain:default/billing"));
        assert_eq!(
            map["system:default/pay"],
            vec![
                SectionAnchor {
                    section_ref: "system:default/pay".to_owned(),
                    subpath: String::new(),
                },
                SectionAnchor {
                    section_ref: "domain:default/billing".to_owned(),
                    subpath: "systems/pay".to_owned(),
                },
                SectionAnchor {
                    section_ref: "section:default/root".to_owned(),
                    subpath: "domains/billing/systems/pay".to_owned(),
                },
            ]
        );
        assert_eq!(
            map["section:default/root"],
            vec![SectionAnchor {
                section_ref: "section:default/root".to_owned(),
                subpath: String::new(),
            }]
        );
        // An unresolvable ref is skipped, not an error.
        assert!(sections.ancestry_for(["nope:default/x"]).is_empty());
    }

    #[test]
    fn find_page_deepest_wins() {
        let sections = Sections::new(HashMap::from([
            (
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    namespace: Namespace::default(),
                    name: "billing".to_owned(),
                },
            ),
            (
                "domains/billing/systems/pay".to_owned(),
                Section {
                    kind: "system".to_owned(),
                    namespace: Namespace::default(),
                    name: "pay".to_owned(),
                },
            ),
        ]));
        let sp = sections
            .find_page("domains/billing/systems/pay/api")
            .unwrap();
        assert_eq!(sp.section.to_string(), "system:default/pay");
        assert_eq!(sp.path, "api");
    }

    #[test]
    fn find_page_no_partial_segment_match() {
        let sections = Sections::new(HashMap::from([(
            "domains/bill".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "bill".to_owned(),
            },
        )]));
        assert!(sections.find_page("domains/billing").is_none());
    }

    /// Section roots holding characters an href would read as delimiters.
    fn reserved_char_sections() -> Sections {
        let system = |name: &str| Section {
            kind: "system".to_owned(),
            namespace: Namespace::default(),
            name: name.to_owned(),
        };
        Sections::with_implicit_root(
            HashMap::from([
                ("billing".to_owned(), system("billing")),
                ("billing/sys#1".to_owned(), system("sys#1")),
                ("q?x".to_owned(), system("q?x")),
                ("100%".to_owned(), system("100%")),
            ]),
            Namespace::default(),
        )
    }

    #[test]
    fn find_page_reads_reserved_chars_as_part_of_the_path() {
        let sections = reserved_char_sections();
        for (path, expected_ref, expected_subpath) in [
            ("billing/c#", "system:default/billing", "c#"),
            ("billing/c#/intro", "system:default/billing", "c#/intro"),
            ("billing/a?b", "system:default/billing", "a?b"),
            ("billing/sys#1", "system:default/sys#1", ""),
            ("billing/sys#1/api", "system:default/sys#1", "api"),
            ("q?x/api", "system:default/q?x", "api"),
            ("100%/api", "system:default/100%", "api"),
        ] {
            let sp = sections.find_page(path).unwrap();
            assert_eq!(
                (sp.section.to_string().as_str(), sp.path),
                (expected_ref, expected_subpath),
                "{path:?}"
            );
        }
    }

    #[test]
    fn anchors_read_reserved_chars_as_part_of_the_path() {
        let sections = reserved_char_sections();
        let subpaths: Vec<(String, String)> = sections
            .anchors("billing/sys#1/c#")
            .into_iter()
            .map(|a| (a.section_ref, a.subpath))
            .collect();
        assert_eq!(
            subpaths,
            [
                ("system:default/sys#1".to_owned(), "c#".to_owned()),
                ("system:default/billing".to_owned(), "sys#1/c#".to_owned()),
                (
                    "section:default/root".to_owned(),
                    "billing/sys#1/c#".to_owned()
                ),
            ]
        );
        assert_eq!(
            sections.ancestry_for(["system:default/q?x"])["system:default/q?x"],
            [
                SectionAnchor {
                    section_ref: "system:default/q?x".to_owned(),
                    subpath: String::new(),
                },
                SectionAnchor {
                    section_ref: "section:default/root".to_owned(),
                    subpath: "q?x".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn resolve_refpath_base_path_reads_reserved_chars_as_part_of_the_path() {
        let sections = reserved_char_sections();
        let (href, sp) = sections
            .resolve_refpath("::overview", Some("/billing/sys#1/api"))
            .expect("should resolve within the current section");
        assert_eq!(href, "/billing/sys#1/overview");
        assert_eq!(sp.section.to_string(), "system:default/sys#1");
    }

    #[test]
    fn resolve_refpath_current_section_root_with_fragment() {
        let sections = billing();
        let (href, sp) = sections
            .resolve_refpath("::#intro", Some("/domains/billing/api"))
            .expect("should resolve to the current section root");
        assert_eq!(href, "/domains/billing#intro");
        assert_eq!(sp.section.to_string(), "domain:default/billing");
        assert_eq!(sp.path, "");
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

    #[test]
    fn find_by_ref_deterministic_on_duplicate_identifier() {
        // Three different section roots collapse to the same kind:namespace/name
        // ref. Three (not two) distinguishes "keep the running minimum" from
        // "keep the last-seen-smaller": the winner must be the global minimum
        // regardless of HashMap iteration order.
        let billing = Section {
            kind: "domain".to_owned(),
            namespace: Namespace::default(),
            name: "billing".to_owned(),
        };
        let sections = Sections::new(HashMap::from([
            ("m/billing".to_owned(), billing.clone()),
            ("z/billing".to_owned(), billing.clone()),
            ("a/billing".to_owned(), billing),
        ]));
        assert_eq!(
            sections.find_by_ref("domain:default/billing"),
            Some("a/billing")
        );
    }

    #[test]
    fn parsed_refpath_parse() {
        let refpath = |kind, namespace, name, subpath, fragment| ParsedRefpath {
            kind,
            namespace,
            name,
            subpath,
            fragment,
        };
        for (input, expected) in [
            (
                "domain:billing::overview#tokens",
                refpath(
                    Some("domain"),
                    None,
                    Some("billing"),
                    Some("overview"),
                    Some("tokens"),
                ),
            ),
            (
                "domain:production/billing::overview",
                refpath(
                    Some("domain"),
                    Some("production"),
                    Some("billing"),
                    Some("overview"),
                    None,
                ),
            ),
            ("billing", refpath(None, None, Some("billing"), None, None)),
            (
                "billing::deep/page",
                refpath(None, None, Some("billing"), Some("deep/page"), None),
            ),
            (
                "::overview",
                refpath(None, None, None, Some("overview"), None),
            ),
            ("::", refpath(None, None, None, None, None)),
            ("#heading", refpath(None, None, None, None, Some("heading"))),
            (
                "domain:billing#intro",
                refpath(Some("domain"), None, Some("billing"), None, Some("intro")),
            ),
        ] {
            assert_eq!(ParsedRefpath::parse(input), expected, "{input}");
        }
    }

    #[test]
    fn parsed_refpath_section() {
        let payments: Namespace = "payments".parse().unwrap();
        for (input, current, expected) in [
            (
                "domain:billing::overview",
                None,
                Some("domain:default/billing"),
            ),
            (
                "domain:billing::overview",
                Some(&payments),
                Some("domain:payments/billing"),
            ),
            (
                "domain:production/billing::overview",
                Some(&payments),
                Some("domain:production/billing"),
            ),
            ("billing", None, Some("section:default/billing")),
            ("::overview", None, None),
            ("domain:/billing", None, None),
            ("domain:bad value/billing", None, None),
        ] {
            let section = ParsedRefpath::parse(input).section(current);
            assert_eq!(
                section.map(|s| s.to_string()).as_deref(),
                expected,
                "{input}"
            );
        }
    }

    #[test]
    fn section_ref_round_trips() {
        for (ref_string, kind, namespace, name) in [
            ("domain:default/billing", "domain", "default", "billing"),
            ("system:production/auth", "system", "production", "auth"),
            ("component:staging/api", "component", "staging", "api"),
        ] {
            let section = Section {
                kind: kind.to_owned(),
                namespace: namespace.parse().unwrap(),
                name: name.to_owned(),
            };
            assert_eq!(ref_string.parse::<Section>().unwrap(), section);
            assert_eq!(section.to_string(), ref_string);
        }
    }

    #[test]
    fn section_from_str_rejects_malformed() {
        // Shape failures (no ':' / no '/') and empty kind or name all map to
        // the generic Format variant.
        let parse = |s: &str| s.parse::<Section>().unwrap_err();
        assert!(matches!(parse(""), ParseSectionError::Format));
        assert!(std::error::Error::source(&parse("")).is_none());
        assert!(matches!(parse("domain"), ParseSectionError::Format)); // no ':'
        assert!(matches!(parse("domain:billing"), ParseSectionError::Format)); // no '/'
        assert!(matches!(
            parse(":default/billing"),
            ParseSectionError::Format
        )); // empty kind
        assert!(matches!(
            parse("domain:default/"),
            ParseSectionError::Format
        )); // empty name

        // An empty namespace fails the charset check, so it is reported as the
        // Namespace variant rather than Format.
        assert!(matches!(
            parse("domain:/billing"),
            ParseSectionError::Namespace(_)
        ));

        // Structurally complete but namespace fails the Backstage charset —
        // must propagate InvalidNamespace as the source rather than the
        // generic "expected kind:namespace/name" message.
        let err = parse("domain:bad value/billing");
        assert!(
            matches!(err, ParseSectionError::Namespace(_)),
            "charset failure should map to the Namespace variant: {err}"
        );
        let source = std::error::Error::source(&err)
            .expect("InvalidNamespace should be carried as the error source");
        assert!(
            source.to_string().contains("bad value"),
            "namespace error should surface the bad value: {source}"
        );
    }

    #[test]
    fn section_root_uses_given_namespace() {
        assert_eq!(
            Section::root("payments".parse().unwrap()).to_string(),
            "section:payments/root"
        );
        assert_eq!(Section::root(Namespace::default()).namespace, "default");
    }

    #[test]
    fn resolve_refpath_omitted_namespace_uses_current_page_namespace() {
        let sections = Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: "payments".parse().unwrap(),
                name: "billing".to_owned(),
            },
        )]));
        // base_path is a page inside the payments-namespace section; the wikilink
        // omits the namespace, so it must resolve within "payments".
        let (href, sp) = sections
            .resolve_refpath("domain:billing::overview", Some("domains/billing/api"))
            .expect("should resolve");
        assert_eq!(href, "/domains/billing/overview");
        assert_eq!(sp.section.namespace, "payments");
    }

    #[test]
    fn resolve_refpath_explicit_namespace_is_honored() {
        let sections = Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: "payments".parse().unwrap(),
                name: "billing".to_owned(),
            },
        )]));
        let (href, _) = sections
            .resolve_refpath("domain:payments/billing::overview", None)
            .expect("explicit namespace resolves");
        assert_eq!(href, "/domains/billing/overview");
        // A mismatched explicit namespace does not resolve.
        assert!(
            sections
                .resolve_refpath("domain:default/billing::overview", None)
                .is_none()
        );
    }

    #[test]
    fn iter_yields_all_entries() {
        let sections = billing();
        let entries: Vec<_> = sections.iter().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            ("domains/billing", sections.get("domains/billing").unwrap())
        );
    }
}
