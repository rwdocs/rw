use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use rw_diagrams::DiagramContent;

use super::{DiagramErrorKind, DiagramRequest, create_agent, render_one};
use crate::language::{DiagramFormat, DiagramLanguage};

const WATCHDOG: Duration = Duration::from_secs(5);

fn listen() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind the HTTP fixture");
    listener.set_nonblocking(true).expect("bound accept waits");
    let url = format!("http://{}", listener.local_addr().unwrap());
    (listener, url)
}

fn accept(listener: &TcpListener) -> BufReader<TcpStream> {
    let deadline = Instant::now() + WATCHDOG;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(WATCHDOG)).unwrap();
                stream.set_write_timeout(Some(WATCHDOG)).unwrap();
                return BufReader::new(stream);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "client never connected");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("accept HTTP request: {error}"),
        }
    }
}

fn read_source(reader: &mut BufReader<TcpStream>) -> String {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line, "POST /mermaid/svg HTTP/1.1\r\n");
    let mut length = None;
    loop {
        line.clear();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "incomplete headers"
        );
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let mut body = vec![0; length.expect("buffered diagram has Content-Length")];
    reader.read_exact(&mut body).unwrap();
    String::from_utf8(body).unwrap()
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
        body.len(),
    )
    .unwrap();
    stream.flush().unwrap();
}

fn diagram(source: &str) -> DiagramRequest {
    DiagramRequest {
        source: source.to_owned(),
        language: DiagramLanguage::Mermaid,
        format: DiagramFormat::Svg,
    }
}

#[test]
fn a_body_timeout_is_transient_and_does_not_poison_the_next_render() {
    let (listener, url) = listen();
    let agent = create_agent(Duration::from_millis(500));
    let (release, wait_for_client) = mpsc::channel();

    thread::scope(|scope| {
        let server = scope.spawn(move || {
            let mut reader = accept(&listener);
            assert_eq!(read_source(&mut reader), "graph TD; A-->B");
            let stream = reader.get_mut();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n<svg>")
                .unwrap();
            stream.flush().unwrap();
            // Keep the incomplete body open until the client returns: EOF must
            // not masquerade as enforcement of the request's timeout.
            wait_for_client
                .recv_timeout(WATCHDOG)
                .expect("client timeout");
            drop(reader);

            let mut retry = accept(&listener);
            assert_eq!(read_source(&mut retry), "graph TD; A-->B");
            respond(retry.get_mut(), 200, "<svg>recovered</svg>");
        });

        let started = Instant::now();
        let first = render_one(&agent, &diagram("graph TD; A-->B"), &url);
        let elapsed = started.elapsed();
        let _ = release.send(());
        let retry = render_one(&agent, &diagram("graph TD; A-->B"), &url);
        server.join().expect("HTTP fixture completed");

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
    });
}

#[test]
fn a_reused_connection_keeps_render_bodies_separate_after_an_http_error() {
    let (listener, url) = listen();
    let agent = create_agent(Duration::from_secs(2));

    thread::scope(|scope| {
        let server = scope.spawn(move || {
            // All three requests must arrive on this one connection. Accepting
            // a new socket for each would never exercise the agent's pool.
            let mut reader = accept(&listener);
            for (source, status, body) in [
                ("first diagram", 200, "<svg>first response</svg>"),
                ("bad diagram", 503, "temporarily unavailable"),
                ("last diagram", 200, "<svg>last response</svg>"),
            ] {
                assert_eq!(read_source(&mut reader), source);
                respond(reader.get_mut(), status, body);
            }
        });

        let first = render_one(&agent, &diagram("first diagram"), &url);
        let failed = render_one(&agent, &diagram("bad diagram"), &url);
        let last = render_one(&agent, &diagram("last diagram"), &url);
        server
            .join()
            .expect("all requests used the same connection");

        assert_eq!(
            first.expect("first render"),
            DiagramContent::Svg("<svg>first response</svg>".to_owned())
        );
        let error = failed.expect_err("503 must remain a render failure");
        assert!(error.is_transient());
        match error {
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
    });
}
