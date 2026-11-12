//! `vc-playground`: the WASI command the docs playground's worker runs. It reads one JSON
//! [`Request`] from stdin, prints what `vc check` or `vc run` prints to stdout and stderr, and
//! exits with the CLI's exit code.

use std::io::{Read as _, Write as _};
use std::sync::Arc;

use vc_browser::{Request, Sink, Stream};

/// A request the page sent wrong: the CLI's exit code for a usage error.
const EXIT_USAGE: u8 = 2;
/// The runtime would not start: the CLI's exit code for an internal error.
const EXIT_INTERNAL: u8 = 70;

/// Prints to the process's stdout and stderr, flushing each piece so the page shows it at once.
#[derive(Debug)]
struct Stdio;

impl Sink for Stdio {
    fn print(&self, stream: Stream, text: &str) {
        let written = match stream {
            Stream::Err => std::io::stderr().lock().write_all(text.as_bytes()).and_then(|()| std::io::stderr().flush()),
            _ => std::io::stdout().lock().write_all(text.as_bytes()).and_then(|()| std::io::stdout().flush()),
        };
        drop(written);
    }
}

fn main() {
    std::process::exit(i32::from(play()));
}

/// Reads the request and carries it out on a single-threaded runtime: the exit code.
fn play() -> u8 {
    let mut input = String::new();
    let request = std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())
        .and_then(|_| serde_json::from_str::<Request>(&input).map_err(|error| error.to_string()));
    let request = match request {
        Ok(request) => request,
        Err(error) => {
            Stdio.line(Stream::Err, &format!("error: the playground sent a request vc cannot read: {error}"));
            return EXIT_USAGE;
        }
    };
    match tokio::runtime::Builder::new_current_thread().enable_time().build() {
        Ok(runtime) => runtime.block_on(vc_browser::execute(request, Arc::new(Stdio))),
        Err(error) => {
            Stdio.line(Stream::Err, &format!("error: internal error in vc: the runtime would not start: {error}"));
            EXIT_INTERNAL
        }
    }
}
