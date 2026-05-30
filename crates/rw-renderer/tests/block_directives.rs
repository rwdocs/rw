//! Stream-native leaf & container directive behavior, driven through the full
//! `MarkdownRenderer` pipeline.

use rw_renderer::directive::{
    DirectiveArgs, DirectiveContext, DirectiveOutput, DirectiveProcessor, LeafDirective,
};
use rw_renderer::{
    HtmlBackend, MarkdownRenderer, Pipeline, RenderResult, StatusDirective, TabsDirective,
};

fn render_tabs(md: &str) -> RenderResult {
    let directives = DirectiveProcessor::new().with_container(TabsDirective::new());
    MarkdownRenderer::<HtmlBackend>::new().render(md, Pipeline::new().with_directives(directives))
}

fn render_status(md: &str) -> RenderResult {
    let directives = DirectiveProcessor::new().with_inline(StatusDirective::new());
    MarkdownRenderer::<HtmlBackend>::new().render(md, Pipeline::new().with_directives(directives))
}

#[test]
fn container_with_block_body_renders() {
    let md = ":::tab[macOS]\n\nInstall with Homebrew.\n\n:::tab[Linux]\n\nInstall with apt.\n\n:::";
    let result = render_tabs(md);
    assert!(
        result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains("Install with Homebrew."),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains("Install with apt."),
        "got: {}",
        result.html
    );
    assert!(!result.html.contains("<p>:::"), "got: {}", result.html);
}

#[test]
fn bracketed_attr_delimiter_is_recognized() {
    let md = ":::tab[Label with spaces]\n\nBody.\n\n:::";
    let result = render_tabs(md);
    assert!(
        result.html.contains(">Label with spaces</button>"),
        "got: {}",
        result.html
    );
}

#[test]
fn status_inline_alone_on_a_line_still_expands() {
    let result = render_status(":status[Done]{color=green}");
    // The inline directive must expand even when it is alone on a line (i.e. it
    // must not be mistaken for / swallowed by block-directive deferral). The
    // `<rw-status>` marker is rewritten to `status status-green` in post-process,
    // so the post-processed class is the observable proof of expansion.
    assert!(result.html.contains("status-green"), "got: {}", result.html);
}

#[test]
fn delimiter_then_bold_commits_as_normal_paragraph() {
    let result = render_tabs(":::no and **bold**");
    assert!(result.html.contains("<p>"), "got: {}", result.html);
    assert!(
        result.html.contains("<strong>bold</strong>"),
        "got: {}",
        result.html
    );
}

#[test]
fn multi_tab_group_renders_and_warns_unclosed() {
    let md = ":::tab[A]\n\nContent A\n\n:::tab[B]\n\nContent B\n\n:::";
    let result = render_tabs(md);
    assert!(
        result.html.contains(r#"<div class="tabs" id="tabs-0">"#),
        "got: {}",
        result.html
    );
    assert!(result.html.contains(">A</button>"), "got: {}", result.html);
    assert!(result.html.contains(">B</button>"), "got: {}", result.html);
    assert!(result.html.contains("Content A"), "got: {}", result.html);
    assert!(result.html.contains("Content B"), "got: {}", result.html);
    assert!(!result.html.contains("<rw-tab"), "got: {}", result.html);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("unclosed") && w.contains("tab")),
        "got: {:?}",
        result.warnings,
    );
}

#[test]
fn tabs_html_has_no_stray_blank_runs() {
    let md = ":::tab[A]\n\nBody.\n\n:::";
    let result = render_tabs(md);
    // The tab panel sits flush against the body paragraph — the reparse-padding
    // whitespace is gone. Asserts the whitespace invariant (no stray newline
    // before the body <p>) without coupling to the panel's generated id.
    assert!(result.html.contains("<p>Body."), "got: {}", result.html);
    assert!(
        !result.html.contains("\n<p>Body."),
        "stray newline before tab body: {}",
        result.html,
    );
}

// --- A custom leaf that splices markdown in context (the `::include` shape) ---

struct IncludeFixed {
    body: String,
}
impl LeafDirective for IncludeFixed {
    fn name(&self) -> &'static str {
        "include"
    }
    fn process(&mut self, _args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
        DirectiveOutput::markdown(self.body.clone())
    }
}

struct IncludeSelf;
impl LeafDirective for IncludeSelf {
    fn name(&self) -> &'static str {
        "include"
    }
    fn process(&mut self, _args: DirectiveArgs, _ctx: &DirectiveContext) -> DirectiveOutput {
        DirectiveOutput::markdown("::include[self]".to_owned())
    }
}

fn render_with<D: LeafDirective + 'static>(md: &str, leaf: D) -> RenderResult {
    let directives = DirectiveProcessor::new()
        .with_leaf(leaf)
        .with_inline(StatusDirective::new());
    MarkdownRenderer::<HtmlBackend>::new().render(md, Pipeline::new().with_directives(directives))
}

#[test]
fn directive_inside_indented_code_stays_literal() {
    let result = render_tabs("    :::tab[X]\n    body\n    :::");
    assert!(result.html.contains("<code>"), "got: {}", result.html);
    assert!(result.html.contains(":::tab[X]"), "got: {}", result.html);
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn directive_inside_fenced_code_stays_literal() {
    let result = render_tabs("```\n:::tab[X]\n```");
    assert!(result.html.contains(":::tab[X]"), "got: {}", result.html);
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn container_inside_blockquote_is_recognized() {
    let md = "> :::tab[Q]\n>\n> Body.\n>\n> :::";
    let result = render_tabs(md);
    assert!(result.html.contains("<blockquote>"), "got: {}", result.html);
    assert!(
        result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn container_inside_loose_list_is_recognized() {
    let md = "- item\n\n  :::tab[L]\n\n  Body.\n\n  :::";
    let result = render_tabs(md);
    assert!(result.html.contains("<li>"), "got: {}", result.html);
    assert!(
        result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn unregistered_container_renders_literally_with_stray_close_warning() {
    let directives = DirectiveProcessor::new().with_inline(StatusDirective::new());
    let result = MarkdownRenderer::<HtmlBackend>::new().render(
        ":::foo[x]\n\nBody.\n\n:::",
        Pipeline::new().with_directives(directives),
    );
    assert!(
        result.html.contains("<p>:::foo[x]</p>"),
        "got: {}",
        result.html
    );
    assert!(result.html.contains("<p>:::</p>"), "got: {}", result.html);
    assert!(
        result.warnings.iter().any(|w| w.contains("stray")),
        "got: {:?}",
        result.warnings
    );
}

#[test]
fn frontmatter_directive_shaped_text_is_inert() {
    let md = "---\ntitle: x\nnote: ':::tab[oops]'\n---\n\nBody.";
    let result = render_tabs(md);
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn empty_container_body_has_no_stray_paragraph() {
    let md = ":::tab[Empty]\n\n:::";
    let result = render_tabs(md);
    assert!(
        result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
    assert!(!result.html.contains("<p></p>"), "got: {}", result.html);
}

#[test]
fn leaf_markdown_output_is_reparsed_in_context() {
    let result = render_with(
        "::include[x]",
        IncludeFixed {
            body: "## Included\n\nStatus: :status[Go]{color=green}".to_owned(),
        },
    );
    assert!(result.html.contains("status-green"), "got: {}", result.html);
    assert!(
        result.html.contains(r#"<h2 id="included">"#),
        "got: {}",
        result.html
    );
    assert!(
        result.toc.iter().any(|e| e.title == "Included"),
        "toc: {:?}",
        result.toc
    );
}

#[test]
fn leaf_markdown_output_ending_in_directive_does_not_corrupt() {
    // The included markdown's LAST block is itself a directive paragraph (a
    // container close), which exercises the `text_buffer` / paragraph-state
    // save/restore around the nested reparse. Both the included content and the
    // outer trailing paragraph must render intact — the nested `:::` close must
    // not leak paragraph state into the outer walker.
    let directives = DirectiveProcessor::new()
        .with_leaf(IncludeFixed {
            body: "Before the tabs.\n\n:::tab[T]\n\ntab body\n\n:::".to_owned(),
        })
        .with_container(TabsDirective::new());
    let result = MarkdownRenderer::<HtmlBackend>::new().render(
        "::include[x]\n\nAfter the include.",
        Pipeline::new().with_directives(directives),
    );
    // Included prose and the included container both rendered.
    assert!(
        result.html.contains("Before the tabs."),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
    assert!(result.html.contains("tab body"), "got: {}", result.html);
    // The outer trailing paragraph is intact, in its own <p>, and not duplicated.
    assert!(
        result.html.contains("<p>After the include.</p>"),
        "got: {}",
        result.html
    );
    assert_eq!(
        result.html.matches("After the include.").count(),
        1,
        "got: {}",
        result.html
    );
    // The included container closed cleanly — no leftover unclosed/stray warning.
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn leaf_markdown_recursion_is_depth_limited() {
    let result = render_with("::include[self]", IncludeSelf);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("Maximum include depth")),
        "got: {:?}",
        result.warnings,
    );
}

#[test]
fn delimiter_with_trailing_hard_break_commits_as_paragraph() {
    let md = ":::tab[X]  \nstill same paragraph";
    let result = render_tabs(md);
    assert!(result.html.contains("<p>"), "got: {}", result.html);
    assert!(
        !result.html.contains(r#"role="tablist""#),
        "got: {}",
        result.html
    );
}

#[test]
fn two_consecutive_normal_paragraphs_do_not_leak_state() {
    // Regression guard: pending_paragraph / paragraph_open must not leak across
    // paragraph boundaries.
    let result = render_tabs("para one\n\npara two");
    assert_eq!(
        result.html.matches("<p>").count(),
        2,
        "got: {}",
        result.html
    );
    assert_eq!(
        result.html.matches("</p>").count(),
        2,
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains("<p>para one</p>"),
        "got: {}",
        result.html
    );
    assert!(
        result.html.contains("<p>para two</p>"),
        "got: {}",
        result.html
    );
}

#[test]
fn directive_immediately_followed_by_paragraph() {
    // A container directly followed by ordinary content renders both correctly.
    let md = ":::tab[A]\n\nInside.\n\n:::\n\nAfter the tabs.";
    let result = render_tabs(md);
    assert!(result.html.contains("Inside."), "got: {}", result.html);
    assert!(
        result.html.contains("<p>After the tabs.</p>"),
        "got: {}",
        result.html
    );
    assert!(!result.html.contains("<p>:::"), "got: {}", result.html);
}

#[test]
fn nested_same_name_containers_render_balanced() {
    use rw_renderer::directive::ContainerDirective;
    struct Details {
        depth: usize,
    }
    impl ContainerDirective for Details {
        fn name(&self) -> &'static str {
            "details"
        }
        fn start(&mut self, _a: DirectiveArgs, _c: &DirectiveContext) -> DirectiveOutput {
            self.depth += 1;
            DirectiveOutput::html("<details>".to_owned())
        }
        fn end(&mut self, _line: usize) -> Option<String> {
            self.depth -= 1;
            Some("</details>".to_owned())
        }
    }
    let directives = DirectiveProcessor::new().with_container(Details { depth: 0 });
    let md = ":::details[Outer]\n\n:::details[Inner]\n\nx\n\n:::\n\n:::";
    let result = MarkdownRenderer::<HtmlBackend>::new()
        .render(md, Pipeline::new().with_directives(directives));
    assert_eq!(
        result.html.matches("</details>").count(),
        2,
        "got: {}",
        result.html
    );
    assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
}

#[test]
fn unclosed_container_warns() {
    let md = ":::tab[Open]\n\nbody";
    let result = render_tabs(md);
    assert!(
        result.warnings.iter().any(|w| w.contains("unclosed")),
        "got: {:?}",
        result.warnings
    );
}

#[test]
fn tab_label_is_html_escaped_and_quotes_stripped() {
    // Special characters in the label are HTML-escaped. (A label that is a
    // recognized HTML tag like `<script>` can't be used here: pulldown-cmark
    // consumes that whole line as an HTML block before the directive scanner
    // sees it. `<` / `&` that don't start an HTML block reach the directive
    // and are escaped on the way out.)
    let r1 = render_tabs(":::tab[a < b & c]\n\nx\n\n:::");
    assert!(r1.html.contains("a &lt; b &amp; c"), "got: {}", r1.html);
    // Surrounding quotes are stripped from the label.
    let r2 = render_tabs(":::tab[\"macOS и Linux\"]\n\nx\n\n:::");
    assert!(
        r2.html.contains(">macOS и Linux</button>"),
        "got: {}",
        r2.html
    );
}
