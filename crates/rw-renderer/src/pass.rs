//! Two-phase rendering: walk the document, resolve its diagrams somewhere
//! else, then fill and assemble.
//!
//! The split keeps diagram I/O out of the renderer, and a diagram's markup
//! with the backend rather than the provider.

use std::collections::HashMap;
use std::marker::PhantomData;

use rw_diagrams::{Asset, DiagramContent, DiagramError, DiagramRequest, DiagramSource, Resolved};

use crate::backend::RenderBackend;
use crate::config::RenderConfig;
use crate::diagram::{DiagramView, scan_svg_links};
use crate::renderer::RenderResult;
use crate::walker::Paused;

/// A walked document whose diagrams are not resolved yet.
///
/// Exposes no rendered output, so a caller cannot skip
/// [`finish`](Self::finish) and leave its holes unfilled.
pub struct RenderPass<'a, B: RenderBackend> {
    cfg: &'a RenderConfig,
    paused: Paused,
    _backend: PhantomData<B>,
}

impl<'a, B: RenderBackend> RenderPass<'a, B> {
    pub(crate) fn new(cfg: &'a RenderConfig, paused: Paused) -> Self {
        Self {
            cfg,
            paused,
            _backend: PhantomData,
        }
    }

    /// Every diagram in the document, fence or file image, in document order.
    ///
    /// Empty unless the renderer was built with
    /// [`with_diagram_languages`](crate::MarkdownRenderer::with_diagram_languages).
    #[must_use]
    pub fn requests(&self) -> &[DiagramRequest] {
        &self.paused.diagrams
    }

    /// Fill each diagram from `resolutions`, which answer
    /// [`requests`](Self::requests) by position, and assemble the page.
    ///
    /// A request past the end of `resolutions` is not an error: a fence falls
    /// back to [`RenderBackend::code_block`], which is how a diagram fence
    /// renders when no diagram service is configured, and a file image to a
    /// paragraph of its alt text.
    #[must_use]
    pub fn finish(self, resolutions: &[Result<Resolved, DiagramError>]) -> RenderResult {
        let Self {
            cfg,
            mut paused,
            _backend,
        } = self;

        let mut warnings = Vec::new();
        let ids = if B::DIAGRAM_IDS {
            resolve_ids(&paused.diagrams, &mut warnings)
        } else {
            Vec::new()
        };

        // `zip` would silently drop the diagrams past a missing hole.
        debug_assert_eq!(paused.diagrams.len(), paused.diagram_holes.len());
        for (pos, (request, (slot, alt))) in paused
            .diagrams
            .iter()
            .zip(&paused.diagram_holes)
            .enumerate()
        {
            let id = ids.get(pos).map(String::as_str);
            let number = pos + 1;

            let mut markup = String::new();
            match resolutions.get(pos) {
                Some(Ok(resolved)) => {
                    // Only an inline SVG has links to find: a raster asset has
                    // no `<a>` tags, and a reference is a name, not content.
                    let links = match (&resolved.asset, cfg.sections.as_deref()) {
                        (Asset::Inline(DiagramContent::Svg(svg)), Some(sections)) => {
                            scan_svg_links(svg, sections)
                        }
                        _ => Vec::new(),
                    };
                    B::diagram(
                        &DiagramView {
                            id,
                            asset: &resolved.asset,
                            size: resolved.size,
                            links: &links,
                            alt: alt.as_deref(),
                        },
                        &mut markup,
                    );
                    warnings.extend(
                        resolved
                            .warnings
                            .iter()
                            .map(|warning| format!("diagram {number}: {warning}")),
                    );
                    paused
                        .section_refs
                        .extend(links.into_iter().map(|link| link.section_ref));
                }
                Some(Err(error)) => {
                    let message = format!("diagram {number}: {error}");
                    B::diagram_error(id, &message, &mut markup);
                    warnings.push(message);
                }
                None => match &request.source {
                    DiagramSource::Inline(text) => {
                        B::code_block(Some(&request.language), text, &mut markup);
                    }
                    // The renderer never reads a diagram file, so its alt text stands in.
                    DiagramSource::File(_) => {
                        B::paragraph_start(&mut markup);
                        B::text(alt.as_deref().unwrap_or_default(), &mut markup);
                        B::paragraph_end(&mut markup);
                    }
                },
            }
            paused.holes.fill(*slot, markup);
        }

        paused.assemble::<B>(warnings)
    }
}

/// Resolve each diagram's id, warning on collisions.
///
/// Explicit `{#id}`, else `diagram-<n>` with n the zero-based position among
/// the page's diagrams, not among its code blocks.
fn resolve_ids(requests: &[DiagramRequest], warnings: &mut Vec<String>) -> Vec<String> {
    let ids: Vec<String> = requests
        .iter()
        .enumerate()
        .map(|(pos, request)| {
            request
                .id
                .clone()
                .unwrap_or_else(|| format!("diagram-{pos}"))
        })
        .collect();

    let mut seen: HashMap<&str, usize> = HashMap::with_capacity(ids.len());
    for (number, id) in (1..).zip(&ids) {
        if let Some(&first) = seen.get(id.as_str()) {
            warnings.push(format!(
                "diagram {number}: duplicate id '{id}' (already used by diagram {first})"
            ));
        } else {
            seen.insert(id, number);
        }
    }

    ids
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};
    use std::sync::Arc;

    use rw_diagrams::{
        DiagramError, DiagramProvider, DiagramRequest, DiagramRouter, DiagramSource, Providers,
        ResolveContext, Resolved, Size,
    };
    use rw_sections::{Namespace, Section, Sections};

    use crate::file::resolvers::{Echo, PngsInside};
    use crate::renderer::{MarkdownRenderer, RenderResult};
    use crate::search_document::SearchDocumentBackend;
    use crate::{Asset, DiagramContent, HtmlBackend, RenderBackend};

    fn domain(name: &str) -> (String, Section) {
        (
            format!("domains/{name}"),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: name.to_owned(),
            },
        )
    }

    /// A provider that claims one language and answers from the fence source,
    /// without touching a network.
    ///
    /// The source doubles as a script: a body of `fail` errors, a body of `png`
    /// resolves to raster bytes with a display size, and a body starting with
    /// `warn ` resolves but reports the rest of the line as a provider warning.
    /// Everything else comes back as an SVG echoing the source, so a test can
    /// see *which* fence filled *which* hole; a file comes back as an SVG
    /// echoing its path.
    struct Stub {
        language: &'static str,
    }

    impl Stub {
        fn router(language: &'static str) -> Arc<Providers> {
            Arc::new(
                Providers::empty().with(Arc::new(Self { language }) as Arc<dyn DiagramProvider>),
            )
        }
    }

    impl DiagramProvider for Stub {
        fn handles(&self, language: &str) -> bool {
            language == self.language
        }

        fn resolve(
            &self,
            requests: &[&DiagramRequest],
            _ctx: &ResolveContext<'_>,
        ) -> Vec<Result<Resolved, DiagramError>> {
            requests
                .iter()
                .map(|request| {
                    let source = match &request.source {
                        DiagramSource::Inline(source) => source.trim(),
                        DiagramSource::File(path) => &format!("file:{path}"),
                    };
                    if source == "fail" {
                        return Err(DiagramError {
                            message: "kroki said no".to_owned(),
                            transient: false,
                        });
                    }
                    // A raster asset is the one shape whose size a backend has
                    // to write out: an SVG carries its own dimensions.
                    if source == "png" {
                        return Ok(Resolved {
                            asset: Asset::Inline(DiagramContent::Png(Vec::from([0x89, b'P']))),
                            size: Some(Size {
                                width: 10,
                                height: 5,
                            }),
                            digest: "0".to_owned(),
                            warnings: Vec::new(),
                        });
                    }
                    let warnings = source
                        .strip_prefix("warn ")
                        .map(|rest| Vec::from([rest.to_owned()]))
                        .unwrap_or_default();
                    Ok(Resolved {
                        asset: Asset::Inline(DiagramContent::Svg(format!("<svg>{source}</svg>"))),
                        size: Some(Size {
                            width: 10,
                            height: 10,
                        }),
                        digest: "0".to_owned(),
                        warnings,
                    })
                })
                .collect()
        }
    }

    fn diagram_renderer<B: RenderBackend>(providers: &Arc<Providers>) -> MarkdownRenderer<B> {
        MarkdownRenderer::<B>::new()
            .with_diagram_languages(Arc::clone(providers) as Arc<dyn DiagramRouter>)
    }

    /// Walk `markdown`, resolve every request through `providers`, and finish —
    /// the whole two-phase round trip a real caller makes.
    fn render_with<B: RenderBackend>(
        renderer: &MarkdownRenderer<B>,
        markdown: &str,
        providers: &Providers,
    ) -> RenderResult {
        let pass = renderer.begin(markdown);
        let resolutions = providers.resolve(pass.requests(), &ResolveContext::default());
        pass.finish(&resolutions)
    }

    /// [`render_with`] on a fresh renderer routing its fences through `providers`.
    fn round_trip<B: RenderBackend>(markdown: &str, providers: &Arc<Providers>) -> RenderResult {
        render_with(&diagram_renderer::<B>(providers), markdown, providers)
    }

    #[test]
    fn a_diagram_render_returns_its_image_files() {
        let providers = Stub::router("plantuml");
        let renderer = diagram_renderer::<HtmlBackend>(&providers)
            .with_source_path("a.md")
            .with_file_resolver(PngsInside);
        let result = render_with(
            &renderer,
            "![x](b.png)\n\n```plantuml\nA -> B\n```\n",
            &providers,
        );
        assert!(result.html.contains("<svg>A -> B</svg>"), "{}", result.html);
        assert_eq!(result.files, BTreeSet::from(["b.png".to_owned()]));
    }

    #[test]
    fn requests_carry_each_diagram_fence_in_document_order() {
        let providers = Stub::router("plantuml");
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_diagram_languages(Arc::clone(&providers) as Arc<dyn DiagramRouter>);
        let pass = renderer.begin(
            "```plantuml {#first format=png}\nA -> B\n```\n\n```rust\nlet x = 1;\n```\n\n```plantuml\nC -> D\n```\n",
        );

        let requests = pass.requests();
        assert_eq!(requests.len(), 2, "the rust fence is not a diagram");
        assert_eq!(requests[0].language, "plantuml");
        assert_eq!(
            requests[0].source,
            DiagramSource::Inline("A -> B\n".to_owned())
        );
        assert_eq!(requests[0].id.as_deref(), Some("first"));
        assert_eq!(
            requests[0].attrs,
            [("format".to_owned(), "png".to_owned())],
            "attributes travel in the order the author wrote them",
        );
        assert_eq!(
            requests[1].source,
            DiagramSource::Inline("C -> D\n".to_owned())
        );
        assert_eq!(requests[1].id, None);
        assert_eq!(requests[1].attrs, [] as [(String, String); 0]);
    }

    /// Providers warn about keys they do not know, so `alt` must not reach
    /// one. Only the second diagram has alt text: a misaligned pairing would
    /// put it on a neighbour.
    #[test]
    fn alt_text_is_taken_from_the_request_and_labels_its_own_diagram() {
        let providers = Stub::router("plantuml");
        let renderer = diagram_renderer::<HtmlBackend>(&providers);
        let pass = renderer.begin(
            "```plantuml\nfirst\n```\n\n\
             ```plantuml {alt=\"Payment flow\" format=png}\nsecond\n```\n\n\
             ```plantuml\npng\n```\n",
        );

        assert_eq!(
            pass.requests()[1].attrs,
            [("format".to_owned(), "png".to_owned())]
        );

        let resolutions = providers.resolve(pass.requests(), &ResolveContext::default());
        let html = pass.finish(&resolutions).html;
        assert!(
            html.contains(r#"<figure class="diagram" data-diagram-id="diagram-0"><rw-diagram>"#)
                && html.contains(
                    r#"<figure class="diagram" data-diagram-id="diagram-1" aria-label="Payment flow"><rw-diagram>"#
                )
                && html.contains(r#" alt="diagram"></figure>"#),
            "{html}"
        );
    }

    /// An empty label names nothing, so the diagram keeps the generic one.
    #[test]
    fn an_empty_alt_is_no_alt() {
        let providers = Stub::router("plantuml");
        let renderer = diagram_renderer::<HtmlBackend>(&providers);
        let pass = renderer.begin("```plantuml {alt=\"\"}\npng\n```\n");
        assert_eq!(pass.requests()[0].attrs, [] as [(String, String); 0]);

        let resolutions = providers.resolve(pass.requests(), &ResolveContext::default());
        let html = pass.finish(&resolutions).html;
        assert!(html.contains(r#" alt="diagram"></figure>"#), "{html}");
    }

    /// The fence language reaches the provider as authored: canonicalizing
    /// `kroki-mermaid` is the provider's business, and a renderer that
    /// normalized it would hide aliases from the one component that defines them.
    #[test]
    fn a_request_carries_the_fence_language_as_authored() {
        let providers = Arc::new(Providers::empty().with(Arc::new(Stub {
            language: "kroki-mermaid",
        }) as Arc<dyn DiagramProvider>));
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_diagram_languages(Arc::clone(&providers) as Arc<dyn DiagramRouter>);
        let pass = renderer.begin("```kroki-mermaid\ngraph TD\n```\n");

        assert_eq!(pass.requests()[0].language, "kroki-mermaid");
    }

    #[test]
    fn an_unresolved_request_falls_through_to_a_code_block() {
        let result = diagram_renderer::<HtmlBackend>(&Stub::router("plantuml"))
            .render("```plantuml\nA -> B\n```\n");

        // Exactly what a diagram fence renders as with no service configured.
        let mut expected = String::new();
        HtmlBackend::code_block(Some("plantuml"), "A -> B\n", &mut expected);
        assert_eq!(result.html, expected);
        assert_eq!(result.warnings, [] as [String; 0]);
    }

    /// Provider and backend tests build their `DiagramView` by hand, so only
    /// this one sees a `finish` that drops [`Resolved::size`].
    #[test]
    fn a_resolved_size_reaches_the_backend() {
        let result =
            round_trip::<HtmlBackend>("```plantuml\npng\n```\n", &Stub::router("plantuml"));

        assert!(
            result.html.contains(r#" width="10" height="5""#),
            "the provider's size did not reach the markup: {}",
            result.html,
        );
    }

    #[test]
    fn a_failed_resolution_renders_an_error_figure_and_warns_with_its_text() {
        // Two code blocks first, one indented, so the number can only be the
        // diagram's position.
        let result = round_trip::<HtmlBackend>(
            "    indented\n\n```rust\nlet x = 1;\n```\n\n```plantuml\nfail\n```\n",
            &Stub::router("plantuml"),
        );

        assert!(
            result
                .html
                .contains("Diagram rendering failed: diagram 1: kroki said no"),
            "the error figure is missing or unnumbered: {}",
            result.html
        );
        assert_eq!(result.warnings, ["diagram 1: kroki said no"]);
    }

    #[test]
    fn provider_warnings_are_prefixed_with_the_diagram_position() {
        // Two code blocks first, one indented, so the prefix can only be the
        // diagram's position.
        let result = round_trip::<HtmlBackend>(
            "    indented\n\n```rust\nlet x = 1;\n```\n\n```plantuml\nwarn unknown attribute 'zz' ignored\n```\n",
            &Stub::router("plantuml"),
        );

        assert_eq!(
            result.warnings,
            ["diagram 1: unknown attribute 'zz' ignored"],
        );
    }

    #[test]
    fn diagram_warnings_precede_the_walk_s_own_warnings() {
        // `:nope` is an unknown inline directive: a warning the walk itself
        // raises, after the fence in document order.
        let result = round_trip::<HtmlBackend>(
            "```plantuml\nwarn from the provider\n```\n\n:nope[x]\n",
            &Stub::router("plantuml"),
        );

        assert_eq!(
            result.warnings,
            [
                "diagram 1: from the provider",
                "unknown inline directive ':nope'",
            ],
        );
    }

    #[test]
    fn auto_ids_number_by_diagram_position_not_code_block_index() {
        // The interleaved `rust` fence is what makes the two numberings differ:
        // the second diagram is code block 2 but diagram 1.
        let result = round_trip::<HtmlBackend>(
            "```plantuml\nfirst\n```\n\n```rust\nlet x = 1;\n```\n\n```plantuml\nsecond\n```\n",
            &Stub::router("plantuml"),
        );

        assert!(
            result.html.contains(r#"data-diagram-id="diagram-0""#)
                && result.html.contains(r#"data-diagram-id="diagram-1""#),
            "diagram ids must number sequentially over diagrams: {}",
            result.html
        );
        assert!(
            !result.html.contains(r#"data-diagram-id="diagram-2""#),
            "numbering leaked the code-block index: {}",
            result.html
        );
    }

    #[test]
    fn duplicate_explicit_ids_warn_once_naming_both_diagrams() {
        let result = round_trip::<HtmlBackend>(
            "```plantuml {#flow}\nfirst\n```\n\n```rust\nlet x = 1;\n```\n\n```plantuml {#flow}\nsecond\n```\n",
            &Stub::router("plantuml"),
        );

        assert_eq!(
            result.warnings,
            ["diagram 2: duplicate id 'flow' (already used by diagram 1)"],
        );
    }

    /// An explicit `{#diagram-1}` can collide with the id auto-numbering hands
    /// the *next* diagram, which is the collision an author cannot see coming.
    #[test]
    fn an_explicit_id_colliding_with_an_auto_id_warns() {
        let result = round_trip::<HtmlBackend>(
            "```plantuml {#diagram-1}\nfirst\n```\n\n```plantuml\nsecond\n```\n",
            &Stub::router("plantuml"),
        );

        assert_eq!(
            result.warnings,
            ["diagram 2: duplicate id 'diagram-1' (already used by diagram 1)"],
        );
    }

    #[test]
    fn a_backend_that_emits_no_ids_raises_no_duplicate_id_warnings() {
        let markdown = "```plantuml {#flow}\nfirst\n```\n\n```plantuml {#flow}\nsecond\n```\n";
        let providers = Stub::router("plantuml");

        // The same document through a backend that does write ids, so the test
        // cannot pass just because the fixture stopped colliding.
        assert_eq!(
            round_trip::<HtmlBackend>(markdown, &providers)
                .warnings
                .len(),
            1,
            "the fixture must actually contain a duplicate id",
        );
        assert!(
            round_trip::<SearchDocumentBackend>(markdown, &providers)
                .warnings
                .is_empty(),
            "a backend with DIAGRAM_IDS = false must not warn about ids it never writes",
        );
    }

    #[test]
    fn svg_links_that_resolve_become_section_refs_on_the_result() {
        let sections = Arc::new(Sections::new(HashMap::from([domain("billing")])));
        let providers = Stub::router("plantuml");
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_sections(sections)
            .with_diagram_languages(Arc::clone(&providers) as Arc<dyn DiagramRouter>);

        // The stub echoes its source into the SVG, so the fence body is how a
        // test gets an `<a href>` inside the rendered diagram.
        let pass = renderer.begin("```plantuml\n<a href=\"/domains/billing/api\">x</a>\n```\n");
        let resolutions = providers.resolve(pass.requests(), &ResolveContext::default());
        let result = pass.finish(&resolutions);

        assert!(
            result.section_refs.contains("domain:default/billing"),
            "the diagram's link did not reach section_refs: {:?}",
            result.section_refs
        );
        assert!(
            result
                .html
                .contains(r#"data-section-ref="domain:default/billing""#),
            "the link was not annotated in the markup: {}",
            result.html
        );
    }

    /// Prose links are collected during the walk and diagram links during
    /// `finish`, into the same set; `finish` must add to it, not replace it.
    #[test]
    fn diagram_links_are_added_to_the_prose_links_already_collected() {
        let sections = Arc::new(Sections::new(HashMap::from([
            domain("billing"),
            domain("shipping"),
        ])));
        let providers = Stub::router("plantuml");
        let renderer = MarkdownRenderer::<HtmlBackend>::new()
            .with_sections(sections)
            .with_diagram_languages(Arc::clone(&providers) as Arc<dyn DiagramRouter>);

        // The stub echoes its source into the SVG, so the fence body is the
        // diagram's link; the paragraph above it is the prose one.
        let pass = renderer.begin(
            "[prose](/domains/shipping/rates)\n\n\
             ```plantuml\n<a href=\"/domains/billing/api\">x</a>\n```\n",
        );
        let resolutions = providers.resolve(pass.requests(), &ResolveContext::default());
        let result = pass.finish(&resolutions);

        // Whole-set equality: `contains` on either ref alone passes on a
        // `finish` that threw the prose ref away.
        assert_eq!(
            result.section_refs,
            BTreeSet::from([
                "domain:default/billing".to_owned(),
                "domain:default/shipping".to_owned(),
            ]),
        );
    }

    #[test]
    fn a_diagram_inside_a_blockquote_fills_within_it() {
        let result = round_trip::<HtmlBackend>(
            "> quoted\n>\n> ```plantuml\n> A -> B\n> ```\n\nafter\n",
            &Stub::router("plantuml"),
        );

        let figure = result
            .html
            .find("<figure")
            .expect("figure missing from output");
        let close = result
            .html
            .find("</blockquote>")
            .expect("blockquote should close");
        assert!(
            figure < close,
            "diagram landed outside the blockquote: {}",
            result.html
        );
    }

    #[test]
    fn a_diagram_inside_a_list_item_fills_within_it() {
        let result = round_trip::<HtmlBackend>(
            "- item\n\n  ```plantuml\n  A -> B\n  ```\n\nafter\n",
            &Stub::router("plantuml"),
        );

        let figure = result
            .html
            .find("<figure")
            .expect("figure missing from output");
        let close = result.html.find("</li>").expect("list item should close");
        assert!(
            figure < close,
            "diagram landed outside the list item: {}",
            result.html
        );
    }

    #[test]
    fn no_router_configured_leaves_every_fence_a_code_block() {
        let renderer = MarkdownRenderer::<HtmlBackend>::new();
        let pass = renderer.begin("```plantuml\nA -> B\n```\n");

        assert!(
            pass.requests().is_empty(),
            "no router means no fence is a diagram",
        );

        let result = pass.finish(&[]);
        let mut expected = String::new();
        HtmlBackend::code_block(Some("plantuml"), "A -> B\n", &mut expected);
        assert_eq!(result.html, expected);
    }

    /// [`diagram_renderer`] for the page `guide/page.md`, placing every file.
    fn file_renderer<B: RenderBackend>(providers: &Arc<Providers>) -> MarkdownRenderer<B> {
        diagram_renderer::<B>(providers)
            .with_source_path("guide/page.md")
            .with_file_resolver(Echo)
    }

    fn file_round_trip(markdown: &str, providers: &Arc<Providers>) -> RenderResult {
        render_with(
            &file_renderer::<HtmlBackend>(providers),
            markdown,
            providers,
        )
    }

    fn assert_no_image_traces(result: &RenderResult) {
        assert_eq!(result.files, BTreeSet::new());
        assert_eq!(result.image_refusals, [] as [String; 0]);
    }

    #[test]
    fn a_lone_diagram_file_image_becomes_a_file_request() {
        let providers = Stub::router("plantuml");
        let renderer = file_renderer::<HtmlBackend>(&providers);
        for (src, path) in [
            ("diagrams/payment.puml", "guide/diagrams/payment.puml"),
            ("/flows/Payment.PUML?v=2", "flows/Payment.PUML"),
        ] {
            let pass = renderer.begin(&format!("![Payment flow]({src})\n"));
            assert_eq!(
                pass.requests(),
                [DiagramRequest {
                    language: "plantuml".to_owned(),
                    source: DiagramSource::File(path.to_owned()),
                    attrs: Vec::new(),
                    id: None,
                }],
            );

            let resolutions = providers.resolve(pass.requests(), &ResolveContext::default());
            let result = pass.finish(&resolutions);
            assert!(
                result.html.starts_with(
                    r#"<figure class="diagram" data-diagram-id="diagram-0" aria-label="Payment flow">"#
                ) && result.html.contains(&format!("<svg>file:{path}</svg>")),
                "{}",
                result.html
            );
            assert_eq!(result.warnings, [] as [String; 0]);
            assert_no_image_traces(&result);
        }
    }

    /// The stub answers a file with an SVG of its path, so a fence with that
    /// body is what the image should render as, wherever it sits.
    #[test]
    fn a_lone_diagram_file_image_renders_where_a_fence_would() {
        let fence = "```plantuml {alt=\"Flow\"}\nfile:guide/x.puml\n```";
        for (image, fenced) in [
            ("![Flow](x.puml)\n", format!("{fence}\n")),
            (
                "```plantuml\nfirst\n```\n\n![Flow](x.puml)\n",
                format!("```plantuml\nfirst\n```\n\n{fence}\n"),
            ),
            (
                "::::tabs\n:::tab[A]\n![Flow](x.puml)\n:::\n:::tab[B]\nb\n:::\n::::\n",
                format!("::::tabs\n:::tab[A]\n{fence}\n:::\n:::tab[B]\nb\n:::\n::::\n"),
            ),
            (
                "- item\n\n  ![Flow](x.puml)\n\n- next\n",
                format!("- item\n\n  {}\n\n- next\n", fence.replace('\n', "\n  ")),
            ),
            (
                "> quoted\n>\n> ![Flow](x.puml)\n",
                format!("> quoted\n>\n> {}\n", fence.replace('\n', "\n> ")),
            ),
        ] {
            let providers = Stub::router("plantuml");
            let from_image = file_round_trip(image, &providers);
            assert!(from_image.html.contains("<figure"), "{}", from_image.html);
            assert!(
                !from_image.html.contains("<p><figure"),
                "{}",
                from_image.html
            );
            assert_eq!(
                from_image.html,
                round_trip::<HtmlBackend>(&fenced, &providers).html,
                "{image}"
            );
            assert_eq!(from_image.warnings, [] as [String; 0]);
        }
    }

    #[test]
    fn a_diagram_file_image_not_alone_warns_and_renders_its_alt_text() {
        let providers = Stub::router("plantuml");
        // A files base shows whether the image was served as one.
        let renderer = file_renderer::<HtmlBackend>(&providers).with_files_base("/_api/files");
        for (image, alt) in [
            (
                "Here is the flow:\n![Payment flow](flow.puml)\n",
                "Here is the flow:\nPayment flow\n",
            ),
            ("# Flow ![chart](x.puml)\n", "# Flow chart\n"),
            (
                "[![chart](x.puml)](https://e.com)\n",
                "[chart](https://e.com)\n",
            ),
            ("- ![chart](x.puml)\n", "- chart\n"),
            ("| ![chart](x.puml) |\n|---|\n", "| chart |\n|---|\n"),
        ] {
            let result = render_with(&renderer, image, &providers);
            assert_eq!(result.html, renderer.render(alt).html, "{image}");
            let src = image.split_once("](").unwrap().1.split_once(')').unwrap().0;
            let warning = format!(
                "diagram `{src}` must be alone in its paragraph: put it on its own line with a blank line before and after"
            );
            assert_eq!(result.warnings, result.image_refusals);
            assert_eq!(result.image_refusals, [warning]);
            assert_eq!(result.files, BTreeSet::new());
        }
    }

    #[test]
    fn a_lone_diagram_file_without_a_router_for_it_is_its_alt_text() {
        let unrouted = MarkdownRenderer::<HtmlBackend>::new()
            .with_source_path("guide/page.md")
            .with_file_resolver(Echo);
        let mermaid_unrouted = file_renderer::<HtmlBackend>(&Stub::router("plantuml"));
        for (renderer, image, warning) in [
            (
                &unrouted,
                "![a < b & *c*](x.puml)\n",
                "diagram `x.puml` not rendered: no diagram renderer for plantuml is configured",
            ),
            (
                &mermaid_unrouted,
                "![a < b & *c*](x.mmd)\n",
                "diagram `x.mmd` not rendered: no diagram renderer for mermaid is configured",
            ),
        ] {
            let pass = renderer.begin(image);
            assert!(pass.requests().is_empty(), "{image}");
            let result = pass.finish(&[]);
            assert_eq!(result.html, "<p>a &lt; b &amp; c</p>", "{image}");
            assert_eq!(result.warnings, [warning]);
            assert_no_image_traces(&result);
        }
    }

    /// Only `render()` with a router leaves a file request unresolved: it
    /// renders as if no router had claimed it.
    #[test]
    fn an_unresolved_file_request_falls_through_to_its_alt_text() {
        let markdown = "![Flow](x.puml)\n\n![](y.puml)\n";
        let routed = file_renderer::<HtmlBackend>(&Stub::router("plantuml"));
        assert_eq!(routed.begin(markdown).requests().len(), 2);

        let result = routed.render(markdown);
        assert_eq!(result.html, "<p>Flow</p><p></p>");
        assert_eq!(result.warnings, [] as [String; 0]);
        let unrouted = MarkdownRenderer::<HtmlBackend>::new()
            .with_source_path("guide/page.md")
            .with_file_resolver(Echo);
        assert_eq!(result.html, unrouted.render(markdown).html);
    }

    #[test]
    fn diagram_files_are_not_image_files() {
        let result = file_round_trip(
            "![a](a.png)\n\n![d](d.puml)\n\n![e](e.gv)\n\n![m](m.mermaid)\n",
            &Stub::router("plantuml"),
        );
        assert_eq!(result.files, BTreeSet::from(["guide/a.png".to_owned()]));
        assert_eq!(
            result.warnings,
            [
                "diagram `e.gv` not rendered: no diagram renderer for graphviz is configured",
                "diagram `m.mermaid` not rendered: no diagram renderer for mermaid is configured",
            ]
        );
    }

    #[test]
    fn a_lone_image_that_is_no_diagram_renders_as_before() {
        let markdown = "![a](a.png)\n\n![b](b.puml.png)\n\n![c](https://e.com/c.puml)\n";
        let result = file_round_trip(markdown, &Stub::router("plantuml"));
        assert_eq!(
            result.html,
            "<p><img src=\"a.png\" alt=\"a\"></p>\
             <p><img src=\"b.puml.png\" alt=\"b\"></p>\
             <p><img src=\"https://e.com/c.puml\" alt=\"c\"></p>"
        );
        assert_eq!(
            result.html,
            MarkdownRenderer::<HtmlBackend>::new().render(markdown).html
        );
    }

    /// The resolver says which files a page may name: one that takes no
    /// diagram files refuses them like any other file it does not take.
    #[test]
    fn a_refused_diagram_file_is_a_refused_image() {
        let markdown = "![a](x.puml)\n\n![b](../../y.puml)\n";
        let renderer = diagram_renderer::<HtmlBackend>(&Stub::router("plantuml"))
            .with_source_path("guide/page.md")
            .with_file_resolver(PngsInside);
        let pass = renderer.begin(markdown);
        assert_eq!(pass.requests(), [] as [DiagramRequest; 0]);
        let result = pass.finish(&[]);
        assert_eq!(
            result.html,
            r#"<p><img src="x.puml" alt="a"></p><p><img src="../../y.puml" alt="b"></p>"#
        );
        let refusals = [
            "image `x.puml` is not a supported image type",
            "image `../../y.puml` is outside the project",
        ];
        assert_eq!(result.warnings, refusals);
        assert_eq!(result.image_refusals, refusals);
    }

    /// Two diagrams, so a numbering that carried over from the first render
    /// shows in the second.
    #[test]
    fn a_reused_renderer_renders_a_document_the_same_way_twice() {
        let md = "```rust\nlet x = 1;\n```\n\n```plantuml\nwarn first\n```\n\n```plantuml\nwarn second\n```";
        let providers = Stub::router("plantuml");
        let renderer = diagram_renderer::<HtmlBackend>(&providers);

        let r1 = render_with(&renderer, md, &providers);
        let r2 = render_with(&renderer, md, &providers);

        assert_eq!(r1.html, r2.html);
        assert_eq!(r1.warnings, ["diagram 1: first", "diagram 2: second"]);
        assert_eq!(r2.warnings, r1.warnings);
    }
}
