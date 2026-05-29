//! Per-render scratch state for [`MarkdownRenderer`](crate::MarkdownRenderer).
//!
//! `Walker` is constructed fresh inside every call to
//! [`MarkdownRenderer::render`](crate::MarkdownRenderer::render). That's how
//! we guarantee per-render state (`code_block_index`, heading accumulator
//! id-counts, "seen first H1" flag, scope stacks, buffers) starts empty —
//! the renderer's own scratch cannot leak across renders.
//!
//! Borrows the long-lived `RenderConfig` and (mutably) the processor and
//! directive extensions from the façade. Dropped on the way out — including
//! on panic, which leaves the façade's `RenderConfig` untouched for
//! subsequent renders.
//!
//! # Borrow discipline
//!
//! Two distinct borrow patterns are load-bearing inside `Walker` methods.
//! Both are explained in detail on the methods that use them:
//! pattern B (tightly-scoped reborrow) lives in [`Walker::flush_text`];
//! pattern A (field-disjoint borrows) lives in the `TagEnd::CodeBlock` arm
//! of [`Walker::end_tag`]. Don't "simplify" either pattern without reading
//! the comments first; both will fail to compile if hoisted.

use std::collections::HashMap;
use std::marker::PhantomData;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Tag, TagEnd};

use crate::backend::{AlertKind, RenderBackend};
use crate::code_block::{CodeBlockProcessor, ProcessResult, parse_fence_info};
use crate::config::RenderConfig;
use crate::directive::DirectiveOutput;
use crate::directive::DirectiveProcessor;
use crate::directive::parser::{ParsedDirective, parse_line};
use crate::link;
use crate::renderer::RenderResult;
use crate::scope::Scope;
use crate::table::TableState;
use crate::toc::HeadingAccumulator;
use crate::util::heading_level_to_num;
use crate::wikilink::{self, WikilinkResolution};

pub(crate) struct Walker<'r, B: RenderBackend> {
    cfg: &'r RenderConfig,
    processors: &'r mut [Box<dyn CodeBlockProcessor>],
    directives: Option<&'r mut DirectiveProcessor>,
    output: String,
    list_stack: Vec<bool>,
    table: TableState,
    heading: HeadingAccumulator,
    alert_stack: Vec<Option<AlertKind>>,
    code_block_index: usize,
    skip_wikilink_text: bool,
    text_buffer: String,
    scopes: Vec<Scope>,
    _backend: PhantomData<B>,
}

impl<'r, B: RenderBackend> Walker<'r, B> {
    /// Construct a fresh walker. Per-render state starts empty;
    /// `HeadingAccumulator` is built from the config's `extract_title` flag
    /// and the backend's `TITLE_AS_METADATA` constant. `output` is pre-allocated
    /// at 4 KiB to give average-sized documents a warm start.
    pub(crate) fn new(
        cfg: &'r RenderConfig,
        processors: &'r mut [Box<dyn CodeBlockProcessor>],
        directives: Option<&'r mut DirectiveProcessor>,
    ) -> Self {
        Self {
            cfg,
            processors,
            directives,
            // 4 KiB warm-start capacity for the output buffer — average-
            // page-sized documents fit without reallocating. A capacity-hint
            // API on the façade could carry per-call sizing (e.g., based on
            // the previous render's final size) but is out of scope here.
            output: String::with_capacity(4096),
            list_stack: Vec::new(),
            table: TableState::default(),
            heading: HeadingAccumulator::new(cfg.extract_title, B::TITLE_AS_METADATA),
            alert_stack: Vec::new(),
            code_block_index: 0,
            skip_wikilink_text: false,
            text_buffer: String::new(),
            scopes: Vec::new(),
            _backend: PhantomData,
        }
    }

    /// Consume the walker and produce the final `RenderResult`.
    ///
    /// Order is load-bearing:
    ///
    /// 1. `mem::take` `output` into a local `html` — this owned `String`
    ///    will be moved into the returned `RenderResult`, so it must be
    ///    freestanding (not a borrow into `self`).
    /// 2. Iterate `self.processors` mutably and call
    ///    `processor.post_process(&mut html)` on each — replaces deferred
    ///    code-block placeholders with rendered output.
    /// 3. Collect code-block processor warnings (directive warnings are
    ///    collected by the façade in `render`).
    /// 4. Take title and toc from the heading accumulator into the
    ///    `RenderResult` struct literal.
    pub(crate) fn finish(mut self) -> RenderResult {
        debug_assert!(
            self.scopes.is_empty(),
            "Walker::finish called with unclosed scopes (malformed event stream): {} scopes still open",
            self.scopes.len()
        );
        debug_assert!(
            self.list_stack.is_empty(),
            "Walker::finish called with unclosed list nesting: {} levels still open",
            self.list_stack.len()
        );
        debug_assert!(
            self.alert_stack.is_empty(),
            "Walker::finish called with unclosed blockquote/alert nesting: {} levels still open",
            self.alert_stack.len()
        );
        let mut html = std::mem::take(&mut self.output);
        for processor in self.processors.iter_mut() {
            processor.post_process(&mut html);
        }
        let warnings = self
            .processors
            .iter()
            .flat_map(|p| p.warnings())
            .cloned()
            .collect();
        RenderResult {
            html,
            title: self.heading.take_title(),
            toc: self.heading.take_toc(),
            warnings,
        }
    }

    pub(crate) fn process_event(&mut self, event: Event<'_>) {
        // Inline-directive expansion needs to see a full `:name[content]`
        // span, but pulldown-cmark splits text at delimiters like `[` and
        // `]`. We buffer adjacent `Event::Text` content outside code blocks
        // and metadata, and flush it through `flush_text` immediately
        // before processing any non-text event.
        let in_code_or_metadata = matches!(
            self.scopes.last(),
            Some(Scope::CodeBlock { .. } | Scope::Metadata)
        );
        let should_buffer =
            matches!(&event, Event::Text(_)) && !in_code_or_metadata && !self.skip_wikilink_text;

        if !should_buffer {
            self.flush_text_buffer();
        }

        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => {
                if self.skip_wikilink_text {
                    self.skip_wikilink_text = false;
                    return;
                }
                // Short-circuit before flush_text_buffer would otherwise run
                // the directive scanner (parse_line / dispatch_inline_named)
                // over YAML content, polluting result.warnings and firing
                // handler side effects. The Scope::Metadata arm of self.text
                // only suppresses the final B::text — by then the side
                // effects have already happened.
                if matches!(self.scopes.last(), Some(Scope::Metadata)) {
                    return;
                }
                let in_code = matches!(self.scopes.last(), Some(Scope::CodeBlock { .. }));
                if in_code {
                    self.text(&text);
                } else {
                    self.text_buffer.push_str(&text);
                }
            }
            Event::Code(code) => {
                self.inline_code(&code);
            }
            Event::Html(html) | Event::InlineHtml(html) => self.raw_html(&html),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.horizontal_rule(),
            Event::TaskListMarker(checked) => self.task_list_marker(checked),
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                // Not supported
            }
        }
    }

    /// Flush buffered text through inline-directive expansion (if any handlers
    /// are registered) and into the backend via [`text`](Self::text) /
    /// [`raw_html`](Self::raw_html).
    pub(crate) fn flush_text_buffer(&mut self) {
        if self.text_buffer.is_empty() {
            return;
        }
        let buf = std::mem::take(&mut self.text_buffer);
        self.flush_text(&buf);
    }

    fn flush_text(&mut self, text: &str) {
        if self.directives.is_none() {
            self.text(text);
            return;
        }

        let mut remaining = text;
        while !remaining.is_empty() {
            let Some((directive, start, end)) = parse_line(remaining) else {
                self.text(remaining);
                return;
            };

            if start > 0 {
                self.text(&remaining[..start]);
            }

            let matched = &remaining[start..end];

            // Tightly-scoped processor borrow: dispatch and capture the name
            // before relinquishing the borrow so we can call self.text /
            // self.raw_html below.
            //
            // Borrow discipline (pattern B): release the `&mut self.directives`
            // reborrow at the end of this block before any `&mut self` method
            // call (self.raw_html / self.text) below. The compiler can't prove
            // raw_html doesn't touch self.directives, so holding the directives
            // borrow across the call would fail. The outcome must be owned data,
            // not a borrow.
            let outcome: Option<(DirectiveOutput, String)> = match directive {
                ParsedDirective::Inline { name, args } => {
                    let processor = self
                        .directives
                        .as_deref_mut()
                        .expect("checked above: directives is Some");
                    let output = processor.dispatch_inline_named(&name, args);
                    Some((output, name))
                }
                // Block-level directives shouldn't reach here (the line
                // preprocessor consumed them), but defensively pass them
                // through verbatim.
                _ => None,
            };

            match outcome {
                Some((DirectiveOutput::Html(html), _)) => {
                    self.raw_html(&html);
                }
                Some((DirectiveOutput::Marker { open, body, close }, _)) => {
                    self.raw_html(&open);
                    self.text(&body);
                    self.raw_html(&close);
                }
                Some((DirectiveOutput::Markdown(md), name)) => {
                    if let Some(p) = self.directives.as_deref_mut() {
                        p.push_warning(format!(
                            "inline directive ':{name}' returned Markdown; emitted as raw HTML (re-parsing of inline-directive Markdown output is not supported)"
                        ));
                    }
                    self.raw_html(&md);
                }
                Some((DirectiveOutput::Skip, name)) => {
                    if let Some(p) = self.directives.as_deref_mut() {
                        p.push_warning(format!(
                            "unknown inline directive ':{name}' — no handler registered (or handler returned Skip)"
                        ));
                    }
                    self.text(matched);
                }
                None => {
                    self.text(matched);
                }
            }

            remaining = &remaining[end..];
        }
    }

    #[allow(clippy::too_many_lines)]
    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                if !matches!(self.scopes.last(), Some(Scope::CodeBlock { .. })) {
                    B::paragraph_start(&mut self.output);
                }
            }
            Tag::Heading { level, .. } => {
                let level_num = heading_level_to_num(level);
                // Decide once, at start: is_skipped_title flips to false
                // after the first H1 closes, so TagEnd::Heading would get a
                // different answer and skip nothing (or skip the wrong
                // heading) if we re-consulted.
                let in_first_h1 = self.heading.is_skipped_title(level_num);
                self.scopes.push(Scope::Heading {
                    level: level_num,
                    in_first_h1,
                    toc_text: String::new(),
                    rendered_html: String::new(),
                });
            }
            Tag::BlockQuote(kind) => {
                if let Some(bq_kind) = kind {
                    let alert_kind = AlertKind::from(bq_kind);
                    self.alert_stack.push(Some(alert_kind));
                    B::alert_start(alert_kind, &mut self.output);
                } else {
                    self.alert_stack.push(None);
                    B::blockquote_start(&mut self.output);
                }
            }
            Tag::CodeBlock(kind) => {
                let (language, attrs) = match kind {
                    CodeBlockKind::Fenced(ref info) if !info.is_empty() => {
                        let (lang, attrs) = parse_fence_info(info);
                        (if lang.is_empty() { None } else { Some(lang) }, attrs)
                    }
                    _ => (None, HashMap::new()),
                };
                self.scopes.push(Scope::CodeBlock {
                    language,
                    buffer: String::new(),
                    attrs,
                });
            }
            Tag::List(start) => {
                self.list_stack.push(start.is_some());
                B::list_start(start.is_some(), start, &mut self.output);
            }
            Tag::Item => {
                B::list_item_start(&mut self.output);
            }
            Tag::FootnoteDefinition(_) | Tag::HtmlBlock => {}
            Tag::MetadataBlock(_) => {
                self.scopes.push(Scope::Metadata);
            }
            Tag::DefinitionList => {
                B::definition_list_start(&mut self.output);
            }
            Tag::DefinitionListTitle => {
                B::definition_title_start(&mut self.output);
            }
            Tag::DefinitionListDefinition => {
                B::definition_detail_start(&mut self.output);
            }
            Tag::Table(alignments) => {
                self.table.start(alignments);
                B::table_start(&mut self.output);
            }
            Tag::TableHead => {
                self.table.start_head();
                B::table_head_start(&mut self.output);
            }
            Tag::TableRow => {
                self.table.start_row();
                B::table_row_start(&mut self.output);
            }
            Tag::TableCell => {
                let alignment = self.table.current_alignment();
                let is_head = self.table.is_in_head();
                B::table_cell_start(is_head, alignment, &mut self.output);
            }
            Tag::Emphasis => {
                self.with_markup_buffer(B::emphasis_start);
            }
            Tag::Strong => {
                self.with_markup_buffer(B::strong_start);
            }
            Tag::Strikethrough => {
                self.with_markup_buffer(B::strikethrough_start);
            }
            Tag::Link {
                link_type: LinkType::WikiLink { has_pothole },
                dest_url,
                ..
            } if self.cfg.wikilinks => {
                let resolution = wikilink::resolve(self.cfg, &dest_url);
                match &resolution {
                    WikilinkResolution::Resolved {
                        href,
                        section_ref,
                        subpath,
                        ..
                    } => {
                        let section_attrs = (!section_ref.is_empty())
                            .then_some((section_ref.as_str(), subpath.as_str()));
                        self.with_markup_buffer(|out| B::link_start(href, section_attrs, out));
                    }
                    WikilinkResolution::Fragment(fragment) => {
                        let href = format!("#{fragment}");
                        self.with_markup_buffer(|out| B::link_start(&href, None, out));
                    }
                    WikilinkResolution::Broken { .. } => {
                        self.with_markup_buffer(B::broken_link_start);
                    }
                }
                if !has_pothole {
                    let display = wikilink::display_text(self.cfg, &resolution);
                    self.skip_wikilink_text = true;
                    self.text(&display);
                }
            }
            Tag::Link { dest_url, .. } => {
                let dest_url = link::strip_origin(self.cfg, &dest_url);
                let href = B::transform_link(&dest_url, self.cfg.base_path.as_deref());
                let section_ref = link::section_ref_attrs(self.cfg, &href);
                let section_attrs = section_ref.as_ref().map(|(r, p)| (r.as_str(), p.as_str()));
                self.with_markup_buffer(|out| B::link_start(&href, section_attrs, out));
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                let dest_url = link::strip_origin(self.cfg, &dest_url).into_owned();
                self.scopes.push(Scope::Image {
                    alt_text: String::new(),
                    dest_url,
                    title: title.to_string(),
                });
            }
            Tag::Superscript => {
                self.with_markup_buffer(B::superscript_start);
            }
            Tag::Subscript => {
                self.with_markup_buffer(B::subscript_start);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if !matches!(self.scopes.last(), Some(Scope::CodeBlock { .. })) {
                    B::paragraph_end(&mut self.output);
                }
            }
            TagEnd::Heading(_level) => {
                if !matches!(self.scopes.last(), Some(Scope::Heading { .. })) {
                    debug_assert!(false, "TagEnd::Heading without matching Scope::Heading");
                    return;
                }
                let Some(Scope::Heading {
                    level,
                    in_first_h1,
                    toc_text,
                    rendered_html,
                }) = self.scopes.pop()
                else {
                    unreachable!("peeked match guarantees pop returns Some(Scope::Heading)");
                };
                if in_first_h1 {
                    self.heading.complete_first_h1(&toc_text);
                } else {
                    let done = self
                        .heading
                        .complete_heading(level, &toc_text, rendered_html);
                    B::heading_start(done.adjusted_level, &done.id, &mut self.output);
                    self.output.push_str(done.rendered_html.trim());
                    B::heading_end(done.adjusted_level, &mut self.output);
                }
            }
            TagEnd::BlockQuote(_) => match self.alert_stack.pop() {
                Some(Some(alert_kind)) => {
                    B::alert_end(alert_kind, &mut self.output);
                }
                _ => {
                    B::blockquote_end(&mut self.output);
                }
            },
            TagEnd::CodeBlock => {
                if !matches!(self.scopes.last(), Some(Scope::CodeBlock { .. })) {
                    debug_assert!(false, "TagEnd::CodeBlock without matching Scope::CodeBlock");
                    return;
                }
                let Some(Scope::CodeBlock {
                    language,
                    buffer,
                    attrs,
                }) = self.scopes.pop()
                else {
                    unreachable!("peeked match guarantees pop returns Some(Scope::CodeBlock)");
                };
                let index = self.code_block_index;
                self.code_block_index += 1;

                // Borrow discipline (pattern A): field-disjoint borrows.
                // `self.processors` (mutably via iter_mut) and `self.output` (mutably
                // via push_str inside the closure) are distinct fields of Walker, so
                // NLL splits the borrow per field. This works because the closure body
                // directly names both fields — NLL sees them as disjoint. If the
                // push_str calls were wrapped in a helper (e.g. `self.emit_html(...)`)
                // that took `&mut self`, the whole-struct reborrow would prevent the
                // concurrent iter_mut borrow on self.processors.
                let processed = language.as_deref().is_some_and(|lang_str| {
                    self.processors.iter_mut().any(|processor| {
                        match processor.process(lang_str, &attrs, &buffer, index) {
                            ProcessResult::Placeholder(placeholder) => {
                                self.output.push_str(&placeholder);
                                true
                            }
                            ProcessResult::Inline(html) => {
                                self.output.push_str(&html);
                                true
                            }
                            ProcessResult::PassThrough => false,
                        }
                    })
                });

                if !processed {
                    B::code_block(language.as_deref(), &buffer, &mut self.output);
                }
            }
            TagEnd::List(ordered) => {
                self.list_stack.pop();
                B::list_end(ordered, &mut self.output);
            }
            TagEnd::Item => {
                B::list_item_end(&mut self.output);
            }
            TagEnd::FootnoteDefinition | TagEnd::HtmlBlock => {}
            TagEnd::MetadataBlock(_) => {
                if !matches!(self.scopes.last(), Some(Scope::Metadata)) {
                    debug_assert!(
                        false,
                        "TagEnd::MetadataBlock without matching Scope::Metadata"
                    );
                    return;
                }
                self.scopes.pop();
            }
            TagEnd::Image => {
                if !matches!(self.scopes.last(), Some(Scope::Image { .. })) {
                    debug_assert!(false, "TagEnd::Image without matching Scope::Image");
                    return;
                }
                let Some(Scope::Image {
                    alt_text,
                    dest_url,
                    title,
                }) = self.scopes.pop()
                else {
                    unreachable!("peeked match guarantees pop returns Some(Scope::Image)");
                };
                // Pop BEFORE emit: the image's own scope must not intercept
                // its own B::image call — the emit needs to resolve against
                // the parent.
                self.with_markup_buffer(|out| B::image(&dest_url, &alt_text, &title, out));
            }
            TagEnd::DefinitionList => {
                B::definition_list_end(&mut self.output);
            }
            TagEnd::DefinitionListTitle => {
                B::definition_title_end(&mut self.output);
            }
            TagEnd::DefinitionListDefinition => {
                B::definition_detail_end(&mut self.output);
            }
            TagEnd::Table => {
                B::table_end(&mut self.output);
            }
            TagEnd::TableHead => {
                B::table_head_end(&mut self.output);
                self.table.end_head();
            }
            TagEnd::TableRow => {
                B::table_row_end(&mut self.output);
            }
            TagEnd::TableCell => {
                B::table_cell_end(self.table.is_in_head(), &mut self.output);
                self.table.next_cell();
            }
            TagEnd::Emphasis => {
                self.with_markup_buffer(B::emphasis_end);
            }
            TagEnd::Strong => {
                self.with_markup_buffer(B::strong_end);
            }
            TagEnd::Strikethrough => {
                self.with_markup_buffer(B::strikethrough_end);
            }
            TagEnd::Link => {
                self.with_markup_buffer(B::link_end);
            }
            TagEnd::Superscript => {
                self.with_markup_buffer(B::superscript_end);
            }
            TagEnd::Subscript => {
                self.with_markup_buffer(B::subscript_end);
            }
        }
    }

    fn text(&mut self, text: &str) {
        match self.scopes.last_mut() {
            Some(Scope::Heading {
                rendered_html,
                toc_text,
                ..
            }) => {
                toc_text.push_str(text);
                B::text(text, rendered_html);
            }
            Some(Scope::Image { alt_text, .. }) => alt_text.push_str(text),
            Some(Scope::CodeBlock { buffer, .. }) => buffer.push_str(text),
            Some(Scope::Metadata) => {}
            None => B::text(text, &mut self.output),
        }
    }

    fn inline_code(&mut self, code: &str) {
        match self.scopes.last_mut() {
            Some(Scope::Heading {
                rendered_html,
                toc_text,
                ..
            }) => {
                toc_text.push_str(code);
                B::inline_code(code, rendered_html);
            }
            // CommonMark: alt text is plain text — append code body without `<code>` wrap.
            Some(Scope::Image { alt_text, .. }) => alt_text.push_str(code),
            // Dormant: pulldown-cmark doesn't emit InlineCode inside a fenced code block.
            Some(Scope::CodeBlock { .. }) => {
                debug_assert!(false, "inline_code inside a fenced code block");
            }
            Some(Scope::Metadata) => {}
            None => B::inline_code(code, &mut self.output),
        }
    }

    fn raw_html(&mut self, html: &str) {
        match self.scopes.last_mut() {
            // toc_text intentionally NOT touched: raw HTML inside a heading
            // is rendered into the HTML body but does not contribute to the
            // TOC entry title or the slug id.
            Some(Scope::Heading { rendered_html, .. }) => B::raw_html(html, rendered_html),
            // CommonMark: alt text is plain text — raw HTML tags are suppressed
            // (their visible text comes through as Text events).
            // Metadata: suppressed entirely.
            Some(Scope::Image { .. } | Scope::Metadata) => {}
            // Dormant: pulldown-cmark doesn't emit Html/InlineHtml inside fenced code.
            Some(Scope::CodeBlock { .. }) => {
                debug_assert!(false, "raw_html inside a fenced code block");
            }
            None => B::raw_html(html, &mut self.output),
        }
    }

    fn soft_break(&mut self) {
        match self.scopes.last_mut() {
            Some(Scope::Heading { rendered_html, .. }) => B::soft_break(rendered_html),
            // Soft breaks inside alt text collapse to a single space (CommonMark plain-text rule).
            Some(Scope::Image { alt_text, .. }) => alt_text.push(' '),
            Some(Scope::CodeBlock { buffer, .. }) => buffer.push('\n'),
            Some(Scope::Metadata) => {}
            None => B::soft_break(&mut self.output),
        }
    }

    fn hard_break(&mut self) {
        match self.scopes.last_mut() {
            Some(Scope::Heading { rendered_html, .. }) => B::hard_break(rendered_html),
            // Hard breaks inside alt text collapse to a single space.
            Some(Scope::Image { alt_text, .. }) => alt_text.push(' '),
            // Dormant: pulldown-cmark doesn't emit HardBreak inside fenced code.
            Some(Scope::CodeBlock { .. }) => {
                debug_assert!(false, "hard_break inside a fenced code block");
            }
            Some(Scope::Metadata) => {}
            None => B::hard_break(&mut self.output),
        }
    }

    /// Run `f` against the buffer of the currently-active markup-accepting
    /// scope, or against `self.output` if no scope is active.
    /// `Image` / `Metadata` are no-ops — the closure is never invoked and
    /// the buffer is never exposed. `CodeBlock` is dormant
    /// (`debug_assert! + drop`).
    ///
    /// # Invariant
    ///
    /// **This is the only path from inline-markup events to a `&mut String`.**
    /// Do NOT add a `markup_buffer_mut() -> Option<&mut String>` accessor:
    /// the closure form is what makes "alt text drops markup" and
    /// "metadata block suppresses output" structural rather than convention.
    /// An accessor lets callers forget to no-op those scopes and silently
    /// leak markup into `<img alt="…">` or into the suppressed metadata
    /// block.
    fn with_markup_buffer(&mut self, f: impl FnOnce(&mut String)) {
        match self.scopes.last_mut() {
            Some(Scope::Heading { rendered_html, .. }) => f(rendered_html),
            Some(Scope::Image { .. } | Scope::Metadata) => {}
            Some(Scope::CodeBlock { .. }) => {
                debug_assert!(false, "markup-gated event inside a fenced code block");
            }
            None => f(&mut self.output),
        }
    }

    fn horizontal_rule(&mut self) {
        B::horizontal_rule(&mut self.output);
    }

    fn task_list_marker(&mut self, checked: bool) {
        B::task_list_marker(checked, &mut self.output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::HtmlBackend;

    fn cfg() -> RenderConfig {
        RenderConfig::new()
    }

    #[test]
    fn finish_returns_empty_result_when_no_events_processed() {
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());
        let result = walker.finish();
        assert!(result.html.is_empty());
        assert!(result.title.is_none());
        assert!(result.toc.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    #[should_panic(expected = "unclosed scopes")]
    #[cfg(debug_assertions)]
    fn finish_debug_asserts_scopes_empty() {
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let mut walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());
        walker.scopes.push(Scope::Metadata);
        walker.finish();
    }

    #[test]
    #[should_panic(expected = "unclosed list nesting")]
    #[cfg(debug_assertions)]
    fn finish_debug_asserts_list_stack_empty() {
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let mut walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());
        walker.list_stack.push(false);
        walker.finish();
    }

    #[test]
    #[should_panic(expected = "unclosed blockquote/alert nesting")]
    #[cfg(debug_assertions)]
    fn finish_debug_asserts_alert_stack_empty() {
        let config = cfg();
        let mut processors: Vec<Box<dyn CodeBlockProcessor>> = Vec::new();
        let mut directives = None;
        let mut walker = Walker::<HtmlBackend>::new(&config, &mut processors, directives.as_mut());
        walker.alert_stack.push(None);
        walker.finish();
    }
}
