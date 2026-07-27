//! Characterization test for `Site::render_search_document`'s handling of
//! fenced code blocks.
//!
//! Diagram fences are reduced to text by a pre-pass over the markdown
//! (`rw_parser::rewrite_fences` + `rw_plantuml::search_text`) and then indexed
//! as ordinary code blocks; no diagram provider runs. These tests assert at the
//! level of `rw_site`'s public search API, so they hold whatever mechanism sits
//! underneath — which is what let them survive the switch away from an
//! intercept-during-the-walk processor.

use std::fs;
use std::sync::Arc;

use rw_cache::NullCache;
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

/// Build a `Site` over a real temp directory holding one page with four
/// fenced code blocks: `plantuml`, `c4plantuml`, `mermaid`, and `python`.
/// Returns the `TempDir` too, so it stays alive for the caller's assertions.
fn site_with_diagram_fixture() -> (tempfile::TempDir, Site) {
    let temp_dir = tempfile::tempdir().unwrap();
    let docs = temp_dir.path().join("docs");
    fs::create_dir_all(&docs).unwrap();

    let markdown = "\
# Diagrams Fixture

## PlantUML

```plantuml
@startuml
skinparam defaultFontName Roboto
!include common.puml
Person(user1, \"User One\", \"A user\")
System(sys1, \"System One\", \"The system\")
@enduml
```

## C4-PlantUML

```c4plantuml
@startuml
!include C4_Context.puml
Person(user2, \"User Two\")
System(sys2, \"System Two\")
@enduml
```

## Mermaid

```mermaid
graph TD
    A-->B
    B-->C
```

## Python

```python
def hello():
    pass
```
";

    fs::write(docs.join("diagrams.md"), markdown).unwrap();

    let storage = Arc::new(FsStorage::new(temp_dir.path().to_path_buf(), docs));
    let site = Site::new(storage, Arc::new(NullCache), PageRendererConfig::default());
    (temp_dir, site)
}

#[test]
fn plantuml_fence_strips_boilerplate_but_keeps_entities() {
    let (_temp_dir, site) = site_with_diagram_fixture();

    let doc = site
        .render_search_document("diagrams")
        .unwrap()
        .expect("page has content");

    assert!(
        !doc.text.contains("@startuml"),
        "expected @startuml stripped, got: {}",
        doc.text
    );
    assert!(
        !doc.text.contains("skinparam"),
        "expected skinparam stripped, got: {}",
        doc.text
    );
    assert!(
        !doc.text.contains("!include"),
        "expected !include stripped, got: {}",
        doc.text
    );
    assert!(
        doc.text.contains("Person(user1,"),
        "expected plantuml entity kept, got: {}",
        doc.text
    );
    assert!(
        doc.text.contains("System(sys1,"),
        "expected plantuml entity kept, got: {}",
        doc.text
    );
}

#[test]
fn c4plantuml_fence_strips_boilerplate_but_keeps_entities() {
    let (_temp_dir, site) = site_with_diagram_fixture();

    let doc = site
        .render_search_document("diagrams")
        .unwrap()
        .expect("page has content");

    assert!(
        !doc.text.contains("@startuml"),
        "expected @startuml stripped, got: {}",
        doc.text
    );
    assert!(
        !doc.text.contains("!include C4_Context.puml"),
        "expected !include stripped, got: {}",
        doc.text
    );
    assert!(
        doc.text.contains("Person(user2,"),
        "expected c4plantuml entity kept, got: {}",
        doc.text
    );
    assert!(
        doc.text.contains("System(sys2,"),
        "expected c4plantuml entity kept, got: {}",
        doc.text
    );
}

#[test]
fn mermaid_fence_survives_as_raw_source() {
    let (_temp_dir, site) = site_with_diagram_fixture();

    let doc = site
        .render_search_document("diagrams")
        .unwrap()
        .expect("page has content");

    assert!(
        doc.text.contains("A-->B"),
        "expected raw mermaid source, got: {}",
        doc.text
    );
    assert!(
        doc.text.contains("B-->C"),
        "expected raw mermaid source, got: {}",
        doc.text
    );
}

#[test]
fn python_fence_survives_as_raw_source() {
    let (_temp_dir, site) = site_with_diagram_fixture();

    let doc = site
        .render_search_document("diagrams")
        .unwrap()
        .expect("page has content");

    assert!(
        doc.text.contains("def hello():"),
        "expected raw python source, got: {}",
        doc.text
    );
    assert!(
        doc.text.contains("pass"),
        "expected raw python source, got: {}",
        doc.text
    );
}

/// The whole indexed string, exactly.
///
/// This is the assertion that detects a change: it subsumes every `contains`
/// above, and it is the only one that sees ordering, separation between
/// blocks, and stray or missing whitespace. The `contains` assertions are kept
/// for diagnosis, not detection — when this one fails, the one that failed
/// alongside it names the fence class that was lost.
///
/// Note what it records: every block, diagram or not, ends in the `\n ` that
/// `SearchDocumentBackend::code_block` produces — the fence's own closing line
/// break plus a separating space. Do NOT emit diagram text without it — a
/// diagram then runs straight into the following block
/// (`"The system")C4-PlantUML`) and merges two words in the index.
///
/// The H1 is absent because title extraction consumes it into
/// `SearchDocument::title`.
#[test]
fn the_indexed_text_is_pinned_verbatim() {
    let (_temp_dir, site) = site_with_diagram_fixture();

    let doc = site
        .render_search_document("diagrams")
        .unwrap()
        .expect("page has content");

    assert_eq!(
        doc.text,
        concat!(
            "PlantUML Person(user1, \"User One\", \"A user\")\n",
            "System(sys1, \"System One\", \"The system\")\n ",
            "C4-PlantUML Person(user2, \"User Two\")\n",
            "System(sys2, \"System Two\")\n ",
            "Mermaid graph TD\n    A-->B\n    B-->C\n ",
            "Python def hello():\n    pass\n ",
        )
    );
}
