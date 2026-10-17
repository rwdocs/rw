//! The content digest a render is cached and published under.

use sha2::{Digest, Sha256};

use crate::language::{DiagramFormat, DiagramLanguage};

/// The cache and attachment key for one render: hex SHA-256 of
/// `"{endpoint}:{format}:{dpi}:{source}"`.
///
/// `source` is the prepared source — after `PlantUML` include resolution
/// and config injection — because that is what determines the bytes.
///
/// The DPI is [`DiagramLanguage::render_dpi`], so it adds nothing the endpoint
/// does not already decide; it stays in the hashed string because Confluence
/// attachments are named from this digest, and dropping it would rename every
/// one of them. Do NOT key on a user-configurable DPI either: a `PlantUML`
/// source already carries its injected `skinparam dpi`, and for every other
/// language DPI changes nothing about the bytes.
pub(crate) fn diagram_digest(
    source: &str,
    language: DiagramLanguage,
    format: DiagramFormat,
) -> String {
    let content = format!(
        "{}:{format}:{}:{source}",
        language.kroki_endpoint(),
        language.render_dpi(),
    );
    hex::encode(Sha256::digest(content.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confluence attachments are named `diagram_<hash12>.png` from this
    /// digest, so it is part of rw's published output: changing it renames
    /// every attachment on the next publish, and Confluence keeps the old ones
    /// too.
    #[test]
    fn png_attachment_hash_is_stable_for_a_known_source() {
        let digest = diagram_digest(
            "@startuml\nAlice -> Bob\n@enduml",
            DiagramLanguage::PlantUml,
            DiagramFormat::Png,
        );
        assert_eq!(&digest[..12], "07399b44cd31");
    }

    /// A caller must not be able to collapse the SVG and PNG renders of one
    /// diagram onto a single cache entry / attachment.
    #[test]
    fn digest_differs_by_format() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        assert_ne!(
            diagram_digest(source, DiagramLanguage::PlantUml, DiagramFormat::Svg),
            diagram_digest(source, DiagramLanguage::PlantUml, DiagramFormat::Png),
        );
    }

    /// Mermaid and `GraphViz` render at the same DPI, so this isolates the
    /// endpoint alone.
    #[test]
    fn digest_differs_by_endpoint_at_the_same_dpi() {
        let source = "A -> B";
        assert_ne!(
            diagram_digest(source, DiagramLanguage::Mermaid, DiagramFormat::Png),
            diagram_digest(source, DiagramLanguage::GraphViz, DiagramFormat::Png),
        );
    }
}
