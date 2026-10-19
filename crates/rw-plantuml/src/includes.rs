//! `PlantUML` `!include` resolution and render-config injection.
//!
//! Resolves `!include` directives against the configured include directories
//! and the site model. On top of that, [`prepare_diagram_source`] injects the
//! DPI and font settings a render needs, while [`bundle_source`]
//! only resolves. Stdlib (`!include <tupadr3/common>`) and URL includes are
//! left alone.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::{Captures, Regex};
use rw_diagrams::{FileStamp, SiteModel};

use crate::is_plantuml_fence;
use crate::meta_includes::{is_meta_include_pattern, resolve_meta_include};

/// How many files deep includes may nest. Cycles end sooner, so this only
/// keeps a long chain of distinct files from overflowing the stack, which on
/// a 2 MiB tokio worker aborts `rw serve`.
const MAX_INCLUDE_DEPTH: usize = 256;

/// How many files one diagram may inline. `!include_many` and `!includesub`
/// repeat freely, so a short chain that includes each next file twice
/// would otherwise grow exponentially.
const MAX_INCLUDES: usize = 1_000;

static INCLUDE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^([ \t]*)!(include(?:_many|_once)?|includesub)\s+(.+)$").unwrap()
});

/// What an include takes from a file.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Part {
    /// The diagram a selector (`file!1`, `file!ID`) picks, or the first one;
    /// the whole file when it holds no diagram.
    Diagram(Option<String>),
    /// `!includesub file!NAME`.
    Sub(String),
}

impl Part {
    /// The file and part `!directive target` names; `None` for an
    /// `!includesub` without a file, of a sub the diagram defines.
    fn parse<'t>(directive: &str, target: &'t str) -> Option<(&'t str, Self)> {
        if directive == "includesub" {
            let (path, name) = target.split_once('!')?;
            return Some((path, Self::Sub(name.to_owned())));
        }
        Some(match target.rsplit_once('!') {
            Some((path, selector)) => (path, Self::Diagram(Some(selector.to_owned()))),
            None => (target, Self::Diagram(None)),
        })
    }

    /// The lines this part takes from `content`, or `None` if it is not there.
    fn lines<'c>(&self, content: &'c str) -> Option<Vec<&'c str>> {
        match self {
            Self::Diagram(selector) => diagram_lines(content, selector.as_deref()),
            Self::Sub(name) => sub_lines(content, name),
        }
    }
}

/// Whether `line` is an `@word…` or `\word…` directive.
fn is_directive(line: &str, word: &str) -> bool {
    line.trim_start()
        .strip_prefix(['@', '\\'])
        .is_some_and(|rest| rest.starts_with(word))
}

/// As `PlantUML`'s `DiagramExtractor`: the body of the `@start…` line the
/// selector picks, up to the next `@end…`.
fn diagram_lines<'c>(content: &'c str, selector: Option<&str>) -> Option<Vec<&'c str>> {
    let is_start = |line: &&str| is_directive(line, "start");
    let mut lines = content.lines();
    if !lines.clone().any(|line| is_start(&line)) {
        return Some(lines.collect());
    }
    let mut starts = lines.by_ref().filter(is_start);
    match selector {
        None => starts.next(),
        Some(index) if index.bytes().all(|b| b.is_ascii_digit()) => starts.nth(index.parse().ok()?),
        Some(id) => starts.find(|line| has_id(line, id)),
    }?;
    Some(
        lines
            .take_while(|line| !is_directive(line, "end"))
            .collect(),
    )
}

/// `PlantUML` wants a non-word character after the id, as in `@startuml(id=ID)`.
fn has_id(line: &str, id: &str) -> bool {
    let tag = format!("id={id}");
    line.match_indices(&tag).any(|(at, _)| {
        line[at + tag.len()..].starts_with(|c: char| !c.is_alphanumeric() && c != '_')
    })
}

/// As `PlantUML`'s `Sub.fromFile`: every `!startsub name` block, joined. Any
/// `!endsub` ends a block, even one closing a nested sub.
fn sub_lines<'c>(content: &'c str, name: &str) -> Option<Vec<&'c str>> {
    let mut found = false;
    let mut inside = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(sub) = trimmed.strip_prefix("!startsub") {
            if sub.trim() == name {
                found = true;
                inside = true;
            }
        } else if trimmed.starts_with("!endsub") {
            inside = false;
        } else if inside {
            lines.push(line);
        }
    }
    found.then_some(lines)
}

/// Indent content with the given whitespace prefix, preserving empty lines.
fn indent_content(content: &str, indent: &str) -> String {
    if indent.is_empty() {
        return content.to_owned();
    }
    content
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Result of preparing diagram source with potential warnings.
#[derive(Debug)]
pub struct PrepareResult {
    /// Prepared diagram source.
    pub source: String,
    /// Warnings generated during preparation (e.g., unresolved includes).
    pub warnings: Vec<String>,
    /// Every include candidate probed on disk, including misses, so a caller
    /// can tell when an edit or a new file would change [`source`](Self::source).
    pub reads: Vec<FileStamp>,
}

/// Resolve `PlantUML` `!include` directives in diagram source.
///
/// When `meta_source` is given, meta includes
/// (`systems/sys_payment_gateway.iuml`) are looked up there first; a miss
/// falls back to the filesystem like any other path. Files are searched in
/// `include_dirs` order; an include inside an included file is first searched
/// next to that file, as `PlantUML` does. Only files under an include directory
/// are read, symlinks resolved: an absolute path, or one that leads elsewhere,
/// is refused.
///
/// As in `PlantUML`, a file holding diagrams contributes the body of one: the
/// first, or the one a selector (`file!1`, `file!ID`) picks. `!includesub
/// file!NAME` takes the file's `!startsub NAME` blocks.
///
/// As with `PlantUML`'s `!include`, a file already included anywhere in the
/// source expands to nothing the second time, which also ends cycles.
/// `!include_once` does too, with the warning that stands in for `PlantUML`'s
/// error. `!include_many` and `!includesub` expand every time; a cycle
/// through them is left in place with a warning.
///
/// An include that does not resolve is left in place and a message is pushed
/// onto `warnings`; so is one past [`MAX_INCLUDE_DEPTH`] or [`MAX_INCLUDES`],
/// warned about once. Left in place silently: stdlib and URL includes, which
/// `PlantUML` fetches itself, an `!includesub` of a sub the diagram defines,
/// and, when `meta_source` is `None`, meta-pattern paths, for a later pass
/// that has a model to expand them.
///
/// Each filesystem candidate probed is pushed onto `reads`.
#[must_use]
pub(crate) fn resolve_includes(
    source: &str,
    include_dirs: &[PathBuf],
    meta_source: Option<&dyn SiteModel>,
    warnings: &mut Vec<String>,
    reads: &mut Vec<FileStamp>,
) -> String {
    Resolver {
        include_dirs: include_dirs
            .iter()
            .map(|dir| (dir.as_path(), dir.canonicalize().ok()))
            .collect(),
        meta_source,
        included: HashSet::new(),
        expanding: Vec::new(),
        inlined: 0,
        warned_limit: false,
        warnings,
        reads,
    }
    .resolve(source, None)
}

struct Resolver<'a> {
    /// Each include directory as configured, and canonical when it exists.
    include_dirs: Vec<(&'a Path, Option<PathBuf>)>,
    meta_source: Option<&'a dyn SiteModel>,
    /// Canonical, so two spellings of one file are one file.
    included: HashSet<(PathBuf, Part)>,
    /// The includes being expanded, outermost first.
    expanding: Vec<(PathBuf, Part)>,
    /// How many files have been inlined.
    inlined: usize,
    warned_limit: bool,
    warnings: &'a mut Vec<String>,
    reads: &'a mut Vec<FileStamp>,
}

impl Resolver<'_> {
    /// `here` is the directory of the file `source` was read from.
    fn resolve(&mut self, source: &str, here: Option<&Path>) -> String {
        INCLUDE_PATTERN
            .replace_all(source, |caps: &Captures| {
                match self.include(&caps[2], caps[3].trim(), here) {
                    Some(content) => indent_content(&content, &caps[1]),
                    None => caps[0].to_owned(),
                }
            })
            .into_owned()
    }

    /// Whether another file would pass [`MAX_INCLUDE_DEPTH`] or
    /// [`MAX_INCLUDES`]; the first time, also warns.
    fn at_limit(&mut self, include_path: &str) -> bool {
        let limit = if self.expanding.len() == MAX_INCLUDE_DEPTH {
            format!("Include nesting deeper than {MAX_INCLUDE_DEPTH}")
        } else if self.inlined == MAX_INCLUDES {
            format!("More than {MAX_INCLUDES} includes")
        } else {
            return false;
        };
        if !self.warned_limit {
            self.warned_limit = true;
            self.warnings.push(format!("{limit}: '{include_path}'"));
        }
        true
    }

    /// What `!directive target` expands to; `None` leaves the line as is.
    fn include(&mut self, directive: &str, target: &str, here: Option<&Path>) -> Option<String> {
        let stdlib = target.starts_with('<') && target.ends_with('>');
        if stdlib || target.starts_with("http://") || target.starts_with("https://") {
            return None;
        }

        let (include_path, part) = Part::parse(directive, target)?;
        if part == Part::Diagram(None)
            && let Some(meta) = self.meta_source
            && let Some(content) = resolve_meta_include(include_path, meta)
        {
            return Some(content);
        }
        let once = matches!(directive, "include" | "include_once");

        if Path::new(include_path).has_root() {
            self.warnings.push(format!(
                "Include path must be relative to the include directories: '{include_path}'"
            ));
            return None;
        }

        if self.at_limit(include_path) {
            return None;
        }

        let candidates: Vec<PathBuf> = here
            .into_iter()
            .chain(
                self.include_dirs
                    .iter()
                    .filter(|(_, canonical)| {
                        here.is_none_or(|here| canonical.as_deref() != Some(here))
                    })
                    .map(|(dir, _)| *dir),
            )
            .map(|dir| dir.join(include_path))
            .collect();

        for candidate in &candidates {
            // Stamped before the read, so an edit racing it leaves a stale
            // stamp (a spurious re-render) rather than stale content.
            self.reads.push(FileStamp::of(candidate.clone()));
            let Ok(file) = candidate.canonicalize() else {
                continue;
            };
            let confined = self
                .include_dirs
                .iter()
                .filter_map(|(_, canonical)| canonical.as_deref())
                .any(|root| file.starts_with(root));
            if !confined {
                self.warnings.push(format!(
                    "Include file is outside the include directories: '{include_path}'"
                ));
                return None;
            }
            let key = (file, part.clone());
            if once && self.included.contains(&key) {
                if directive == "include_once" {
                    self.warnings
                        .push(format!("Include file already included: '{target}'"));
                }
                return Some(String::new());
            }
            if self.expanding.contains(&key) {
                self.warnings.push(format!("Include cycle: '{target}'"));
                return None;
            }
            // Read through the canonical path, the one confinement checked.
            let Ok(content) = fs::read_to_string(&key.0) else {
                continue;
            };
            let Some(lines) = part.lines(&content) else {
                self.warnings
                    .push(format!("Include block not found: '{target}'"));
                return None;
            };
            let dir = key.0.parent().map(Path::to_path_buf);
            self.included.insert(key.clone());
            self.expanding.push(key);
            self.inlined += 1;
            let resolved = self.resolve(&lines.join("\n"), dir.as_deref());
            self.expanding.pop();
            return Some(resolved);
        }

        // Without a model, meta-pattern includes (systems/sys_*.iuml etc.)
        // are left for request time, when one is available.
        if self.meta_source.is_some() || !is_meta_include_pattern(target) {
            if candidates.is_empty() {
                self.warnings.push(format!(
                    "Include file not found: '{include_path}' (no include directories configured)"
                ));
            } else {
                let searched: Vec<_> = candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
                self.warnings.push(format!(
                    "Include file not found: '{include_path}' (searched: {})",
                    searched.join(", ")
                ));
            }
        }
        None
    }
}

/// The `skinparam dpi` that [`prepare_diagram_source`] injects: twice the
/// 96-DPI CSS pixel, so diagrams stay sharp on retina displays once scaled
/// back down for display.
pub const PLANTUML_DPI: u32 = 192;

/// Prepare `PlantUML` source for rendering.
///
/// Resolves includes and injects [`PLANTUML_DPI`] and font settings
/// after the `@startuml` directive. Meta includes
/// (`systems/sys_payment_gateway.iuml`) expand from `meta_source`; without one
/// they stay in place, unexpanded and unwarned.
#[must_use]
pub fn prepare_diagram_source(
    source: &str,
    include_dirs: &[PathBuf],
    meta_source: Option<&dyn SiteModel>,
) -> PrepareResult {
    let mut warnings = Vec::new();
    let mut reads = Vec::new();
    let resolved = resolve_includes(source, include_dirs, meta_source, &mut warnings, &mut reads);

    // Inject DPI and font config after @startuml directive
    let config_block = format!(
        "skinparam dpi {PLANTUML_DPI}\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\n"
    );

    // Without a complete `@startuml` line the config is prepended, which not
    // every diagram type accepts.
    let insert_pos = resolved
        .find("@startuml")
        .and_then(|pos| Some(pos + resolved[pos..].find('\n')? + 1))
        .unwrap_or(0);
    let (head, tail) = resolved.split_at(insert_pos);

    PrepareResult {
        source: format!("{head}{config_block}{tail}"),
        warnings,
        reads,
    }
}

/// Inline `PlantUML` `!include` directives in a fence's source, returning
/// `None` when there is nothing to change.
///
/// `None` covers fences outside the `PlantUML` family and sources whose
/// includes all failed to resolve; in both cases the fence is left as the
/// author wrote it. Failures are appended to `warnings`.
///
/// Meta includes are deliberately left unresolved: they expand at request time
/// from live page metadata, which a published bundle does not carry.
#[must_use]
pub fn bundle_source(
    language: &str,
    source: &str,
    include_dirs: &[PathBuf],
    warnings: &mut Vec<String>,
) -> Option<String> {
    if !is_plantuml_fence(language) {
        return None;
    }
    let resolved = resolve_includes(source, include_dirs, None, warnings, &mut Vec::new());
    (resolved != source).then_some(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_diagram_source() {
        let source = "@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        // DPI and font should be injected after @startuml
        assert_eq!(
            result.source,
            "@startuml\nskinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\nAlice -> Bob\n@enduml"
        );
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    #[test]
    fn test_prepare_diagram_source_preserves_content_before_startuml() {
        let source = "' comment\n@startuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        // Content before @startuml should be preserved
        assert_eq!(
            result.source,
            "' comment\n@startuml\nskinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\nAlice -> Bob\n@enduml"
        );
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    #[test]
    fn test_unresolved_include_generates_warning() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("not found"));
    }

    #[test]
    fn test_unresolved_include_with_dirs_shows_searched_paths() {
        let source = "@startuml\n!include missing.iuml\nAlice -> Bob\n@enduml";
        let include_dirs = vec![PathBuf::from("/tmp/includes")];
        let result = prepare_diagram_source(source, &include_dirs, None);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
        assert!(result.warnings[0].contains("/tmp/includes"));
    }

    #[test]
    fn test_stdlib_include_no_warning() {
        let source = "@startuml\n!include <tupadr3/common>\nAlice -> Bob\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        // Stdlib includes should not generate warnings
        assert_eq!(result.warnings, [] as [String; 0]);
        // Stdlib include should be preserved as-is
        assert!(result.source.contains("!include <tupadr3/common>"));
    }

    #[test]
    fn test_indented_include_resolved() {
        // Create a temp file for the include
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("test_component.iuml");
        std::fs::write(&include_path, "Component(comp, \"Component\")").unwrap();

        let source = "@startuml\nSystem_Boundary(sys, \"System\")\n  !include test_component.iuml\nBoundary_End()\n@enduml";
        let result = prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], None);

        assert_eq!(result.warnings, [] as [String; 0]);
        // Indented include should be resolved and content should be indented
        assert!(result.source.contains("  Component(comp, \"Component\")"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_indented_include_warning() {
        let source = "@startuml\nSystem_Boundary(sys, \"System\")\n  !include missing.iuml\nBoundary_End()\n@enduml";
        let result = prepare_diagram_source(source, &[], None);

        // Should generate warning for indented include too
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("missing.iuml"));
    }

    #[test]
    fn test_prepare_diagram_source_no_startuml() {
        let source = "Alice -> Bob";
        let result = prepare_diagram_source(source, &[], None);

        assert_eq!(
            result.source,
            "skinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\nAlice -> Bob"
        );
    }

    #[test]
    fn test_prepare_diagram_source_startuml_no_newline() {
        let source = "@startuml";
        let result = prepare_diagram_source(source, &[], None);

        assert_eq!(
            result.source,
            "skinparam dpi 192\nskinparam defaultFontName Roboto\nskinparam backgroundColor transparent\n@startuml"
        );
    }

    /// Resolve `source` against `dirs`, returning the result and the warnings.
    fn resolve(source: &str, dirs: &[PathBuf]) -> (String, Vec<String>) {
        let mut warnings = Vec::new();
        let resolved = resolve_includes(source, dirs, None, &mut warnings, &mut Vec::new());
        (resolved, warnings)
    }

    /// `PlantUML`'s plain `!include` skips a file the diagram already
    /// included, which is what keeps a self-include, a cycle, or a diamond
    /// (two libraries sharing a third) linear.
    #[test]
    fn a_self_including_file_expands_once() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp_dir.path().join("self.iuml"),
            "!include self.iuml\n!include self.iuml\n!include self.iuml\nSelf",
        )
        .unwrap();

        let (resolved, warnings) = resolve("!include self.iuml", &[temp_dir.path().to_path_buf()]);

        assert_eq!(resolved, "\n\n\nSelf");
        assert_eq!(warnings, [] as [String; 0]);
    }

    #[test]
    fn a_cycle_through_two_files_expands_each_once() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("a.iuml"), "A1\n!include b.iuml\nA2").unwrap();
        std::fs::write(temp_dir.path().join("b.iuml"), "B1\n!include a.iuml\nB2").unwrap();

        let (resolved, warnings) = resolve("!include a.iuml", &[temp_dir.path().to_path_buf()]);

        assert_eq!(resolved, "A1\nB1\n\nB2\nA2");
        assert_eq!(warnings, [] as [String; 0]);
    }

    #[test]
    fn a_file_already_included_elsewhere_expands_to_nothing() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(temp_dir.path().join("lib")).unwrap();
        std::fs::write(temp_dir.path().join("lib/shared.iuml"), "Shared").unwrap();
        std::fs::write(
            temp_dir.path().join("a.iuml"),
            "A\n!include lib/shared.iuml",
        )
        .unwrap();
        std::fs::write(
            temp_dir.path().join("lib/b.iuml"),
            "B\n!include shared.iuml",
        )
        .unwrap();

        let (resolved, warnings) = resolve(
            "!include a.iuml\n!include lib/b.iuml\n!include ./a.iuml",
            &[temp_dir.path().to_path_buf()],
        );

        assert_eq!(resolved, "A\nShared\nB\n\n");
        assert_eq!(warnings, [] as [String; 0]);
    }

    /// Each level includes the next twice: expanding every repeat would
    /// read the bottom file 2^19 times.
    #[test]
    fn a_diamond_chain_reads_each_file_once() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        for level in 0..19 {
            let next = level + 1;
            std::fs::write(
                temp_dir.path().join(format!("d{level}.iuml")),
                format!("D{level}\n!include d{next}.iuml\n!include d{next}.iuml"),
            )
            .unwrap();
        }
        std::fs::write(temp_dir.path().join("d19.iuml"), "Bottom").unwrap();

        let mut reads = Vec::new();
        let resolved = resolve_includes(
            "!include d0.iuml",
            &[temp_dir.path().to_path_buf()],
            None,
            &mut Vec::new(),
            &mut reads,
        );

        assert_eq!(resolved.matches("Bottom").count(), 1, "{resolved}");
        assert_eq!(reads.len(), 1 + 19 * 2);
    }

    /// The same chain through `!include_many`: uncapped, it inlines the
    /// bottom file 2^19 times.
    #[test]
    fn a_doubling_include_many_chain_stops_at_the_include_limit_with_one_warning() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        for level in 0..19 {
            let next = level + 1;
            std::fs::write(
                temp_dir.path().join(format!("d{level}.iuml")),
                format!("D{level}\n!include_many d{next}.iuml\n!include_many d{next}.iuml"),
            )
            .unwrap();
        }
        std::fs::write(temp_dir.path().join("d19.iuml"), "Bottom").unwrap();

        let (resolved, warnings) =
            resolve("!include_many d0.iuml", &[temp_dir.path().to_path_buf()]);

        let inlined = resolved
            .lines()
            .filter(|line| !line.starts_with('!'))
            .count();
        assert_eq!(inlined, MAX_INCLUDES, "{resolved}");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with(&format!("More than {MAX_INCLUDES} includes: 'd")),
            "{warnings:?}"
        );
    }

    /// No fixed depth cap: an acyclic chain deeper than ten expands in full.
    #[test]
    fn a_deep_include_chain_expands_fully() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        for level in 0..20 {
            std::fs::write(
                temp_dir.path().join(format!("l{level}.iuml")),
                format!("L{level}\n!include l{}.iuml", level + 1),
            )
            .unwrap();
        }
        std::fs::write(temp_dir.path().join("l20.iuml"), "Bottom").unwrap();

        let (resolved, warnings) = resolve("!include l0.iuml", &[temp_dir.path().to_path_buf()]);

        assert_eq!(warnings, [] as [String; 0]);
        let expected: Vec<_> = (0..20)
            .map(|level| format!("L{level}"))
            .chain(["Bottom".to_owned()])
            .collect();
        assert_eq!(resolved, expected.join("\n"));
    }

    /// Run on a 2 MiB stack, a tokio worker's: without the backstop a chain
    /// of distinct files recurses until the process aborts.
    #[test]
    fn a_chain_past_the_nesting_limit_stops_with_one_warning() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let files = 2_000;
        for level in 0..files {
            std::fs::write(
                temp_dir.path().join(format!("l{level}.iuml")),
                format!(
                    "L{level}\n!include l{0}.iuml\n!include l{0}.iuml",
                    level + 1
                ),
            )
            .unwrap();
        }
        let dirs = [temp_dir.path().to_path_buf()];

        let (resolved, warnings) = std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || resolve("!include l0.iuml", &dirs))
            .unwrap()
            .join()
            .unwrap();

        let last = MAX_INCLUDE_DEPTH - 1;
        let left = format!("!include l{MAX_INCLUDE_DEPTH}.iuml");
        assert!(
            resolved.contains(&format!("L{last}\n{left}\n{left}\n")),
            "{resolved}"
        );
        assert_eq!(
            warnings,
            [format!(
                "Include nesting deeper than {MAX_INCLUDE_DEPTH}: 'l{MAX_INCLUDE_DEPTH}.iuml'"
            )]
        );
    }

    /// `PlantUML` looks a nested include up next to the including file before
    /// its include path, so a vendored library's own includes resolve.
    #[test]
    fn a_nested_include_is_searched_next_to_the_including_file_first() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let lib = temp_dir.path().join("vendor/lib");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("main.iuml"), "Main\n!include common.iuml").unwrap();
        std::fs::write(lib.join("common.iuml"), "LibCommon").unwrap();
        std::fs::write(temp_dir.path().join("common.iuml"), "RootCommon").unwrap();

        let (resolved, warnings) = resolve(
            "!include vendor/lib/main.iuml\n!include common.iuml",
            &[temp_dir.path().to_path_buf()],
        );

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(resolved, "Main\nLibCommon\nRootCommon");
    }

    #[test]
    fn a_nested_parent_dir_include_inside_the_include_dir_resolves() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(temp_dir.path().join("lib")).unwrap();
        std::fs::write(
            temp_dir.path().join("lib/main.iuml"),
            "!include ../shared.iuml",
        )
        .unwrap();
        std::fs::write(temp_dir.path().join("shared.iuml"), "Shared").unwrap();

        let (resolved, warnings) =
            resolve("!include lib/main.iuml", &[temp_dir.path().to_path_buf()]);

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(resolved, "Shared");
    }

    /// Even one that points into an include directory: an absolute path
    /// names a file on the machine that renders, not in the docs.
    #[test]
    fn an_absolute_include_is_rejected_unread() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let inside = temp_dir.path().join("inside.iuml");
        std::fs::write(&inside, "Inside").unwrap();
        let outside = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(outside.path(), "Secret").unwrap();
        let dirs = [temp_dir.path().to_path_buf()];

        for path in [outside.path(), &inside] {
            let source = format!("!include {}", path.display());
            let (resolved, warnings) = resolve(&source, &dirs);

            assert_eq!(resolved, source);
            assert_eq!(
                warnings,
                [format!(
                    "Include path must be relative to the include directories: '{}'",
                    path.display()
                )]
            );
        }
    }

    #[test]
    fn a_parent_dir_include_escaping_the_include_dirs_is_rejected_unread() {
        let root = tempfile::tempdir().expect("tempdir");
        let includes = root.path().join("includes");
        std::fs::create_dir(&includes).unwrap();
        std::fs::write(root.path().join("secret.txt"), "Secret").unwrap();

        let (resolved, warnings) = resolve("!include ../secret.txt", &[includes]);

        assert_eq!(resolved, "!include ../secret.txt");
        assert_eq!(
            warnings,
            ["Include file is outside the include directories: '../secret.txt'"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_escaping_the_include_dirs_is_rejected_unread() {
        let root = tempfile::tempdir().expect("tempdir");
        let includes = root.path().join("includes");
        std::fs::create_dir(&includes).unwrap();
        std::fs::write(root.path().join("secret.txt"), "Secret").unwrap();
        std::os::unix::fs::symlink(root.path().join("secret.txt"), includes.join("link.iuml"))
            .unwrap();

        let (resolved, warnings) = resolve("!include link.iuml", &[includes]);

        assert_eq!(resolved, "!include link.iuml");
        assert_eq!(
            warnings,
            ["Include file is outside the include directories: 'link.iuml'"]
        );
    }

    /// An include directory that is itself a symlink confines to its target.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_include_dir_still_resolves() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(root.path().join("real")).unwrap();
        std::fs::write(root.path().join("real/a.iuml"), "A").unwrap();
        std::os::unix::fs::symlink(root.path().join("real"), root.path().join("linked")).unwrap();

        let (resolved, warnings) = resolve("!include a.iuml", &[root.path().join("linked")]);

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(resolved, "A");
    }

    /// `PlantUML` fetches these itself; neither is a path on disk.
    #[test]
    fn url_includes_pass_through_silently() {
        let source = "\
!include https://example.com/C4_Context.puml
!include http://example.com/x.puml
!includeurl https://example.com/x.puml";

        let (resolved, warnings) = resolve(source, &[]);

        assert_eq!(resolved, source);
        assert_eq!(warnings, [] as [String; 0]);
    }

    /// Its own markers would otherwise nest inside the including diagram.
    #[test]
    fn a_file_holding_diagrams_contributes_the_first_ones_body() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp_dir.path().join("part.puml"),
            "' header\n  @startuml\nP0\n@enduml\n@startuml\nP1\n@enduml\n",
        )
        .unwrap();

        let (resolved, warnings) = resolve(
            "@startuml\n!include part.puml\n@enduml",
            &[temp_dir.path().to_path_buf()],
        );

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(resolved, "@startuml\nP0\n@enduml");
    }

    #[test]
    fn a_block_selector_picks_a_diagram_by_index_or_id() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp_dir.path().join("part.puml"),
            "@startuml\nP0\n@enduml\n@startuml\nP1\n@enduml\n@startuml(id=ID)\nP2\n@enduml",
        )
        .unwrap();
        let source = "\
!include part.puml!1
!include part.puml!ID
!include part.puml!0
!include part.puml!1
!include part.puml";

        let (resolved, warnings) = resolve(source, &[temp_dir.path().to_path_buf()]);

        assert_eq!(warnings, [] as [String; 0]);
        // `part.puml` without a selector is a different include from `!0`.
        assert_eq!(resolved, "P1\nP2\nP0\n\nP0");
    }

    #[test]
    fn a_block_selector_matching_no_diagram_is_left_with_a_warning() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp_dir.path().join("part.puml"),
            "@startuml(id=IDX)\nP0\n@enduml",
        )
        .unwrap();
        let source = "!include part.puml!1\n!include part.puml!ID";

        let (resolved, warnings) = resolve(source, &[temp_dir.path().to_path_buf()]);

        assert_eq!(resolved, source);
        assert_eq!(
            warnings,
            [
                "Include block not found: 'part.puml!1'",
                "Include block not found: 'part.puml!ID'",
            ]
        );
    }

    #[test]
    fn include_once_skips_a_file_already_included() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("a.iuml"), "A").unwrap();

        let (resolved, warnings) = resolve(
            "!include_many a.iuml\n!include_once a.iuml\n!include_once a.iuml\n!include a.iuml",
            &[temp_dir.path().to_path_buf()],
        );

        assert_eq!(resolved, "A\n\n\n");
        // `PlantUML` rejects the diagram for each repeat.
        assert_eq!(
            warnings,
            [
                "Include file already included: 'a.iuml'",
                "Include file already included: 'a.iuml'",
            ]
        );
    }

    #[test]
    fn include_many_expands_a_file_every_time() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("a.iuml"), "A").unwrap();

        let (resolved, warnings) = resolve(
            "!include a.iuml\n!include_many a.iuml\n!include_many a.iuml\n!include a.iuml",
            &[temp_dir.path().to_path_buf()],
        );

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(resolved, "A\nA\nA\n");
    }

    /// Each file includes the other twice: without the cycle check the
    /// expansion doubles at every level down to the nesting limit.
    #[test]
    fn an_include_many_cycle_is_left_with_a_warning() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp_dir.path().join("a.iuml"),
            "A\n!include_many b.iuml\n!include_many b.iuml",
        )
        .unwrap();
        std::fs::write(temp_dir.path().join("b.iuml"), "B\n!include_many a.iuml").unwrap();

        let (resolved, warnings) =
            resolve("!include_many a.iuml", &[temp_dir.path().to_path_buf()]);

        assert_eq!(
            resolved,
            "A\nB\n!include_many a.iuml\nB\n!include_many a.iuml"
        );
        assert_eq!(
            warnings,
            ["Include cycle: 'a.iuml'", "Include cycle: 'a.iuml'"]
        );
    }

    #[test]
    fn includesub_takes_every_block_of_the_named_sub() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp_dir.path().join("lib.iuml"),
            "\
!startsub A
A1
!endsub
X
!startsub B
B1
!endsub
  !startsub A
A2
  !endsub",
        )
        .unwrap();

        let (resolved, warnings) = resolve(
            "!includesub lib.iuml!A\n!includesub lib.iuml!A\n!includesub lib.iuml!B",
            &[temp_dir.path().to_path_buf()],
        );

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(resolved, "A1\nA2\nA1\nA2\nB1");
    }

    /// A file may take a sub from itself; only re-entering the same sub is a
    /// cycle.
    #[test]
    fn includesub_of_the_including_file_resolves_and_a_self_including_sub_does_not() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp_dir.path().join("lib.iuml"),
            "!startsub A\nA1\n!endsub\n!includesub lib.iuml!A",
        )
        .unwrap();
        std::fs::write(
            temp_dir.path().join("loop.iuml"),
            "!startsub LOOP\nL\n!includesub loop.iuml!LOOP\n!endsub",
        )
        .unwrap();

        let (resolved, warnings) = resolve(
            "!include lib.iuml\n!includesub loop.iuml!LOOP",
            &[temp_dir.path().to_path_buf()],
        );

        assert_eq!(
            resolved,
            "!startsub A\nA1\n!endsub\nA1\nL\n!includesub loop.iuml!LOOP"
        );
        assert_eq!(warnings, ["Include cycle: 'loop.iuml!LOOP'"]);
    }

    /// Without a file, the sub is defined in the diagram itself.
    #[test]
    fn includesub_without_a_file_passes_through_silently() {
        let (resolved, warnings) = resolve("!includesub LOCAL", &[]);

        assert_eq!(resolved, "!includesub LOCAL");
        assert_eq!(warnings, [] as [String; 0]);
    }

    #[test]
    fn includesub_of_a_missing_sub_is_left_with_a_warning() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("lib.iuml"), "!startsub A\nA1\n!endsub").unwrap();

        let (resolved, warnings) =
            resolve("!includesub lib.iuml!B", &[temp_dir.path().to_path_buf()]);

        assert_eq!(resolved, "!includesub lib.iuml!B");
        assert_eq!(warnings, ["Include block not found: 'lib.iuml!B'"]);
    }

    #[test]
    fn a_files_final_newline_adds_no_blank_line() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("a.iuml"), "A\n").unwrap();

        let (resolved, warnings) = resolve("!include a.iuml\nB", &[temp_dir.path().to_path_buf()]);

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(resolved, "A\nB");
    }

    #[test]
    fn bundle_source_never_inlines_a_file_outside_the_include_dirs() {
        let root = tempfile::tempdir().expect("tempdir");
        let includes = [root.path().join("includes")];
        std::fs::create_dir(&includes[0]).unwrap();
        let secret = root.path().join("secret.txt");
        std::fs::write(&secret, "Secret").unwrap();

        let mut warnings = Vec::new();
        for include in ["../secret.txt".to_owned(), secret.display().to_string()] {
            let source = format!("@startuml\n!include {include}\n@enduml");
            assert_eq!(
                bundle_source("plantuml", &source, &includes, &mut warnings),
                None
            );
        }
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    #[test]
    fn test_multiple_includes_resolved() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include1 = temp_dir.path().join("part1.iuml");
        let include2 = temp_dir.path().join("part2.iuml");
        std::fs::write(&include1, "Alice -> Bob").unwrap();
        std::fs::write(&include2, "Bob -> Charlie").unwrap();

        let source = "@startuml\n!include part1.iuml\n!include part2.iuml\n@enduml";
        let result = prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], None);

        assert_eq!(result.warnings, [] as [String; 0]);
        assert!(result.source.contains("Alice -> Bob"));
        assert!(result.source.contains("Bob -> Charlie"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_nested_includes() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let outer = temp_dir.path().join("outer.iuml");
        let inner = temp_dir.path().join("inner.iuml");
        std::fs::write(&inner, "InnerContent").unwrap();
        std::fs::write(&outer, "OuterBefore\n!include inner.iuml\nOuterAfter").unwrap();

        let source = "@startuml\n!include outer.iuml\n@enduml";
        let result = prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], None);

        assert_eq!(result.warnings, [] as [String; 0]);
        assert!(result.source.contains("OuterBefore"));
        assert!(result.source.contains("InnerContent"));
        assert!(result.source.contains("OuterAfter"));
    }

    /// Misses count: creating `outer.iuml` in the first directory, `inner.iuml`
    /// next to `outer.iuml`, or the missing `absent.iuml` anywhere, would
    /// change the prepared source.
    #[test]
    fn every_probed_candidate_is_reported_with_its_stamp() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        std::fs::write(second.path().join("outer.iuml"), "!include inner.iuml").unwrap();
        std::fs::write(first.path().join("inner.iuml"), "Inner").unwrap();
        let dirs = [first.path().to_path_buf(), second.path().to_path_buf()];

        let source = "@startuml\n!include outer.iuml\n!include absent.iuml\n@enduml";
        let result = prepare_diagram_source(source, &dirs, None);

        let stamp = |dir: &Path, name: &str| FileStamp::of(dir.join(name));
        let [first, second] = &dirs;
        // `second` is also the including file's directory, so it is probed once.
        assert_eq!(
            result.reads,
            [
                stamp(first, "outer.iuml"),
                stamp(second, "outer.iuml"),
                stamp(&second.canonicalize().unwrap(), "inner.iuml"),
                stamp(first, "inner.iuml"),
                stamp(first, "absent.iuml"),
                stamp(second, "absent.iuml"),
            ],
        );
        assert_eq!(result.reads[0].modified, None);
        assert!(result.reads[1].modified.is_some());
    }

    #[test]
    fn test_indented_content_empty_lines_preserved() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("with_empty.iuml");
        std::fs::write(&include_path, "Line1\n\nLine3").unwrap();

        let source = "@startuml\n  !include with_empty.iuml\n@enduml";
        let result = prepare_diagram_source(source, &[temp_dir.path().to_path_buf()], None);

        assert_eq!(result.warnings, [] as [String; 0]);
        // Empty lines should remain empty (not indented)
        assert!(result.source.contains("  Line1\n\n  Line3"));
    }

    #[test]
    fn test_include_after_blank_line_keeps_line_structure() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("after_blank_line.iuml");
        std::fs::write(&include_path, "line1\nline2\nline3").unwrap();

        let source = "@startuml\n\n!include after_blank_line.iuml\n@enduml";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
            &mut Vec::new(),
        );

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(result, "@startuml\n\nline1\nline2\nline3\n@enduml");
    }

    #[test]
    fn test_indented_include_after_blank_line_indents_without_injecting_blanks() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("indented_after_blank.iuml");
        std::fs::write(&include_path, "line1\nline2").unwrap();

        let source = "@startuml\n\n  !include indented_after_blank.iuml\n@enduml";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
            &mut Vec::new(),
        );

        assert_eq!(warnings, [] as [String; 0]);
        assert_eq!(result, "@startuml\n\n  line1\n  line2\n@enduml");
    }

    #[test]
    fn test_crlf_include_after_blank_line_not_indented_by_carriage_return() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let include_path = temp_dir.path().join("crlf_after_blank.iuml");
        std::fs::write(&include_path, "line1\nline2").unwrap();

        let source = "@startuml\r\n\r\n!include crlf_after_blank.iuml\r\n@enduml";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
            &mut Vec::new(),
        );

        assert_eq!(warnings, [] as [String; 0]);
        // Characterizes CRLF handling; it cannot fail independently of the LF
        // cases above, since both corruptions came from `\n` being in the
        // capture's character class. Kept because this is the crate's only
        // CRLF coverage, and because it pins a wart: `(.+)$` swallows the
        // include line's `\r` into the match and the expansion never restores
        // it, so `@enduml` below arrives with a bare `\n`.
        assert_eq!(result, "@startuml\r\n\r\nline1\nline2\n@enduml");
    }

    #[test]
    fn include_does_not_expand_inside_a_longer_path() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("a.iuml"), "Alice -> Bob").unwrap();

        let source = "!include a.iuml\n!include a.iuml.bak";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
            &mut Vec::new(),
        );

        assert_eq!(result, "Alice -> Bob\n!include a.iuml.bak");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("a.iuml.bak"), "{warnings:?}");
    }

    #[test]
    fn commented_out_include_is_left_alone() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("a.iuml"), "Alice -> Bob").unwrap();

        let source = "' !include a.iuml\n!include a.iuml";
        let mut warnings = Vec::new();
        let result = resolve_includes(
            source,
            &[temp_dir.path().to_path_buf()],
            None,
            &mut warnings,
            &mut Vec::new(),
        );

        assert_eq!(result, "' !include a.iuml\nAlice -> Bob");
        assert_eq!(warnings, [] as [String; 0]);
    }

    use rw_diagrams::{Entity, SiteModel};

    struct TestMetaSource;

    impl SiteModel for TestMetaSource {
        fn entity(&self, kind: &str, name: &str) -> Option<Entity> {
            if kind == "system" && name == "payment-gateway" {
                Some(Entity {
                    title: "Payment Gateway".to_owned(),
                    description: Some("Processes payments".to_owned()),
                    url_path: Some("/domains/billing/systems/payment-gateway".to_owned()),
                })
            } else {
                None
            }
        }
    }

    #[test]
    fn test_meta_include_resolves_before_filesystem() {
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\nA -> B\n@enduml";
        let meta_source = TestMetaSource;
        let result = prepare_diagram_source(source, &[], Some(&meta_source));
        assert_eq!(result.warnings, [] as [String; 0]);
        assert!(result.source.contains("System(sys_payment_gateway,"));
        assert!(result.source.contains("Payment Gateway"));
        assert!(!result.source.contains("!include"));
    }

    #[test]
    fn test_meta_include_falls_back_to_filesystem() {
        let source = "@startuml\n!include systems/sys_unknown.iuml\n@enduml";
        let meta_source = TestMetaSource;
        let result = prepare_diagram_source(source, &[], Some(&meta_source));
        assert_ne!(result.warnings, [] as [String; 0]);
        assert!(result.warnings[0].contains("sys_unknown.iuml"));
    }

    #[test]
    fn test_meta_pattern_include_no_warning_without_meta_source() {
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let result = prepare_diagram_source(source, &[], None);
        assert!(
            result.warnings.is_empty(),
            "Meta-pattern includes should not warn when no meta source: {:?}",
            result.warnings
        );
        assert!(
            result
                .source
                .contains("!include systems/sys_payment_gateway.iuml")
        );
    }

    #[test]
    fn bundle_source_skips_non_plantuml_fences() {
        // Every fixture carries a resolvable `!include`, so deleting the language
        // guard makes `bundle_source` rewrite the source and return `Some` — these
        // assertions fail. Fixtures without a resolvable include would pass either
        // way and prove nothing.
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("shared.iuml"), "included content")
            .expect("write include");
        let include_dirs = [temp_dir.path().to_path_buf()];

        let mut warnings = Vec::new();
        assert_eq!(
            bundle_source(
                "mermaid",
                "graph TD\nA --> B\n!include shared.iuml",
                &include_dirs,
                &mut warnings
            ),
            None
        );
        assert_eq!(
            bundle_source(
                "rust",
                "fn main() {}\n!include shared.iuml",
                &include_dirs,
                &mut warnings
            ),
            None
        );
        assert_eq!(
            bundle_source(
                "graphviz",
                "digraph {}\n!include shared.iuml",
                &include_dirs,
                &mut warnings
            ),
            None
        );
        assert_eq!(
            bundle_source(
                "console",
                "$ cat notes.txt\n!include shared.iuml",
                &include_dirs,
                &mut warnings
            ),
            None
        );
        assert!(
            warnings.is_empty(),
            "non-PlantUML fences warn: {warnings:?}"
        );
    }

    #[test]
    fn bundle_source_returns_none_when_there_are_no_includes() {
        let mut warnings = Vec::new();
        assert_eq!(
            bundle_source(
                "plantuml",
                "@startuml\nAlice -> Bob\n@enduml",
                &[],
                &mut warnings
            ),
            None
        );
        assert_eq!(
            bundle_source(
                "c4plantuml",
                "@startuml\nPerson(user, \"User\")\n@enduml",
                &[],
                &mut warnings,
            ),
            None
        );
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn bundle_source_inlines_a_filesystem_include() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("bundle_test.iuml"), "Bob -> Charlie")
            .expect("write include");

        let mut warnings = Vec::new();
        let result = bundle_source(
            "plantuml",
            "@startuml\n!include bundle_test.iuml\n@enduml",
            &[temp_dir.path().to_path_buf()],
            &mut warnings,
        );

        assert_eq!(
            result.as_deref(),
            Some("@startuml\nBob -> Charlie\n@enduml"),
        );
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn bundle_source_accepts_the_kroki_prefixed_spelling() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp_dir.path().join("kroki_prefixed.iuml"),
            "Bob -> Charlie",
        )
        .expect("write include");

        let mut warnings = Vec::new();
        let result = bundle_source(
            "kroki-plantuml",
            "@startuml\n!include kroki_prefixed.iuml\n@enduml",
            &[temp_dir.path().to_path_buf()],
            &mut warnings,
        );

        assert_eq!(
            result.as_deref(),
            Some("@startuml\nBob -> Charlie\n@enduml"),
        );
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn bundle_source_inlines_what_it_can_and_warns_about_the_rest() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp_dir.path().join("good.iuml"), "Bob -> Charlie").expect("write include");
        let mut warnings = Vec::new();
        let result = bundle_source(
            "plantuml",
            "@startuml\n!include good.iuml\n!include missing.iuml\n@enduml",
            &[temp_dir.path().to_path_buf()],
            &mut warnings,
        );
        assert_eq!(
            result.as_deref(),
            Some("@startuml\nBob -> Charlie\n!include missing.iuml\n@enduml"),
        );
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("missing.iuml"), "{warnings:?}");
    }

    #[test]
    fn bundle_source_leaves_meta_includes_for_request_time() {
        struct OneSystem;
        impl SiteModel for OneSystem {
            fn entity(&self, kind: &str, name: &str) -> Option<Entity> {
                (kind == "system" && name == "payment-gateway").then(|| Entity {
                    title: "Payment Gateway".to_owned(),
                    description: Some("Processes payments".to_owned()),
                    url_path: None,
                })
            }
        }

        // The model is reachable, and `resolve_includes` would expand this
        // include if handed it — `bundle_source` passes `None` on purpose.
        let mut with_model = Vec::new();
        let source = "@startuml\n!include systems/sys_payment_gateway.iuml\n@enduml";
        let expanded = resolve_includes(
            source,
            &[],
            Some(&OneSystem),
            &mut with_model,
            &mut Vec::new(),
        );
        assert!(
            expanded.contains("Payment Gateway"),
            "fixture is inert: {expanded}"
        );

        let mut warnings = Vec::new();
        assert_eq!(bundle_source("plantuml", source, &[], &mut warnings), None);
        assert!(
            warnings.is_empty(),
            "meta includes must not warn: {warnings:?}"
        );
    }

    #[test]
    fn bundle_source_reports_an_unresolvable_include() {
        let mut warnings = Vec::new();
        let result = bundle_source(
            "plantuml",
            "@startuml\n!include nonexistent.iuml\n@enduml",
            &[],
            &mut warnings,
        );

        assert_eq!(result, None, "an unchanged source bundles to None");
        assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
        assert!(
            warnings[0].contains("Include file not found")
                && warnings[0].contains("nonexistent.iuml"),
            "unexpected warning: {}",
            warnings[0],
        );
    }
}
