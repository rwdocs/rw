//! `::include[path]`: the markdown file a page renders in place of the line.
//!
//! The walker renders the included file's events itself; this module places
//! the file and names what can go wrong reading it.

use std::borrow::Cow;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use crate::file::{SEPARATORS, fold, is_external};

/// Reads the markdown files `::include[path]` lines name.
///
/// # Examples
///
/// ```
/// use rw_renderer::{HtmlBackend, IncludeError, IncludeReader, MarkdownRenderer};
///
/// struct Partials;
///
/// impl IncludeReader for Partials {
///     fn read(&self, path: &str) -> Result<String, IncludeError> {
///         match path {
///             "docs/_partials/note.md" => Ok("Mind the gap.".to_owned()),
///             _ => Err(IncludeError::NotFound),
///         }
///     }
/// }
///
/// let result = MarkdownRenderer::<HtmlBackend>::new()
///     .with_source_path("docs/guide.md")
///     .with_include_reader(Partials)
///     .render("::include[_partials/note.md]\n\n::include[/missing.md]\n");
///
/// assert!(result.html.starts_with("<p>Mind the gap.</p>"));
/// assert_eq!(Vec::from_iter(&result.includes), ["docs/_partials/note.md"]);
/// assert_eq!(result.warnings, ["include `missing.md` not found"]);
/// ```
pub trait IncludeReader: Send + Sync {
    /// The text of the markdown file at `path`, or why it can't be read.
    ///
    /// `path` is relative to the project root, folded, inside the project as
    /// written, and ends in `.md` in any case. A symlink may still lead out
    /// of the project or to a file that is not markdown; the reader refuses
    /// those.
    fn read(&self, path: &str) -> Result<String, IncludeError>;
}

/// Why an [`IncludeReader`] returns no text. The page warns with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludeError {
    /// No such file.
    NotFound,
    /// No file spelled so: the filesystem found one only by ignoring case.
    /// `on_disk` is its project-relative path as stored.
    Misspelled { on_disk: String },
    /// The file is outside the project.
    OutsideProject,
    /// The file is not a markdown file.
    NotMarkdown,
    /// The file is larger than the reader takes.
    TooLarge,
    /// The file is not UTF-8 text.
    NotUtf8,
    /// Any other failure, described for the writer.
    Other(String),
}

/// The rest of a warning that starts ``include `path` ``.
impl Display for IncludeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::Misspelled { on_disk } => {
                write!(f, "not found; the file on disk is `{on_disk}`")
            }
            Self::OutsideProject => f.write_str("is outside the project"),
            Self::NotMarkdown => f.write_str("is not a markdown file"),
            Self::TooLarge => f.write_str("is too large"),
            Self::NotUtf8 => f.write_str("is not UTF-8 text"),
            Self::Other(message) => write!(f, "can't be read: {message}"),
        }
    }
}

impl Error for IncludeError {}

/// The project file `src` names from the file at `source_path`, placed as an
/// image's is but taken literally: a `?` or `%` is part of the name.
pub(crate) fn project_path(source_path: &str, src: &str) -> Result<String, IncludeError> {
    if is_external(src) {
        return Err(IncludeError::OutsideProject);
    }
    let (dir, path) = match src.strip_prefix(SEPARATORS) {
        Some(path) => ("", path),
        None => (source_path.rsplit_once('/').map_or("", |(dir, _)| dir), src),
    };
    if matches!(path.rsplit(SEPARATORS).next(), Some("" | "." | "..")) {
        return Err(IncludeError::NotMarkdown);
    }
    let path = fold(dir, path.split(SEPARATORS).map(Cow::Borrowed));
    if path.starts_with("../") {
        return Err(IncludeError::OutsideProject);
    }
    if !Path::new(&path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return Err(IncludeError::NotMarkdown);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::mpsc::{self, Receiver, Sender};

    use rw_diagrams::{
        Asset, DiagramContent, DiagramError, DiagramRouter, DiagramSource, Resolved,
    };

    use super::*;
    use crate::file::resolvers::Echo;
    use crate::link::Pages;
    use crate::{
        HtmlBackend, MarkdownRenderer, RenderBackend, RenderResult, SearchDocumentBackend,
    };

    struct Reader(fn(&str) -> Result<&'static str, IncludeError>);

    impl IncludeReader for Reader {
        fn read(&self, path: &str) -> Result<String, IncludeError> {
            (self.0)(path).map(ToOwned::to_owned)
        }
    }

    fn project(path: &str) -> Result<&'static str, IncludeError> {
        match path {
            "docs/part.md" | "docs/PART.MD" => Ok("Part."),
            "docs/_partials/links.md" => Ok("[a](a.md) ![i](img/i.png)\n\n![d](flow.puml)\n"),
            "docs/heading.md" => Ok("---\ntitle: Ignored\n---\n# Setup\n\nText.\n"),
            "docs/tab.md" => Ok(":::tab[X]\n\nx\n"),
            "docs/diagram.md" => Ok("```plantuml {#flow}\nB\n```\n\n![d](flow.puml)\n"),
            "docs/tabs.md" => Ok("::::tabs\n:::tab[Y]\ny\n:::\n::::\n"),
            "docs/big.md" => Err(IncludeError::TooLarge),
            "docs/bytes.md" => Err(IncludeError::NotUtf8),
            "docs/link.md" => Err(IncludeError::OutsideProject),
            "docs/Note.md" => Err(IncludeError::Misspelled {
                on_disk: "docs/note.md".to_owned(),
            }),
            "docs/disk.md" => Err(IncludeError::Other("disk on fire".to_owned())),
            _ => Err(IncludeError::NotFound),
        }
    }

    fn renderer<B: RenderBackend>() -> MarkdownRenderer<B> {
        MarkdownRenderer::<B>::new()
            .with_source_path("docs/guide.md")
            .with_include_reader(Reader(project))
    }

    fn render(markdown: &str) -> RenderResult {
        renderer::<HtmlBackend>().render(markdown)
    }

    struct Diagrams;

    impl DiagramRouter for Diagrams {
        fn handles(&self, _language: &str) -> bool {
            true
        }
    }

    #[test]
    fn an_include_renders_the_file_in_place_of_the_line() {
        let result =
            render("Before\n\n::include[ part.md ]\n\n::include[/docs/PART.MD]\n\nAfter\n");
        assert_eq!(
            result.html,
            "<p>Before</p><p>Part.</p><p>Part.</p><p>After</p>"
        );
        assert_eq!(result.warnings, [] as [String; 0]);
        assert_eq!(
            Vec::from_iter(&result.includes),
            ["docs/PART.MD", "docs/part.md"]
        );
    }

    #[test]
    fn links_images_and_diagram_files_resolve_from_the_included_files_folder() {
        let renderer = renderer::<HtmlBackend>()
            .with_link_resolver(Pages(&[]))
            .with_file_resolver(Echo)
            .with_diagram_languages(Arc::new(Diagrams));
        let pass = renderer.begin(
            "```plantuml\nA\n```\n\n::include[_partials/links.md]\n\n[b](b.md) ![j](img/j.png)\n",
        );
        assert_eq!(
            Vec::from_iter(pass.requests().iter().map(|request| &request.source)),
            [
                &DiagramSource::Inline("A\n".to_owned()),
                &DiagramSource::File("docs/_partials/flow.puml".to_owned())
            ]
        );
        let result = pass.finish(&[]);
        assert!(
            result.html.contains(r#"href="/docs/_partials/a""#),
            "{}",
            result.html
        );
        assert!(result.html.contains(r#"href="/docs/b""#), "{}", result.html);
        assert_eq!(
            Vec::from_iter(&result.files),
            ["docs/_partials/img/i.png", "docs/img/j.png"]
        );
    }

    #[test]
    fn included_headings_are_ordinary_page_headings() {
        for renderer in [renderer::<HtmlBackend>(), renderer().with_title_dropped()] {
            let result = renderer.render("# Guide\n\n## Setup\n\n::include[heading.md]\n");
            assert_eq!(result.title.as_deref(), Some("Guide"));
            assert!(
                result
                    .html
                    .ends_with(r#"<h1 id="setup-1">Setup</h1><p>Text.</p>"#),
                "{}",
                result.html
            );
            let ids = Vec::from_iter(result.toc.iter().map(|entry| entry.id.as_str()));
            assert_eq!(ids, ["setup", "setup-1"]);
        }
        assert_eq!(render("::include[heading.md]\n").title, None);
    }

    #[test]
    fn an_included_file_keeps_its_containers_to_itself() {
        let result = render(
            "::::tabs\n\n:::tab[A]\n\n::include[tab.md]\n\n:::\n\n:::tab[B]\n\n::include[tabs.md]\n\n:::\n\n::::\n",
        );
        assert_eq!(
            result.warnings,
            [
                "docs/tab.md: `:::tab` outside a `::::tabs` group; its content is rendered without tab chrome",
                "docs/tab.md: unclosed container directive :::tab (missing closing :::)",
            ]
        );
        assert_eq!(
            result.html.matches(r#"role="tab""#).count(),
            3,
            "{}",
            result.html
        );
        assert!(result.html.contains(r#"id="tab-1-2""#), "{}", result.html);
    }

    #[test]
    fn an_include_in_a_list_item_or_quote_renders_inside_it() {
        for (markdown, html) in [
            (
                "- a\n\n  ::include[part.md]\n",
                "<ul><li><p>a</p><p>Part.</p></li></ul>",
            ),
            (
                "> ::include[part.md]\n",
                "<blockquote><p>Part.</p></blockquote>",
            ),
        ] {
            assert_eq!(render(markdown).html, html);
        }
    }

    #[test]
    fn a_refused_include_warns_and_renders_as_written() {
        for (src, warning) in [
            ("../../x.md", "include `../../x.md` is outside the project"),
            ("/../x.md", "include `/../x.md` is outside the project"),
            (
                "https://a.b/x.md",
                "include `https://a.b/x.md` is outside the project",
            ),
            ("notes.txt", "include `notes.txt` is not a markdown file"),
            ("part.md/", "include `part.md/` is not a markdown file"),
            ("part.md/.", "include `part.md/.` is not a markdown file"),
            (
                "part.md#intro",
                "include `part.md#intro` can't name a section",
            ),
            ("%70art.md", "include `docs/%70art.md` not found"),
            ("big.md", "include `docs/big.md` is too large"),
            ("bytes.md", "include `docs/bytes.md` is not UTF-8 text"),
            ("link.md", "include `docs/link.md` is outside the project"),
            (
                "Note.md",
                "include `docs/Note.md` not found; the file on disk is `docs/note.md`",
            ),
            (
                "disk.md",
                "include `docs/disk.md` can't be read: disk on fire",
            ),
        ] {
            let result = render(&format!("::include[{src}]\n"));
            assert_eq!(result.html, format!("<p>::include[{src}]</p>"));
            assert_eq!(result.warnings, [warning]);
            assert!(result.includes.is_empty());
        }
        let result = render("::include[part.md]{shift=1}\n");
        assert_eq!(result.html, "<p>::include[part.md]{shift=1}</p>");
        assert_eq!(result.warnings, ["include `part.md` takes no attributes"]);
    }

    #[test]
    fn an_include_needs_a_path() {
        let result = render(
            "::include[]

::include

::include{a=1}

::include[ ]
",
        );
        assert_eq!(
            result.html,
            "<p>::include[]</p><p>::include</p><p>::include{a=1}</p><p>::include[ ]</p>"
        );
        assert_eq!(result.warnings, ["include needs a path"]);
    }

    #[test]
    fn includes_need_a_reader_and_a_source_path() {
        for renderer in [
            MarkdownRenderer::<HtmlBackend>::new().with_source_path("docs/guide.md"),
            MarkdownRenderer::new().with_include_reader(Reader(project)),
        ] {
            let result = renderer.render("::include[/docs/part.md]\n\n::include[/a.md]\n");
            assert_eq!(
                result.html,
                "<p>::include[/docs/part.md]</p><p>::include[/a.md]</p>"
            );
            assert_eq!(result.warnings, ["includes are not available here"]);
        }
    }

    /// Project files by path, and the path of each read, in order.
    struct Files {
        reader: FileReader,
        reads: Receiver<String>,
    }

    #[derive(Clone)]
    struct FileReader {
        texts: BTreeMap<&'static str, String>,
        reads: Sender<String>,
    }

    impl Files {
        fn new<T: Into<String>>(files: impl IntoIterator<Item = (&'static str, T)>) -> Self {
            let texts = files.into_iter().map(|(path, text)| (path, text.into()));
            let (sender, reads) = mpsc::channel();
            let reader = FileReader {
                texts: texts.collect(),
                reads: sender,
            };
            Self { reader, reads }
        }

        fn render(&self, markdown: &str) -> RenderResult {
            MarkdownRenderer::<HtmlBackend>::new()
                .with_source_path("docs/guide.md")
                .with_link_resolver(Pages(&[]))
                .with_include_reader(self.reader.clone())
                .render(markdown)
        }

        fn reads(&self) -> Vec<String> {
            self.reads.try_iter().collect()
        }
    }

    impl IncludeReader for FileReader {
        fn read(&self, path: &str) -> Result<String, IncludeError> {
            self.reads.send(path.to_owned()).unwrap();
            self.texts.get(path).cloned().ok_or(IncludeError::NotFound)
        }
    }

    #[test]
    fn a_nested_include_resolves_from_its_own_files_folder() {
        let files = Files::new([
            ("docs/a/one.md", "[1](x.md)\n\n::include[../b/two.md]\n"),
            ("docs/b/two.md", "[2](x.md)\n\n::include[/three.md]\n"),
            ("three.md", "[3](x.md)\n"),
        ]);
        let result = files.render("::include[a/one.md]\n");
        assert_eq!(
            result.html,
            r#"<p><a href="/docs/a/x">1</a></p><p><a href="/docs/b/x">2</a></p><p><a href="/x">3</a></p>"#
        );
        assert_eq!(result.warnings, [] as [String; 0]);
        assert_eq!(
            Vec::from_iter(&result.includes),
            ["docs/a/one.md", "docs/b/two.md", "three.md"]
        );
    }

    #[test]
    fn a_file_that_includes_itself_renders_the_line_as_written() {
        let files = Files::new([
            ("docs/self.md", "Self.\n\n::include[self.md]\n"),
            ("docs/a.md", "A.\n\n::include[b.md]\n"),
            ("docs/b.md", "B.\n\n::include[c.md]\n"),
            ("docs/c.md", "C.\n\n::include[a.md]\n"),
        ]);
        for (markdown, html, warning) in [
            (
                "::include[guide.md]\n",
                "<p>::include[guide.md]</p>",
                "include `docs/guide.md` includes itself",
            ),
            (
                "::include[self.md]\n",
                "<p>Self.</p><p>::include[self.md]</p>",
                "docs/self.md: include `docs/self.md` includes itself",
            ),
            (
                "::include[a.md]\n",
                "<p>A.</p><p>B.</p><p>C.</p><p>::include[a.md]</p>",
                "docs/c.md: include `docs/a.md` includes itself through `docs/b.md`, `docs/c.md`",
            ),
            (
                "::include[b.md]\n",
                "<p>B.</p><p>C.</p><p>A.</p><p>::include[b.md]</p>",
                "docs/a.md: include `docs/b.md` includes itself through `docs/c.md`, `docs/a.md`",
            ),
        ] {
            let result = files.render(markdown);
            assert_eq!(result.html, html);
            assert_eq!(result.warnings, [warning]);
        }
        let result = Files::new([
            ("docs/a.md", "::include[b.md]\n"),
            ("docs/b.md", "::include[a.md]\n"),
        ])
        .render("::include[a.md]\n");
        assert_eq!(
            result.warnings,
            ["docs/b.md: include `docs/a.md` includes itself through `docs/b.md`"]
        );
    }

    #[test]
    fn a_file_included_twice_renders_twice_and_is_read_once() {
        let files = Files::new([
            ("docs/a.md", "::include[b.md]\n\n::include[c.md]\n"),
            ("docs/b.md", "B.\n\n::include[d.md]\n"),
            ("docs/c.md", "C.\n\n::include[d.md]\n\n::include[gone.md]\n"),
            ("docs/d.md", "D.\n"),
        ]);
        let result = files.render("::include[a.md]\n\n::include[gone.md]\n");
        assert_eq!(
            result.html,
            "<p>B.</p><p>D.</p><p>C.</p><p>D.</p><p>::include[gone.md]</p><p>::include[gone.md]</p>"
        );
        assert_eq!(
            result.warnings,
            [
                "docs/c.md: include `docs/gone.md` not found",
                "include `docs/gone.md` not found"
            ]
        );
        assert_eq!(
            files.reads(),
            [
                "docs/a.md",
                "docs/b.md",
                "docs/d.md",
                "docs/c.md",
                "docs/gone.md"
            ]
        );
    }

    #[test]
    fn a_page_renders_at_most_64_includes() {
        let fan = "::include[leaf.md]\n\n".repeat(70);
        let files = Files::new([("docs/fan.md", fan.as_str()), ("docs/leaf.md", "L.\n")]);
        let result = files.render("::include[fan.md]\n");
        assert_eq!(result.html.matches("<p>L.</p>").count(), 63);
        assert_eq!(result.html.matches("<p>::include[leaf.md]</p>").count(), 7);
        let warning = "docs/fan.md: include `docs/leaf.md` is past the page's limit of 64 includes";
        assert_eq!(result.warnings, [warning; 7]);
        assert_eq!(files.reads(), ["docs/fan.md", "docs/leaf.md"]);
    }

    #[test]
    fn a_page_renders_at_most_32_mib_of_included_text() {
        let half = "a".repeat(16 * 1024 * 1024);
        let files = Files::new([("docs/half.md", half.as_str()), ("docs/x.md", "x")]);
        let result = files.render("::include[half.md]\n\n::include[half.md]\n\n::include[x.md]\n");
        assert!(result.html.ends_with("<p>::include[x.md]</p>"));
        let warning = "include `docs/x.md` is past the page's limit of 32 MiB of included text";
        assert_eq!(result.warnings, [warning]);
    }

    #[test]
    fn a_file_past_the_byte_limit_is_read_once() {
        let half = "a".repeat(16 * 1024 * 1024);
        let big = "b".repeat(17 * 1024 * 1024);
        let files = Files::new([
            ("docs/half.md", half.as_str()),
            ("docs/big.md", big.as_str()),
            ("docs/x.md", "x"),
        ]);
        let result = files.render(
            "::include[half.md]\n\n::include[big.md]\n\n::include[big.md]\n\n::include[x.md]\n\n::include[half.md]\n",
        );
        assert!(result.html.ends_with(
            "<p>::include[big.md]</p><p>::include[big.md]</p><p>x</p><p>::include[half.md]</p>"
        ));
        let limit = "is past the page's limit of 32 MiB of included text";
        assert_eq!(
            result.warnings,
            [
                format!("include `docs/big.md` {limit}"),
                format!("include `docs/big.md` {limit}"),
                format!("include `docs/half.md` {limit}"),
            ]
        );
        assert_eq!(files.reads(), ["docs/half.md", "docs/big.md", "docs/x.md"]);
    }

    #[test]
    fn warnings_in_an_included_file_name_it() {
        let files = Files::new([
            ("docs/one.md", "::include[sub/two.md]\n\n::foo[]\n"),
            (
                "docs/sub/two.md",
                "::foo[]\n\n::foo[]\n\n::include[missing.md]\n",
            ),
        ]);
        let result = files.render("::foo[]\n\n::include[one.md]\n\n::foo[]\n");
        assert_eq!(
            result.warnings,
            [
                "unknown leaf directive '::foo'",
                "docs/sub/two.md: unknown leaf directive '::foo'",
                "docs/sub/two.md: include `docs/sub/missing.md` not found",
                "docs/one.md: unknown leaf directive '::foo'",
            ]
        );
    }

    #[test]
    fn diagram_warnings_name_the_included_file() {
        let renderer = renderer::<HtmlBackend>()
            .with_file_resolver(Echo)
            .with_diagram_languages(Arc::new(Diagrams));
        let pass = renderer.begin("::include[diagram.md]\n\n```plantuml {#flow}\nA\n```\n");
        let failed = || {
            Err(DiagramError {
                message: "kroki said no".to_owned(),
                transient: false,
            })
        };
        let warned = Ok(Resolved {
            asset: Asset::Inline(DiagramContent::Svg("<svg></svg>".to_owned())),
            size: None,
            digest: "0".to_owned(),
            warnings: vec!["unknown attribute 'zz' ignored".to_owned()],
        });
        let result = pass.finish(&[failed(), warned, failed()]);
        assert_eq!(
            result.warnings,
            [
                "diagram 3: duplicate id 'flow' (already used by docs/diagram.md: diagram 1)",
                "docs/diagram.md: diagram 1: kroki said no",
                "docs/diagram.md: diagram 2: unknown attribute 'zz' ignored",
                "diagram 3: kroki said no",
            ]
        );
    }

    #[test]
    fn heading_and_tab_ids_stay_page_wide_through_nested_includes() {
        let tabs = "## Setup\n\n::::tabs\n:::tab[X]\nx\n:::\n::::\n";
        let files = Files::new([
            ("docs/one.md", format!("{tabs}\n::include[two.md]\n")),
            ("docs/two.md", tabs.to_owned()),
        ]);
        let result = files.render(&format!("{tabs}\n::include[one.md]\n"));
        let ids = Vec::from_iter(result.toc.iter().map(|entry| entry.id.as_str()));
        assert_eq!(ids, ["setup", "setup-1", "setup-2"]);
        for id in ["tab-0-0", "tab-1-1", "tab-2-2"] {
            assert!(
                result.html.contains(&format!(r#"id="{id}""#)),
                "{}",
                result.html
            );
        }
    }

    #[test]
    fn search_documents_hold_the_included_text() {
        let result = renderer::<SearchDocumentBackend>().render("Before\n\n::include[part.md]\n");
        assert_eq!(
            result.html.split_whitespace().collect::<Vec<_>>(),
            ["Before", "Part."]
        );
    }
}
