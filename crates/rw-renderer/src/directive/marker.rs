//! Semantic markers emitted by directives.
//!
//! A [`Marker`] names what a directive *means*; the backend decides what it
//! looks like. This keeps backend-specific markup out of the backend-agnostic
//! directive layer.

/// A semantic marker a directive emits.
///
/// The directive names what it *means* (`name`) and supplies attributes
/// (`attrs`) it has already normalized. Backends dispatch on `name` in
/// [`RenderBackend::marker_open`] / [`RenderBackend::marker_close`] and read
/// attributes with [`attr`](Self::attr).
///
/// Dispatch is a `name ==` comparison rather than a typed enum on purpose: the
/// directive API is pluggable, and a third-party directive cannot add a variant
/// to a core enum.
///
/// [`RenderBackend::marker_open`]: crate::RenderBackend::marker_open
/// [`RenderBackend::marker_close`]: crate::RenderBackend::marker_close
///
/// # Example
///
/// ```
/// use rw_renderer::directive::Marker;
///
/// let marker = Marker::new("status").with_attr("color", "green");
/// assert_eq!(marker.name, "status");
/// assert_eq!(marker.attr("color"), Some("green"));
/// assert_eq!(marker.attr("size"), None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    /// Semantic name, e.g. `"status"`.
    pub name: &'static str,
    /// Attributes the directive normalized. Private so [`with_attr`](Self::with_attr)
    /// stays the only way in, which is what makes [`attr`](Self::attr)'s
    /// first-value-wins rule enforceable.
    attrs: Vec<(&'static str, String)>,
}

impl Marker {
    /// Create a marker with no attributes.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            attrs: Vec::new(),
        }
    }

    /// Add a normalized attribute, builder-style.
    #[must_use]
    pub fn with_attr(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.attrs.push((key, value.into()));
        self
    }

    /// Look up an attribute by key, or `None` if the directive didn't set it.
    /// If a directive set the same key twice, the first value wins.
    ///
    /// Values are plain strings with no type-level guarantee, and the directive
    /// API is public — a backend interpolating one into markup must validate it.
    #[must_use]
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_has_no_attrs() {
        let marker = Marker::new("status");
        assert_eq!(marker.name, "status");
        assert_eq!(marker.attr("color"), None);
    }

    #[test]
    fn test_attr_returns_first_of_duplicate_keys() {
        let marker = Marker::new("status")
            .with_attr("color", "green")
            .with_attr("color", "red");
        assert_eq!(marker.attr("color"), Some("green"));
    }
}
