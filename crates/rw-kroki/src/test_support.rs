//! Test doubles shared between this crate's test modules.
//!
//! [`KrokiStub`] answers renders over loopback, which is what makes the paths
//! behind a render reachable from a unit test: SVG post-processing, the cache
//! store, the order results are reassembled in, the display size a tag
//! generator is handed, the file an attachment is written to, and — through
//! [`Reply::Fixed`] — how a response the provider cannot use is classified.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::Mutex;
use rw_diagrams::SiteModel;
use rw_plantuml::prepare_diagram_source;

use crate::language::DiagramLanguage;

/// What the stub answers with.
///
/// A real Kroki answers a bad diagram with a 4xx and a struggling one with a
/// 5xx, and those two are classified differently. Nothing but the response
/// decides that, so a test that wants one has to be able to ask for it.
pub(crate) enum Reply {
    /// Render out of the posted source, so a response identifies the diagram it
    /// came from.
    Rendered,
    /// Answer every request with this status and these bytes, whatever was
    /// posted — an error status, or a body that is not the format it claims.
    Fixed { status: u16, body: &'static [u8] },
}

/// A stand-in Kroki: it answers every POST out of the request body, so a test
/// can tell which diagram a response belongs to, and records the paths it was
/// asked for. [`answering`](KrokiStub::answering) replaces that with a fixed
/// status and body.
///
/// Bound to an ephemeral loopback port, and the serving thread is stopped and
/// joined on drop, so nothing outlives the test that started it.
pub(crate) struct KrokiStub {
    /// Base URL to point a provider at.
    pub(crate) url: String,
    paths: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl KrokiStub {
    pub(crate) fn start() -> Self {
        Self::answering(Reply::Rendered)
    }

    /// A stub that answers every request the same way.
    pub(crate) fn answering(reply: Reply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the stub Kroki");
        let url = format!(
            "http://{}",
            listener.local_addr().expect("the stub's own address")
        );
        listener
            .set_nonblocking(true)
            .expect("poll the stub's listener");

        let paths = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let paths = Arc::clone(&paths);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => serve(&stream, &paths, &reply),
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
            paths,
            stop,
            thread: Some(thread),
        }
    }

    /// The request paths served so far, in the order they were answered.
    pub(crate) fn paths(&self) -> Vec<String> {
        self.paths.lock().clone()
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

/// Answer one request according to `reply`: a rendered answer carries the
/// posted source — SVG in a `<desc>`, PNG appended after the header — so either
/// way it identifies the diagram it came from; a fixed one ignores the source
/// altogether.
fn serve(stream: &TcpStream, paths: &Mutex<Vec<String>>, reply: &Reply) {
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

    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .expect("the stub's request line");
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();

    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).expect("a stub header") == 0 {
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

    let mut source = vec![0u8; length];
    reader.read_exact(&mut source).expect("the posted source");
    let source = String::from_utf8(source).expect("UTF-8 diagram source");

    let (status, body) = match reply {
        Reply::Fixed { status, body } => (*status, (*body).to_vec()),
        Reply::Rendered if path.ends_with("/png") => {
            let mut bytes = png_header();
            bytes.extend_from_slice(source.as_bytes());
            (200, bytes)
        }
        Reply::Rendered => (
            200,
            format!(
                r#"<svg width="400" height="200"><style>@import url('https://fonts.googleapis.com/css?family=Roboto');</style><desc>{source}</desc></svg>"#
            )
            .into_bytes(),
        ),
    };

    paths.lock().push(path);

    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {status} Stub\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .expect("write the stub response head");
    stream.write_all(&body).expect("write the stub response");
    stream.flush().expect("flush the stub response");
}

/// The source a `PlantUML`-family fence is actually rendered from: the body
/// after `!include` resolution and config injection.
///
/// The one language family where the prepared source differs from what the
/// author wrote, and therefore the one where a test keying on the fence body
/// would key on the wrong thing.
pub(crate) fn prepared_plantuml(
    source: &str,
    include_dirs: &[PathBuf],
    model: Option<&dyn SiteModel>,
) -> String {
    prepare_diagram_source(
        source,
        include_dirs,
        DiagramLanguage::PlantUml.render_dpi(),
        model,
    )
    .source
}

/// A 400x200 PNG signature and IHDR chunk — all that any size probe reads.
pub(crate) fn png_header() -> Vec<u8> {
    Vec::from([
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, // signature
        0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', // IHDR
        0x00, 0x00, 0x01, 0x90, // width 400
        0x00, 0x00, 0x00, 0xC8, // height 200
        0x08, 0x06, 0x00, 0x00, 0x00, // bit depth, colour type, …
    ])
}
