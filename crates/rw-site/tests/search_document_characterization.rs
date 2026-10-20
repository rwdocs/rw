//! Characterization test for `Site::render_search_document`'s handling of
//! fenced code blocks.
//!
//! Diagram fences are reduced to text by a pre-pass over the markdown
//! (`rw_parser::rewrite_fences` + `rw_plantuml::search_text`) and then indexed
//! as ordinary code blocks; no diagram provider runs. The test asserts at the
//! level of `rw_site`'s public search API, so it holds whatever mechanism sits
//! underneath.

use std::fs;
use std::sync::Arc;

use rw_cache::NullCache;
use rw_site::{PageRendererConfig, Site};
use rw_storage_fs::FsStorage;

/// The whole indexed string, exactly, so ordering, separation between blocks,
/// and stray or missing whitespace all show.
///
/// Note what it records: every block, diagram or not, ends in the `\n ` that
/// `SearchDocumentBackend::code_block` produces — the fence's own closing line
/// break plus a separating space. Do NOT emit diagram text without it — a
/// diagram then runs straight into the following block
/// (`"The system")C4-PlantUML`) and merges two words in the index.
///
/// The H1's words open the text: the title comes from the site snapshot, not
/// from extracting it out of the indexed body, so nothing removes them.
#[test]
fn the_indexed_text_is_pinned_verbatim() {
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

    let doc = site
        .render_search_document("diagrams")
        .unwrap()
        .expect("page has content");

    assert_eq!(
        doc.text,
        concat!(
            "Diagrams Fixture ",
            "PlantUML Person(user1, \"User One\", \"A user\")\n",
            "System(sys1, \"System One\", \"The system\")\n ",
            "C4-PlantUML Person(user2, \"User Two\")\n",
            "System(sys2, \"System Two\")\n ",
            "Mermaid graph TD\n    A-->B\n    B-->C\n ",
            "Python def hello():\n    pass\n ",
        )
    );
}
