//! Providers, and routing a batch of requests across them.

use std::sync::Arc;

use crate::request::{DiagramRequest, ResolveContext};
use crate::resolved::{DiagramError, Resolved};

/// Turns diagram source into rendered content.
///
/// A provider produces content and never markup: bytes, a size, a digest, and
/// warnings. How a diagram is referenced in HTML or Confluence storage format is
/// decided by whatever turns resolutions into markup, not by this trait.
///
/// Implementors are shared across threads and outlive any single render, so
/// per-page inputs arrive in [`ResolveContext`] rather than in the constructor.
pub trait DiagramProvider: Send + Sync {
    /// Whether this provider renders the given fence language.
    ///
    /// Called both to classify a fence before any request exists and to route an
    /// already-built batch, so the answer must depend on the language alone and
    /// not on any request state. Matching is the provider's business: it may
    /// accept prefixes or aliases.
    fn handles(&self, language: &str) -> bool;

    /// Render a batch.
    ///
    /// Batched so a provider can parallelise a fan-out or parse a shared model
    /// once. Implementors **must** return exactly one result per request, in the
    /// same order: callers match results to requests positionally.
    ///
    /// [`Providers::resolve`] tolerates violations rather than panicking, but
    /// neither direction is a usable outcome: an unanswered request becomes a
    /// non-transient error, and results beyond the request count are discarded.
    ///
    /// A failure is per-diagram: one bad fence yields one `Err` and the rest of
    /// the batch still resolves. Only requests this provider
    /// [`handles`](Self::handles) are passed in.
    fn resolve(
        &self,
        requests: &[&DiagramRequest],
        ctx: &ResolveContext<'_>,
    ) -> Vec<Result<Resolved, DiagramError>>;
}

/// The question a renderer asks about a fence language.
///
/// Narrower than [`DiagramProvider`] on purpose: a renderer holding only this
/// trait can decide whether a fence is a diagram while remaining structurally
/// unable to render one, which keeps resolution outside the render.
pub trait DiagramRouter: Send + Sync {
    /// Whether some provider renders this fence language.
    fn handles(&self, language: &str) -> bool;
}

/// One result per request, in the requests' order.
pub type Resolutions = Vec<Result<Resolved, DiagramError>>;

/// The providers configured for a site, and the routing between them.
#[derive(Clone, Default)]
pub struct Providers(Vec<Arc<dyn DiagramProvider>>);

impl Providers {
    /// No providers configured.
    ///
    /// [`resolve`](Self::resolve) then answers every request with an error,
    /// since no provider claims any language. Whether a fence falls back to a
    /// plain code block instead of becoming a request at all is the renderer's
    /// decision, taken through [`DiagramRouter::handles`].
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Add a provider, after any already added.
    ///
    /// Order is precedence: the first provider claiming a language gets it.
    #[must_use]
    pub fn with(mut self, provider: Arc<dyn DiagramProvider>) -> Self {
        self.0.push(provider);
        self
    }

    /// Resolve a batch, routing each request to the provider that claims it.
    ///
    /// Each provider with at least one matching request is called exactly once,
    /// with all of its requests in the caller's relative order; a provider none
    /// of the requests route to is not called at all.
    ///
    /// The result is total and positional: one entry per request, in order. A
    /// request no provider handles, and a request a provider declined to answer
    /// by returning too few results, both come back as a non-transient error.
    /// Results a provider returned beyond its request count are discarded.
    #[must_use]
    pub fn resolve(&self, requests: &[DiagramRequest], ctx: &ResolveContext<'_>) -> Resolutions {
        let routes: Vec<Option<usize>> = requests
            .iter()
            .map(|request| self.index_handling(&request.language))
            .collect();
        let mut answers: Vec<_> = (0..self.0.len())
            .map(|index| {
                let batch: Vec<&DiagramRequest> = requests
                    .iter()
                    .zip(&routes)
                    .filter_map(|(request, route)| (*route == Some(index)).then_some(request))
                    .collect();
                if batch.is_empty() {
                    Vec::new()
                } else {
                    self.0[index].resolve(&batch, ctx)
                }
                .into_iter()
            })
            .collect();

        requests
            .iter()
            .zip(routes)
            .map(|(request, route)| {
                let message = match route.map(|index| answers[index].next()) {
                    Some(Some(result)) => return result,
                    Some(None) => "diagram provider returned no result for language",
                    None => "no diagram provider handles language",
                };
                Err(DiagramError {
                    message: format!("{message} '{}'", request.language),
                    transient: false,
                })
            })
            .collect()
    }

    /// Index of the first provider claiming `language`.
    fn index_handling(&self, language: &str) -> Option<usize> {
        self.0.iter().position(|p| p.handles(language))
    }
}

impl DiagramRouter for Providers {
    fn handles(&self, language: &str) -> bool {
        self.index_handling(language).is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use super::{DiagramProvider, DiagramRouter, Providers};
    use crate::{
        Asset, DiagramContent, DiagramError, DiagramRequest, DiagramSource, ResolveContext,
        Resolved, Size,
    };

    /// A provider that claims one language and echoes each request's source back
    /// as SVG, tagged with its own language so a test can tell which instance
    /// answered even when two `Echo`s are asked about the same source, and
    /// recording the sources of every batch it was handed so a test can see how
    /// many calls arrived, what was in each, and in which order.
    struct Echo {
        language: &'static str,
        batches: Mutex<Vec<Vec<String>>>,
    }

    impl Echo {
        fn new(language: &'static str) -> Arc<Self> {
            Arc::new(Self {
                language,
                batches: Mutex::new(Vec::new()),
            })
        }

        fn batches(&self) -> Vec<Vec<String>> {
            self.batches.lock().clone()
        }
    }

    impl DiagramProvider for Echo {
        fn handles(&self, language: &str) -> bool {
            language == self.language
        }

        fn resolve(
            &self,
            requests: &[&DiagramRequest],
            _ctx: &ResolveContext<'_>,
        ) -> Vec<Result<Resolved, DiagramError>> {
            self.batches
                .lock()
                .push(requests.iter().map(|r| body(r).to_owned()).collect());
            requests
                .iter()
                .map(|r| {
                    Ok(Resolved {
                        asset: Asset::Inline(DiagramContent::Svg(format!(
                            "{}:{}",
                            self.language,
                            body(r)
                        ))),
                        // Non-trivial on purpose, so a test can tell a size
                        // that travelled through the round trip from a default.
                        size: Some(Size {
                            width: 10,
                            height: 10,
                        }),
                        digest: digest_of(body(r)),
                        warnings: Vec::new(),
                    })
                })
                .collect()
        }
    }

    /// Stands in for a real provider's content hash: lowercase hex, as
    /// [`Resolved::digest`] requires, and deterministic per source.
    fn digest_of(source: &str) -> String {
        format!("{:08x}", source.len())
    }

    fn request(language: &str, source: &str) -> DiagramRequest {
        DiagramRequest {
            language: language.to_owned(),
            source: DiagramSource::Inline(source.to_owned()),
            attrs: Vec::new(),
            id: None,
        }
    }

    fn body(request: &DiagramRequest) -> &str {
        match &request.source {
            DiagramSource::Inline(body) => body,
            DiagramSource::File(path) => panic!("expected a fence, got the file {path}"),
        }
    }

    fn svg_of(resolved: &Resolved) -> &str {
        match &resolved.asset {
            Asset::Inline(DiagramContent::Svg(svg)) => svg,
            other => panic!("expected inline SVG, got {other:?}"),
        }
    }

    #[test]
    fn routes_each_request_to_the_provider_that_claims_its_language() {
        let uml = Echo::new("plantuml");
        let mermaid = Echo::new("mermaid");
        let providers = Providers::empty()
            .with(Arc::clone(&uml) as Arc<dyn DiagramProvider>)
            .with(Arc::clone(&mermaid) as Arc<dyn DiagramProvider>);

        let requests = [
            request("plantuml", "A -> B"),
            request("mermaid", "graph TD"),
        ];
        let resolutions = providers.resolve(&requests, &ResolveContext::default());

        assert_eq!(resolutions.len(), 2);
        // Whole-value, not just the bytes: size, digest and warnings travel back
        // to the caller exactly as the provider set them.
        assert_eq!(
            resolutions[0].as_ref().expect("resolved"),
            &Resolved {
                asset: Asset::Inline(DiagramContent::Svg("plantuml:A -> B".to_owned())),
                size: Some(Size {
                    width: 10,
                    height: 10,
                }),
                digest: digest_of("A -> B"),
                warnings: Vec::new(),
            },
        );
        assert_eq!(
            svg_of(resolutions[1].as_ref().expect("resolved")),
            "mermaid:graph TD",
        );
    }

    #[test]
    fn hands_each_provider_its_whole_batch_in_one_call() {
        let uml = Echo::new("plantuml");
        let providers = Providers::empty().with(Arc::clone(&uml) as Arc<dyn DiagramProvider>);

        let requests = [
            request("plantuml", "one"),
            request("plantuml", "two"),
            request("plantuml", "three"),
        ];
        let resolutions = providers.resolve(&requests, &ResolveContext::default());

        assert_eq!(resolutions.len(), 3);
        assert_eq!(
            uml.batches(),
            [["one", "two", "three"]],
            "three requests for one provider must arrive as one call holding all three",
        );
    }

    #[test]
    fn keeps_each_providers_requests_in_the_callers_order() {
        // A provider answers positionally, so interleaved languages must not
        // scramble which result belongs to which request.
        let uml = Echo::new("plantuml");
        let mermaid = Echo::new("mermaid");
        let providers = Providers::empty()
            .with(Arc::clone(&uml) as Arc<dyn DiagramProvider>)
            .with(Arc::clone(&mermaid) as Arc<dyn DiagramProvider>);

        let requests = [
            request("plantuml", "uml-first"),
            request("mermaid", "mermaid-first"),
            request("plantuml", "uml-second"),
            request("mermaid", "mermaid-second"),
        ];
        let resolutions = providers.resolve(&requests, &ResolveContext::default());

        assert_eq!(
            uml.batches(),
            [["uml-first", "uml-second"]],
            "the provider must see its requests in the caller's relative order",
        );
        assert_eq!(
            mermaid.batches(),
            [["mermaid-first", "mermaid-second"]],
            "the provider must see its requests in the caller's relative order",
        );

        for (position, expected) in [
            (0, "plantuml:uml-first"),
            (1, "mermaid:mermaid-first"),
            (2, "plantuml:uml-second"),
            (3, "mermaid:mermaid-second"),
        ] {
            assert_eq!(
                svg_of(resolutions[position].as_ref().expect("resolved")),
                expected,
                "request {position} got the wrong provider's result",
            );
        }
    }

    #[test]
    fn the_first_provider_that_claims_a_language_wins() {
        let first = Echo::new("plantuml");
        let second = Echo::new("plantuml");
        let providers = Providers::empty()
            .with(Arc::clone(&first) as Arc<dyn DiagramProvider>)
            .with(Arc::clone(&second) as Arc<dyn DiagramProvider>);

        let resolutions =
            providers.resolve(&[request("plantuml", "x")], &ResolveContext::default());

        assert_eq!(resolutions.len(), 1);
        assert_eq!(first.batches(), [["x"]]);
        assert!(
            second.batches().is_empty(),
            "a later provider must not also be asked",
        );
    }

    #[test]
    fn a_provider_can_fail_one_request_and_answer_the_rest() {
        /// Answers one request and fails the other: the per-diagram failure the
        /// trait promises, where one bad fence does not take its batch down with
        /// it.
        struct HalfBad;
        impl DiagramProvider for HalfBad {
            fn handles(&self, _language: &str) -> bool {
                true
            }
            fn resolve(
                &self,
                requests: &[&DiagramRequest],
                _ctx: &ResolveContext<'_>,
            ) -> Vec<Result<Resolved, DiagramError>> {
                requests
                    .iter()
                    .map(|r| {
                        if body(r) == "bad" {
                            Err(DiagramError {
                                message: format!("kroki timed out on {}", body(r)),
                                transient: true,
                            })
                        } else {
                            Ok(Resolved {
                                asset: Asset::Inline(DiagramContent::Svg(format!(
                                    "half-bad:{}",
                                    body(r)
                                ))),
                                size: None,
                                digest: digest_of(body(r)),
                                warnings: Vec::new(),
                            })
                        }
                    })
                    .collect()
            }
        }

        let providers = Providers::empty().with(Arc::new(HalfBad) as Arc<dyn DiagramProvider>);
        let requests = [request("plantuml", "good"), request("plantuml", "bad")];
        let resolutions = providers.resolve(&requests, &ResolveContext::default());

        assert_eq!(resolutions.len(), 2);
        assert_eq!(
            svg_of(resolutions[0].as_ref().expect("resolved")),
            "half-bad:good",
            "the good request must still resolve alongside a failing sibling",
        );

        let error = resolutions[1]
            .as_ref()
            .expect_err("the provider failed this one");
        assert_eq!(
            error.message, "kroki timed out on bad",
            "the provider's own message must reach the caller unchanged",
        );
        assert!(
            error.transient,
            "a timeout stays retryable, or a caller caches the failure forever",
        );
    }

    #[test]
    fn reports_an_error_for_a_language_no_provider_claims() {
        let providers = Providers::empty().with(Echo::new("plantuml") as Arc<dyn DiagramProvider>);

        let resolutions =
            providers.resolve(&[request("vegalite", "{}")], &ResolveContext::default());

        // Total over the requests: a consumer that reserved one slot per request
        // fills each from this list, so a missing entry would resurface there as a
        // request that never got an answer.
        let error = resolutions[0]
            .as_ref()
            .expect_err("no provider handles vegalite");
        assert!(
            error.message.contains("vegalite"),
            "the message should name the language: {}",
            error.message,
        );
        assert!(!error.transient, "retrying cannot conjure a provider");
    }

    #[test]
    fn reports_an_error_when_a_provider_returns_too_few_results() {
        /// Breaks the positional contract: two requests, one result.
        struct Stingy;
        impl DiagramProvider for Stingy {
            fn handles(&self, _language: &str) -> bool {
                true
            }
            fn resolve(
                &self,
                _requests: &[&DiagramRequest],
                _ctx: &ResolveContext<'_>,
            ) -> Vec<Result<Resolved, DiagramError>> {
                Vec::from([Ok(Resolved {
                    asset: Asset::Inline(DiagramContent::Svg("only one".to_owned())),
                    size: None,
                    digest: digest_of("only one"),
                    warnings: Vec::new(),
                })])
            }
        }

        let providers = Providers::empty().with(Arc::new(Stingy) as Arc<dyn DiagramProvider>);
        let requests = [request("plantuml", "first"), request("plantuml", "second")];
        let resolutions = providers.resolve(&requests, &ResolveContext::default());

        assert_eq!(resolutions.len(), 2, "every request must get an entry");
        assert_eq!(
            svg_of(resolutions[0].as_ref().expect("resolved")),
            "only one",
            "the single result pairs with the first request, not some other one",
        );

        let error = resolutions[1]
            .as_ref()
            .expect_err("the provider skipped this one");
        assert!(
            error.message.contains("plantuml"),
            "the message should name the language: {}",
            error.message,
        );
        assert!(!error.transient);
    }

    #[test]
    fn discards_results_a_provider_returns_beyond_its_request_count() {
        /// Breaks the positional contract the other way: one request, two
        /// results.
        struct Generous;
        impl DiagramProvider for Generous {
            fn handles(&self, _language: &str) -> bool {
                true
            }
            fn resolve(
                &self,
                _requests: &[&DiagramRequest],
                _ctx: &ResolveContext<'_>,
            ) -> Vec<Result<Resolved, DiagramError>> {
                ["first", "surplus"]
                    .into_iter()
                    .map(|svg| {
                        Ok(Resolved {
                            asset: Asset::Inline(DiagramContent::Svg(svg.to_owned())),
                            size: None,
                            digest: digest_of(svg),
                            warnings: Vec::new(),
                        })
                    })
                    .collect()
            }
        }

        let providers = Providers::empty().with(Arc::new(Generous) as Arc<dyn DiagramProvider>);
        let resolutions =
            providers.resolve(&[request("plantuml", "only")], &ResolveContext::default());

        // Pinned so that switching to "reject the whole batch on a count
        // mismatch" is a decision someone makes, not a silent change: the first
        // N results pair positionally and the surplus is dropped.
        assert_eq!(resolutions.len(), 1);
        assert_eq!(svg_of(resolutions[0].as_ref().expect("resolved")), "first",);
    }

    #[test]
    fn with_no_providers_every_request_still_resolves_to_an_error() {
        // The default configuration, so totality has to hold here of all places.
        let providers = Providers::empty();

        let requests = [
            request("plantuml", "A -> B"),
            request("mermaid", "graph TD"),
        ];
        let resolutions = providers.resolve(&requests, &ResolveContext::default());

        assert_eq!(resolutions.len(), 2, "every request must get an entry");
        for (position, language) in [(0, "plantuml"), (1, "mermaid")] {
            let error = resolutions[position]
                .as_ref()
                .expect_err("no provider can answer");
            assert!(
                error.message.contains(language),
                "the message should name the language: {}",
                error.message,
            );
            assert!(!error.transient, "retrying cannot conjure a provider");
        }
    }

    #[test]
    fn routing_answers_what_its_providers_answer() {
        let providers = Providers::empty().with(Echo::new("plantuml") as Arc<dyn DiagramProvider>);

        // All a consumer that only classifies fences needs to ask.
        let router: &dyn DiagramRouter = &providers;
        assert!(router.handles("plantuml"));
        assert!(!router.handles("mermaid"));
    }

    #[test]
    fn resolving_no_requests_never_calls_a_provider() {
        let uml = Echo::new("plantuml");
        let providers = Providers::empty().with(Arc::clone(&uml) as Arc<dyn DiagramProvider>);

        assert_eq!(providers.resolve(&[], &ResolveContext::default()), []);
        assert_eq!(uml.batches(), [] as [Vec<String>; 0]);
    }
}
