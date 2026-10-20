//! Two-phase rendering: walk the document, resolve its diagrams somewhere
//! else, then fill and assemble.
//!
//! Diagram bytes come from the network. Reaching out mid-walk would force an
//! HTTP client and a thread pool into the renderer's dependency graph, and
//! would make a diagram's *markup* the provider's job rather than the backend's.
//! Splitting the walk from the fill puts the I/O outside the renderer entirely —
//! the same way the renderer does not fetch `<img src>` bytes.

use std::collections::HashMap;
use std::marker::PhantomData;

use rw_diagrams::{Asset, DiagramContent, DiagramRequest, DiagramSource, Resolutions};

use crate::backend::RenderBackend;
use crate::config::RenderConfig;
use crate::diagram::{DiagramView, scan_svg_links};
use crate::fills::{GlobalKey, HoleKey, Source};
use crate::renderer::RenderResult;
use crate::walker::Paused;

/// A walked document whose diagrams are not resolved yet.
///
/// Deliberately exposes **no** way to read the rendered output: the markup is
/// unobtainable without [`finish`](Self::finish). That is the type-level form
/// of "a reserved hole must be filled": every other hole owner is internal to
/// the walk, so a caller cannot forget to fill this one, and forgetting it would
/// otherwise only trip a debug assert at assembly time.
pub struct RenderPass<'a, B: RenderBackend> {
    /// Borrowed from the renderer rather than cloned, so reaching [`Sections`]
    /// for diagram links costs no `Arc` clone per render.
    ///
    /// [`Sections`]: rw_sections::Sections
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

    /// Every diagram fence in the document, in document order.
    ///
    /// Empty unless the renderer was built with
    /// [`with_diagram_languages`](crate::MarkdownRenderer::with_diagram_languages).
    #[must_use]
    pub fn requests(&self) -> &[DiagramRequest] {
        &self.paused.diagrams
    }

    /// Fill each diagram from `resolutions` and assemble the page.
    ///
    /// A request with no entry in `resolutions` is not an error: its fence falls
    /// back to [`RenderBackend::code_block`], which is how a diagram fence
    /// renders when no diagram service is configured.
    #[must_use]
    pub fn finish(self, resolutions: &Resolutions) -> RenderResult {
        let Self {
            cfg,
            mut paused,
            _backend,
        } = self;

        let mut warnings = Vec::new();
        // A backend that emits no ids resolves none, so it also raises no
        // duplicate-id warnings about an attribute it never writes.
        let ids = if B::DIAGRAM_IDS {
            resolve_ids(&paused.diagrams, &mut warnings)
        } else {
            Vec::new()
        };

        for (pos, request) in paused.diagrams.iter().enumerate() {
            let block = HoleKey::from(request.key);
            let id = ids.get(pos).map(String::as_str);
            let number = pos + 1;

            let mut markup = String::new();
            match resolutions.get(&request.key) {
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
                        },
                        &mut markup,
                    );
                    // A provider cannot know a request's position on the page,
                    // and `RequestKey` is opaque, so attributing its warnings to
                    // a diagram is this caller's job.
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
                None => {
                    // The renderer never reads a diagram file, so its path is
                    // the only text it has to show.
                    let (DiagramSource::Inline(text) | DiagramSource::File(text)) = &request.source;
                    B::code_block(Some(&request.language), text, &mut markup);
                }
            }
            paused
                .fills
                .insert(GlobalKey(Source::Diagram, block), markup);
        }

        paused.assemble::<B>(warnings)
    }
}

/// Resolve each diagram's id, warning on collisions.
///
/// Explicit `{#id}`, else `diagram-<n>` where n is the diagram's zero-based
/// position among diagrams on the page — NOT its code-block index. A page that
/// interleaves prose fences would otherwise get sparse ids like `diagram-0`,
/// `diagram-3`, breaking the sequential numbering docs promise.
///
/// Auto ids are unique by construction, so the duplicate check catches
/// explicit-vs-explicit and explicit-vs-auto collisions.
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
        Resolutions, ResolveContext, Resolved, Size,
    };
    use rw_sections::{Namespace, Section, Sections};

    use crate::file::resolvers::PngsInside;
    use crate::renderer::{MarkdownRenderer, RenderResult};
    use crate::search_document::SearchDocumentBackend;
    use crate::{Asset, DiagramContent, HtmlBackend, RenderBackend};

    /// A provider that claims one language and answers from the fence source,
    /// without touching a network.
    ///
    /// The source doubles as a script: a body of `fail` errors, a body of `png`
    /// resolves to raster bytes with a display size, and a body starting with
    /// `warn ` resolves but reports the rest of the line as a provider warning.
    /// Everything else comes back as an SVG echoing the source, so a test can
    /// see *which* fence filled *which* hole.
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
                    let DiagramSource::Inline(source) = &request.source else {
                        panic!("the walker requests only fences");
                    };
                    let source = source.trim();
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
        assert_eq!(result.files, BTreeSet::from(["source:b.png".to_owned()]));
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

    /// `finish` is the only reader of [`Resolved::size`] there is: the provider
    /// that computes it and the backends that write it out are each tested
    /// against a `DiagramView` built by hand, so a `finish` that dropped the
    /// size would leave both ends green and publish every raster diagram at
    /// twice its intended size.
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
        let sections = Arc::new(Sections::new(HashMap::from([(
            "domains/billing".to_owned(),
            Section {
                kind: "domain".to_owned(),
                namespace: Namespace::default(),
                name: "billing".to_owned(),
            },
        )])));
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
    /// `finish`, into the same set. One document with both — pointing at
    /// different sections — is the only fixture that fails when `finish`
    /// replaces the walk's refs instead of adding to them.
    #[test]
    fn diagram_links_are_added_to_the_prose_links_already_collected() {
        let sections = Arc::new(Sections::new(HashMap::from([
            (
                "domains/billing".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    namespace: Namespace::default(),
                    name: "billing".to_owned(),
                },
            ),
            (
                "domains/shipping".to_owned(),
                Section {
                    kind: "domain".to_owned(),
                    namespace: Namespace::default(),
                    name: "shipping".to_owned(),
                },
            ),
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

        let result = pass.finish(&Resolutions::new());
        let mut expected = String::new();
        HtmlBackend::code_block(Some("plantuml"), "A -> B\n", &mut expected);
        assert_eq!(result.html, expected);
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
