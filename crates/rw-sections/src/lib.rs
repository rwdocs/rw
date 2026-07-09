//! Section reference types and path-to-section resolution.
//!
//! A **section** is a named subtree of a documentation site. Each section has a
//! freeform [`kind`](Section::kind) (e.g., `"domain"`, `"system"`,
//! `"component"`) and a [`name`](Section::name) derived from the last segment
//! of its section root (e.g., `"billing"` for path `domains/billing`). Sections
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
//! let sp = sections.find("/domains/billing/api").unwrap();
//! assert_eq!(sp.section.to_string(), "domain:default/billing");
//! assert_eq!(sp.path, "api");
//! ```
//!
//! # Feature flags
//!
//! - **`serde`** — derives `serde::Serialize` and `serde::Deserialize` on
//!   [`Section`], and a hand-written `serde::Serialize` on [`Sections`] that
//!   emits its bare section-root → section map (the derived reverse index is
//!   never serialized), for JSON API responses and cache storage.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;
use std::iter::from_fn;
use std::str::FromStr;

mod namespace;
pub use namespace::{InvalidNamespace, Namespace};

/// The site-relative path at which a section is rooted — the chain of directory
/// segments from the site root down to the section, with no leading slash
/// (e.g. `"domains/billing"`; `""` is the root section). It is what
/// [`Sections`] keys sections by. A plain `String`; the alias just names the
/// role wherever the type would otherwise read as a bare `String`.
pub type SectionRoot = String;

/// A named documentation section with a kind and name.
///
/// Represents one node in the section hierarchy. The [`kind`](Self::kind) is a
/// freeform label — any string is valid, though typical values include
/// `"domain"`, `"system"`, and `"component"`. The [`name`](Self::name) is
/// currently derived from the last segment of its section root
/// (e.g., `"billing"` for section root `domains/billing`).
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Section {
    /// Freeform label classifying this section (e.g., `"domain"`, `"system"`).
    pub kind: String,
    /// Section namespace (e.g., `"default"`, `"payments"`). Inherited down the
    /// page tree from `namespace` metadata; `#[serde(default)]` so older
    /// cached `Section` JSON written before this field existed (just
    /// `{kind, name}`) still deserializes — the missing namespace becomes
    /// [`Namespace::default()`] (`"default"`), matching historical behavior.
    #[cfg_attr(feature = "serde", serde(default))]
    pub namespace: Namespace,
    /// Section name, currently the last segment of the section root (e.g., `"billing"`).
    pub name: String,
}

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
            kind: "section".to_owned(),
            namespace,
            name: Self::ROOT_NAME.to_owned(),
        }
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
/// carried as the cause (accessible via [`std::error::Error::source`] and
/// surfaced in the [`Display`](fmt::Display) output).
///
/// # Examples
///
/// ```
/// use rw_sections::{Namespace, Section};
///
/// let err = "invalid".parse::<Section>().unwrap_err();
/// assert_eq!(err.to_string(), "invalid section ref: expected \"kind:namespace/name\"");
///
/// // Namespace charset violations propagate the specific reason.
/// let err = "domain:bad value/billing".parse::<Section>().unwrap_err();
/// assert!(err.to_string().contains("bad value"));
/// ```
#[derive(Debug, Default, thiserror::Error)]
pub enum ParseSectionError {
    /// The ref string did not match the `kind:namespace/name` shape, or its
    /// kind or name segment was empty. An empty *namespace* segment is reported
    /// as [`Namespace`](Self::Namespace) instead, since it fails the namespace
    /// charset check.
    #[default]
    #[error("invalid section ref: expected \"kind:namespace/name\"")]
    Format,

    /// The namespace segment was present but failed `validate_namespace` (this
    /// includes an empty namespace); the underlying [`InvalidNamespace`] is
    /// carried as the cause (accessible via [`std::error::Error::source`]).
    #[error("invalid section ref: {0}")]
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
        let (kind, rest) = s.split_once(':').ok_or(ParseSectionError::default())?;
        let (namespace, name) = rest.split_once('/').ok_or(ParseSectionError::default())?;
        if kind.is_empty() || name.is_empty() {
            return Err(ParseSectionError::default());
        }
        let namespace: Namespace = namespace.parse()?;
        Ok(Self {
            kind: kind.to_owned(),
            namespace,
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

/// One section yielded by the internal enclosing walk, with the target's path
/// relative to it. Private: the walk backs [`Sections::find`],
/// [`Sections::ancestors`], and [`Sections::parent`], which each expose only the
/// piece their callers need.
#[derive(Debug)]
struct Enclosing<'s, 'h> {
    /// The enclosing section.
    section: &'s Section,
    /// The enclosing section's own root (no leading slash; `""` for the root).
    section_root: &'s str,
    /// The target path relative to this section (empty for an exact match; the
    /// full path when only the root encloses it). A query string stays part of
    /// it; a `#fragment` is split off before the walk.
    path: &'h str,
}

/// Section-root → [`Section`] index with segment-aware prefix lookup and an O(1)
/// reverse (ref → root) index.
///
/// Section roots are stored without leading slashes (e.g., `"domains/billing"`).
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
/// let sp = sections.find("/domains/billing/systems/pay/api").unwrap();
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
    by_path: HashMap<SectionRoot, Section>,
    /// Canonical section → section root, derived from `by_path` at construction.
    /// Makes [`find_by_ref`](Self::find_by_ref) O(1). Never serialized; rebuilt
    /// whenever a `Sections` is constructed (including cache reload).
    by_ref: HashMap<Section, SectionRoot>,
}

/// Builds the `section → section root` reverse index from `by_path`.
///
/// On a duplicate identifier the lexicographically-first section root wins, so the
/// index is deterministic regardless of `HashMap` iteration order. A collision
/// is logged, not fatal — section identifiers are assumed unique but not
/// validated at load.
fn build_ref_index(by_path: &HashMap<SectionRoot, Section>) -> HashMap<Section, SectionRoot> {
    let mut by_ref: HashMap<Section, SectionRoot> = HashMap::with_capacity(by_path.len());
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

/// Splits a `#fragment` off the end of an href tail (leading slash already
/// stripped). A trailing `#` with nothing after it yields `Some("")`, not
/// `None` — an empty fragment is still a fragment.
fn split_fragment(input: &str) -> (&str, Option<&str>) {
    match input.find('#') {
        Some(pos) => (&input[..pos], Some(&input[pos + 1..])),
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

#[cfg(feature = "serde")]
impl serde::Serialize for Sections {
    /// Serializes as the bare `section-root → section` map — byte-identical to the
    /// previous `#[serde(transparent)]` shape. The derived `by_ref` index is not
    /// serialized (it is rebuilt on deserialize via [`with_implicit_root`]).
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&self.by_path, serializer)
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
    pub fn new(map: HashMap<SectionRoot, Section>) -> Self {
        let by_ref = build_ref_index(&map);
        Self {
            by_path: map,
            by_ref,
        }
    }

    /// Creates a [`Sections`] that always resolves.
    ///
    /// If `map` has no entry at the empty section root, inserts the implicit
    /// root section (`section:<root_namespace>/root`) so [`find`](Self::find)
    /// returns a match for *every* path — a page outside any explicit section
    /// resolves to the root rather than `None`. An explicit root already in
    /// `map` is kept.
    ///
    /// This is the constructor rw uses for the live site: both the section-ref
    /// API and the renderer's link annotation rely on `find` being total.
    /// [`new`](Self::new) builds a map without that guarantee, for tests and
    /// any caller that wants a rootless map.
    #[must_use]
    pub fn with_implicit_root(
        mut map: HashMap<SectionRoot, Section>,
        root_namespace: Namespace,
    ) -> Self {
        map.entry(String::new())
            .or_insert_with(|| Section::root(root_namespace));
        let by_ref = build_ref_index(&map);
        Self {
            by_path: map,
            by_ref,
        }
    }

    /// Returns `true` if no sections are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }

    /// Returns the section at the given section root, or `None`.
    ///
    /// The `path` must match a key exactly (no prefix matching). Use
    /// [`find`](Self::find) for prefix-based lookup from an href.
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
    /// [`find`](Self::find) (deepest prefix). Reach for this only when you
    /// need to visit every entry, e.g. building a secondary index.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Section)> {
        self.by_path
            .iter()
            .map(|(path, section)| (path.as_str(), section))
    }

    /// Returns an iterator over the section roots (map keys), in arbitrary order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.by_path.keys().map(String::as_str)
    }

    /// Finds the deepest section whose section root is a prefix of `href`.
    ///
    /// Returns a [`SectionPath`] with the matching section, the path within
    /// that section, and an optional fragment. Returns `None` if no section
    /// matches.
    ///
    /// The `href` may have a leading slash (it is stripped before matching)
    /// and an optional `#fragment` (it is extracted into
    /// [`SectionPath::fragment`]). Matching is segment-aware: section root
    /// `"domains/bill"` does **not** match href `"/domains/billing"`.
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
        let (path, fragment) = split_fragment(without_slash);
        let innermost = self.enclosing_iter(path).next()?;
        Some(SectionPath {
            section: innermost.section,
            path: innermost.path,
            fragment,
        })
    }

    /// Iterates the sections enclosing `path` (slash- and fragment-stripped),
    /// innermost first and the root (`""`) last. O(D): walks `path`'s
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
    /// `base_path` is used to determine the current section for targets that
    /// start with `::` (current-section links).
    ///
    /// Returns the constructed href and a [`SectionPath`] borrowing from
    /// `self` (section) and `input` (subpath, fragment).
    ///
    /// Returns `None` if the target is fragment-only (`#heading`), cannot be
    /// resolved (unknown section), or `base_path` is missing for
    /// current-section links.
    #[must_use]
    pub fn resolve_refpath<'h>(
        &self,
        input: &'h str,
        base_path: Option<&str>,
    ) -> Option<(String, SectionPath<'_, 'h>)> {
        let target = ParsedRefpath::parse(input);

        if target.fragment_only().is_some() {
            return None;
        }

        let current_namespace = base_path
            .and_then(|bp| self.find(bp))
            .map_or("default", |sp| sp.section.namespace.as_ref());

        let section_ref_string = if let Some(ref_str) = target.to_section_ref(current_namespace) {
            ref_str
        } else {
            let sp = self.find(base_path?)?;
            sp.section.to_string()
        };

        let section_root = self.find_by_ref(&section_ref_string)?;
        let section = self.by_path.get(section_root)?;

        let mut href = if section_root.is_empty() {
            String::from("/")
        } else {
            format!("/{section_root}")
        };

        if let Some(subpath) = target.subpath {
            if href.ends_with('/') {
                href.push_str(subpath);
            } else {
                href.push('/');
                href.push_str(subpath);
            }
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
                fragment: target.fragment,
            },
        ))
    }
}

/// Default section kind used when a refpath omits the kind prefix.
const ROOT_SECTION_KIND: &str = "section";

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
/// - `#heading` → same page, fragment="heading"
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedRefpath<'a> {
    /// Section kind (e.g., `"domain"`). `None` defaults to `"section"`.
    kind: Option<&'a str>,
    /// Section namespace. `None` defaults to `"default"`.
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
        let (input, fragment) = match input.find('#') {
            Some(pos) => {
                let frag = &input[pos + 1..];
                (
                    &input[..pos],
                    if frag.is_empty() { None } else { Some(frag) },
                )
            }
            None => (input, None),
        };

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

    /// Returns the fragment if this target refers only to a heading on the current page
    /// (no section, no subpath — just `#fragment`).
    #[must_use]
    pub fn fragment_only(&self) -> Option<&'a str> {
        if self.name.is_none() && self.subpath.is_none() {
            self.fragment
        } else {
            None
        }
    }

    /// Build a full ref string (e.g., `"domain:default/billing"`).
    ///
    /// Applies defaults: kind defaults to `"section"`; namespace defaults to
    /// `default_namespace` when the refpath omits one.
    ///
    /// Returns `None` if `name` is `None` (current-section target — caller
    /// must resolve the current section externally).
    #[must_use]
    pub fn to_section_ref(&self, default_namespace: &str) -> Option<String> {
        let name = self.name?;
        let kind = self.kind.unwrap_or(ROOT_SECTION_KIND);
        let namespace = self.namespace.unwrap_or(default_namespace);
        Some(format!("{kind}:{namespace}/{name}"))
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
    fn find_handles_degenerate_hrefs_without_panic() {
        let sections = nested_sections();
        // None of these match an explicit section, so they resolve to the
        // implicit root — and the byte-index slicing must not panic on empty,
        // slash-only, doubled-slash, or trailing-slash inputs.
        for href in ["", "/", "#frag", "/#frag", "//", "a//b", "a/"] {
            assert_eq!(
                sections.find(href).unwrap().section.to_string(),
                "section:default/root",
                "href {href:?} should resolve to the root section"
            );
        }
        // The fragment is still extracted for the root-with-fragment form.
        assert_eq!(sections.find("/#frag").unwrap().fragment, Some("frag"));
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
    fn find_deepest_wins() {
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
                namespace: Namespace::default(),
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
            namespace: Namespace::default(),
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
    fn parse_error_default_is_format_variant() {
        let err = ParseSectionError::default();
        assert!(matches!(err, ParseSectionError::Format));
        assert_eq!(
            err.to_string(),
            "invalid section ref: expected \"kind:namespace/name\""
        );
        // The shape failure has no underlying cause.
        assert!(std::error::Error::source(&err).is_none());
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

    #[cfg(feature = "serde")]
    #[test]
    fn serializes_as_bare_section_root_map() {
        let sections = Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )]));
        let json = serde_json::to_value(&sections).unwrap();
        // Bare map keyed by section root; the derived reverse index must not leak.
        assert_eq!(json["domains/billing"]["kind"], "domain");
        assert_eq!(json["domains/billing"]["namespace"], "default");
        assert_eq!(json["domains/billing"]["name"], "billing");
        assert!(json.get("by_ref").is_none());
        assert!(json.get("by_path").is_none());
    }

    #[test]
    fn section_target_full() {
        let t = ParsedRefpath::parse("domain:billing::overview#tokens");
        assert_eq!(t.kind, Some("domain"));
        assert_eq!(t.namespace, None);
        assert_eq!(t.name, Some("billing"));
        assert_eq!(t.subpath, Some("overview"));
        assert_eq!(t.fragment, Some("tokens"));
    }

    #[test]
    fn section_target_with_namespace() {
        let t = ParsedRefpath::parse("domain:production/billing::overview");
        assert_eq!(t.kind, Some("domain"));
        assert_eq!(t.namespace, Some("production"));
        assert_eq!(t.name, Some("billing"));
        assert_eq!(t.subpath, Some("overview"));
        assert_eq!(t.fragment, None);
    }

    #[test]
    fn section_target_name_only() {
        let t = ParsedRefpath::parse("billing");
        assert_eq!(t.kind, None);
        assert_eq!(t.namespace, None);
        assert_eq!(t.name, Some("billing"));
        assert_eq!(t.subpath, None);
        assert_eq!(t.fragment, None);
    }

    #[test]
    fn section_target_name_with_subpath() {
        let t = ParsedRefpath::parse("billing::deep/page");
        assert_eq!(t.kind, None);
        assert_eq!(t.name, Some("billing"));
        assert_eq!(t.subpath, Some("deep/page"));
    }

    #[test]
    fn section_target_current_section() {
        let t = ParsedRefpath::parse("::overview");
        assert_eq!(t.kind, None);
        assert_eq!(t.name, None);
        assert_eq!(t.subpath, Some("overview"));
    }

    #[test]
    fn section_target_current_section_root() {
        let t = ParsedRefpath::parse("::");
        assert_eq!(t.name, None);
        assert_eq!(t.subpath, None);
    }

    #[test]
    fn section_target_fragment_only() {
        let t = ParsedRefpath::parse("#heading");
        assert_eq!(t.kind, None);
        assert_eq!(t.name, None);
        assert_eq!(t.subpath, None);
        assert_eq!(t.fragment, Some("heading"));
    }

    #[test]
    fn section_target_section_root_with_fragment() {
        let t = ParsedRefpath::parse("domain:billing#intro");
        assert_eq!(t.kind, Some("domain"));
        assert_eq!(t.name, Some("billing"));
        assert_eq!(t.subpath, None);
        assert_eq!(t.fragment, Some("intro"));
    }

    #[test]
    fn section_target_deep_subpath() {
        let t = ParsedRefpath::parse("domain:billing::api/auth/v2");
        assert_eq!(t.subpath, Some("api/auth/v2"));
    }

    #[test]
    fn section_target_to_ref_full() {
        let t = ParsedRefpath::parse("domain:billing::overview");
        assert_eq!(
            t.to_section_ref("default").unwrap(),
            "domain:default/billing"
        );
    }

    #[test]
    fn section_target_to_ref_with_namespace() {
        let t = ParsedRefpath::parse("domain:production/billing::overview");
        assert_eq!(
            t.to_section_ref("default").unwrap(),
            "domain:production/billing"
        );
    }

    #[test]
    fn section_target_to_ref_name_only() {
        let t = ParsedRefpath::parse("billing");
        assert_eq!(
            t.to_section_ref("default").unwrap(),
            "section:default/billing"
        );
    }

    #[test]
    fn section_target_to_ref_current_section() {
        let t = ParsedRefpath::parse("::overview");
        assert!(t.to_section_ref("default").is_none());
    }

    #[test]
    fn section_display_custom_namespace() {
        let section = Section {
            kind: "domain".to_owned(),
            namespace: "payments".parse().unwrap(),
            name: "billing".to_owned(),
        };
        assert_eq!(section.to_string(), "domain:payments/billing");
    }

    #[test]
    fn section_from_str_custom_namespace() {
        let section: Section = "system:production/auth".parse().unwrap();
        assert_eq!(section.kind, "system");
        assert_eq!(section.namespace, "production");
        assert_eq!(section.name, "auth");
    }

    #[test]
    fn section_from_str_rejects_malformed() {
        // Shape failures (no ':' / no '/') and empty kind or name all map to
        // the generic Format variant.
        let parse = |s: &str| s.parse::<Section>().unwrap_err();
        assert!(matches!(parse(""), ParseSectionError::Format));
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
        assert!(
            err.to_string().contains("bad value"),
            "namespace error should surface the bad value: {err}"
        );
        assert!(
            std::error::Error::source(&err).is_some(),
            "InvalidNamespace should be carried as the error source"
        );
    }

    #[test]
    fn section_roundtrip_custom_namespace() {
        let section = Section {
            kind: "component".to_owned(),
            namespace: "staging".parse().unwrap(),
            name: "api".to_owned(),
        };
        let parsed: Section = section.to_string().parse().unwrap();
        assert_eq!(parsed, section);
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
    fn to_section_ref_uses_default_namespace_argument() {
        let t = ParsedRefpath::parse("domain:billing::overview");
        assert_eq!(
            t.to_section_ref("payments").unwrap(),
            "domain:payments/billing"
        );
        assert_eq!(
            t.to_section_ref("default").unwrap(),
            "domain:default/billing"
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
