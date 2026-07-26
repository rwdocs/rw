//! Reducing `PlantUML` source to human-readable text.

/// Lines starting with these prefixes are stripped from `PlantUML` source
/// during search document generation.
const PLANTUML_BOILERPLATE_PREFIXES: &[&str] = &[
    "@start",
    "@end",
    "skinparam",
    "!include",
    "!define",
    "!$",
    "hide ",
    "show ",
];

/// Check whether a line is `PlantUML` boilerplate.
fn is_plantuml_boilerplate(line: &str) -> bool {
    let trimmed = line.trim();
    PLANTUML_BOILERPLATE_PREFIXES
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

/// Strip `PlantUML` boilerplate lines, keeping human-readable content.
///
/// Blank lines go too, so the result is a dense block of the names,
/// descriptions, and relationships a search index can use.
#[must_use]
pub fn strip_plantuml_boilerplate(source: &str) -> String {
    source
        .lines()
        .filter(|line| !is_plantuml_boilerplate(line))
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::strip_plantuml_boilerplate;

    #[test]
    fn keeps_only_the_human_readable_lines() {
        let source = "\
@startuml
skinparam dpi 192
!include common.puml
Person(user, \"User\", \"A user\")
System(sys, \"System\", \"The system\")
@enduml";

        assert_eq!(
            strip_plantuml_boilerplate(source),
            "Person(user, \"User\", \"A user\")\nSystem(sys, \"System\", \"The system\")",
        );
    }

    #[test]
    fn strips_indented_boilerplate() {
        let source = "@startuml\n    skinparam dpi 192\n  !include x.iuml\nRel(a, b)\n@enduml";
        assert_eq!(strip_plantuml_boilerplate(source), "Rel(a, b)");
    }

    #[test]
    fn drops_blank_and_whitespace_only_lines() {
        assert_eq!(
            strip_plantuml_boilerplate("A -> B\n\n   \nB -> C"),
            "A -> B\nB -> C"
        );
    }

    #[test]
    fn leaves_a_source_with_no_boilerplate_alone() {
        assert_eq!(
            strip_plantuml_boilerplate("A -> B\nB -> C"),
            "A -> B\nB -> C"
        );
    }
}
