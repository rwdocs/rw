//! A stand-in Kroki for this crate's tests and, behind the `test-support`
//! feature, for other crates' integration tests.
//!
//! [`KrokiStub`] answers renders over loopback, which is what makes the paths
//! behind a render reachable from a test: SVG post-processing, the cache
//! store, the order results come back in, the display size a resolution
//! reports, how a response the provider cannot use is classified, and how the
//! HTTP agent copes with connections it reuses or abandons.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::Mutex;

/// A 400x200 PNG signature and IHDR chunk — all that any size probe reads.
pub const PNG_HEADER: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, // signature
    0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', // IHDR
    0x00, 0x00, 0x01, 0x90, // width 400
    0x00, 0x00, 0x00, 0xC8, // height 200
    0x08, 0x06, 0x00, 0x00, 0x00, // bit depth, colour type, …
];

/// What the stub answers with.
pub enum Reply {
    /// Render out of the posted source — SVG in a `<desc>`, PNG appended
    /// after [`PNG_HEADER`] — so a response identifies the diagram it came
    /// from.
    Rendered,
    /// Answer every request with this status and these bytes, whatever was
    /// posted — an error status, or a body that is not the format it claims.
    Fixed { status: u16, body: &'static [u8] },
    /// Answer the n-th request with the n-th answer, keeping each connection
    /// open for the client to reuse.
    InTurn(Vec<Answer>),
}

/// One answer of a [`Reply::InTurn`] script.
pub enum Answer {
    Full {
        status: u16,
        body: &'static [u8],
    },
    /// A 200 promising more body than it sends. The connection stays open
    /// until the client gives up on it, so only the client's own timeout can
    /// end the read — never an EOF from the stub.
    Truncated(&'static [u8]),
}

/// A request the stub answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Which accepted connection it arrived on, counting from 0.
    pub connection: usize,
    pub path: String,
    pub body: String,
}

/// A Kroki on an ephemeral loopback port, answering one connection at a time.
///
/// The serving thread is stopped and joined on drop, so nothing outlives the
/// test that started it.
pub struct KrokiStub {
    /// Base URL to point a provider at.
    pub url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl KrokiStub {
    pub fn start() -> Self {
        Self::answering(Reply::Rendered)
    }

    pub fn answering(reply: Reply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the stub Kroki");
        let url = format!(
            "http://{}",
            listener.local_addr().expect("the stub's own address")
        );
        listener
            .set_nonblocking(true)
            .expect("poll the stub's listener");

        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut connection = 0;
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            serve(stream, connection, &requests, &reply);
                            connection += 1;
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        Self {
            url,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    /// The requests answered so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().clone()
    }

    /// The paths of [`requests`](Self::requests).
    pub fn paths(&self) -> Vec<String> {
        self.requests
            .lock()
            .iter()
            .map(|r| r.path.clone())
            .collect()
    }
}

impl Drop for KrokiStub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Answer requests on one connection until the reply closes it or the client
/// does.
fn serve(stream: TcpStream, connection: usize, requests: &Mutex<Vec<Request>>, reply: &Reply) {
    stream
        .set_nonblocking(false)
        .expect("read the stub connection");
    // A client that connects and then stalls would otherwise block this thread
    // forever, and the join in `Drop` behind it — a hung test run with nothing
    // to time it out.
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("bound the stub's read");
    let mut reader = BufReader::new(stream.try_clone().expect("clone the stub connection"));
    let mut stream = stream;

    while let Some((path, body)) = read_request(&mut reader) {
        let mut requests = requests.lock();
        // `short` promises one byte more than is sent.
        let (status, bytes, keep_alive, short) = match reply {
            Reply::Fixed { status, body } => (*status, body.to_vec(), false, false),
            Reply::Rendered => {
                let bytes = if path.ends_with("/png") {
                    [PNG_HEADER, body.as_bytes()].concat()
                } else {
                    format!(
                        r#"<svg width="400" height="200"><style>@import url('https://fonts.googleapis.com/css?family=Roboto');</style><desc>{body}</desc></svg>"#
                    )
                    .into_bytes()
                };
                (200, bytes, false, false)
            }
            Reply::InTurn(answers) => match answers.get(requests.len()) {
                Some(Answer::Full { status, body }) => (*status, body.to_vec(), true, false),
                Some(Answer::Truncated(body)) => (200, body.to_vec(), true, true),
                None => panic!("no answer scripted for request {}", requests.len()),
            },
        };
        requests.push(Request {
            connection,
            path,
            body,
        });
        drop(requests);

        let length = bytes.len() + usize::from(short);
        let connection_header = if keep_alive { "keep-alive" } else { "close" };
        write!(
            stream,
            "HTTP/1.1 {status} Stub\r\nContent-Length: {length}\r\nConnection: {connection_header}\r\n\r\n"
        )
        .expect("write the stub response head");
        stream.write_all(&bytes).expect("write the stub response");
        stream.flush().expect("flush the stub response");
        if !keep_alive {
            break;
        }
    }
}

/// The path and body of the next request, or `None` once the client has
/// closed the connection or stalled past the read timeout.
fn read_request(reader: &mut BufReader<TcpStream>) -> Option<(String, String)> {
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).ok()? == 0 {
        return None;
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();

    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().expect("a numeric Content-Length");
        }
    }

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    Some((path, String::from_utf8(body).expect("UTF-8 diagram source")))
}
