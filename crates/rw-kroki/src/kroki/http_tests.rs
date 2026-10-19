use std::time::{Duration, Instant};

use rw_diagrams::DiagramContent;

use super::{DiagramErrorKind, KrokiJob, create_agent, render_one};
use crate::language::{DiagramFormat, DiagramLanguage};
use crate::test_support::{Answer, KrokiStub, Reply, Request};

fn diagram(source: &str) -> KrokiJob {
    KrokiJob {
        source: source.to_owned(),
        language: DiagramLanguage::Mermaid,
        format: DiagramFormat::Svg,
    }
}

fn posted(connection: usize, body: &str) -> Request {
    Request {
        connection,
        path: "/mermaid/svg".to_owned(),
        body: body.to_owned(),
    }
}

#[test]
fn a_body_timeout_is_transient_and_does_not_poison_the_next_render() {
    let stub = KrokiStub::answering(Reply::InTurn(vec![
        Answer::Truncated(b"<svg>"),
        Answer::Full {
            status: 200,
            body: b"<svg>recovered</svg>",
        },
    ]));
    let agent = create_agent(Duration::from_millis(500));

    let started = Instant::now();
    let first = render_one(&agent, &diagram("graph TD; A-->B"), &stub.url);
    let elapsed = started.elapsed();
    let retry = render_one(&agent, &diagram("graph TD; A-->B"), &stub.url);

    let error = first.expect_err("a partial body must time out, not render");
    assert!(
        matches!(
            &error,
            DiagramErrorKind::HttpRequest(ureq::Error::Timeout(ureq::Timeout::Global))
        ),
        "expected the global body-read deadline, got {error:?}",
    );
    assert!(error.is_transient());
    assert!(elapsed < Duration::from_secs(3), "render took {elapsed:?}");
    assert_eq!(
        retry.expect("retry renders successfully"),
        DiagramContent::Svg("<svg>recovered</svg>".to_owned())
    );
    assert_eq!(
        stub.requests(),
        [posted(0, "graph TD; A-->B"), posted(1, "graph TD; A-->B")],
        "the retry must not reuse the abandoned connection",
    );
}

#[test]
fn a_reused_connection_keeps_render_bodies_separate_after_an_http_error() {
    let stub = KrokiStub::answering(Reply::InTurn(vec![
        Answer::Full {
            status: 200,
            body: b"<svg>first response</svg>",
        },
        Answer::Full {
            status: 503,
            body: b"temporarily unavailable",
        },
        Answer::Full {
            status: 200,
            body: b"<svg>last response</svg>",
        },
    ]));
    // Declared after the stub, so it is dropped first and closes the pooled
    // connection the stub is waiting on.
    let agent = create_agent(Duration::from_secs(2));

    let first = render_one(&agent, &diagram("first diagram"), &stub.url);
    let failed = render_one(&agent, &diagram("bad diagram"), &stub.url);
    let last = render_one(&agent, &diagram("last diagram"), &stub.url);

    assert_eq!(
        first.expect("first render"),
        DiagramContent::Svg("<svg>first response</svg>".to_owned())
    );
    match failed.expect_err("503 must remain a render failure") {
        DiagramErrorKind::HttpResponse { status, body } => {
            assert_eq!(status, 503);
            assert_eq!(body, "temporarily unavailable");
        }
        other => panic!("expected the HTTP error body, got {other:?}"),
    }
    assert_eq!(
        last.expect("render after the error"),
        DiagramContent::Svg("<svg>last response</svg>".to_owned())
    );
    // On one connection, or the agent's pool was never exercised.
    assert_eq!(
        stub.requests(),
        [
            posted(0, "first diagram"),
            posted(0, "bad diagram"),
            posted(0, "last diagram"),
        ],
    );
}
