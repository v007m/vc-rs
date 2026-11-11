//! The docs playground (D293) prints what `vc run` prints: the site's example project, run by
//! the real `vc` against the demo service on a socket and by `vc-browser` against it in process,
//! gives the same spec output once timings and the demo's address are masked, when it passes and
//! when a value is broken.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use vc_browser::{Request, Sink, Stream};

use crate::{Project, said};

/// The example project the playground page starts from.
const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../site/src/examples");
/// Where the example project expects the demo service, as the page serves it.
const DEMO: &str = "localhost:7421";

/// The example project, with the `vc demo` lines of its lifecycle blocks made `echo`, since the
/// test serves the demo service itself.
fn examples() -> BTreeMap<String, String> {
    let mut files = vc_testkit::read_folder(Path::new(EXAMPLES)).unwrap();
    for text in files.values_mut() {
        *text = text.replace("command: vc demo", "command: echo demo");
    }
    files
}

/// Sends the page's standard output down a channel.
#[derive(Debug)]
struct Kept(Sender<String>);

impl Sink for Kept {
    fn print(&self, stream: Stream, text: &str) {
        if stream == Stream::Out {
            let _sent = self.0.send(text.to_owned());
        }
    }
}

/// What the playground prints for `files`, and its exit code.
async fn in_the_page(files: BTreeMap<String, String>) -> (u8, String) {
    let (send, printed) = channel();
    let request = Request { files, seed: Some(7), ..Request::default() };
    let exit = vc_browser::execute(request, Arc::new(Kept(send))).await;
    let out = printed.try_iter().collect();
    (exit, out)
}

/// What `vc run --seed 7` prints for `files` against the demo service on a socket, with the
/// demo's address written as the page has it.
async fn on_a_socket(files: &BTreeMap<String, String>) -> (i32, String, String) {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move { axum::serve(listener, vc_demo::router()).await });
    let pointed: Vec<(String, String)> =
        files.iter().map(|(path, text)| (path.clone(), text.replace(DEMO, &address))).collect();
    let pointed: Vec<(&str, &str)> = pointed.iter().map(|(path, text)| (path.as_str(), text.as_str())).collect();
    let project = Project::new(&pointed);
    let run = project.vc(&["run", "--seed", "7"]).output().unwrap();
    server.abort();
    let out = String::from_utf8_lossy(&run.stdout).replace(&address, DEMO);
    (run.status.code().unwrap(), out, said(&run))
}

/// `text` as the two runs can be compared: each duration (`12ms`, `1.4s`) is `<t>`, with the
/// padding that right-aligns it collapsed to one space, since a longer duration takes more of
/// it; the tests (each with its failure excerpt) are sorted, since they finish in any order; and
/// the line naming the report files is left out, since the page writes none and says so on stderr.
fn masked(text: &str) -> String {
    let (mut lines, mut tests) = (Vec::new(), Vec::<String>::new());
    for line in text.lines().filter(|line| !line.starts_with("reports  ")) {
        let mut line: String = line.split(' ').map(timeless).collect::<Vec<_>>().join(" ");
        while line.contains("  <t>") {
            line = line.replace("  <t>", " <t>");
        }
        match tests.last_mut() {
            Some(open) if line.starts_with("  ") => writeln!(open, "{line}").unwrap(),
            _ if !line.is_empty() && !line.starts_with(|c: char| c.is_ascii()) => tests.push(format!("{line}\n")),
            _ => lines.push(line),
        }
    }
    tests.sort();
    format!("{}\n{}", lines.join("\n"), tests.concat())
}

fn timeless(word: &str) -> String {
    let trimmed = word.trim_start_matches('(').trim_end_matches([')', ',']);
    let number = trimmed.trim_end_matches("ms").trim_end_matches('s');
    let timed = number != trimmed && !number.is_empty() && number.chars().all(|c| c.is_ascii_digit() || c == '.');
    if timed { word.replace(trimmed, "<t>") } else { word.to_owned() }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_playground_prints_what_vc_run_prints_for_the_example_project() {
    let files = examples();
    let (native, native_out, said) = on_a_socket(&files).await;
    let (page, page_out) = in_the_page(files).await;
    assert_eq!(native, 0, "{said}");
    assert_eq!(page, 0, "{page_out}");
    assert_eq!(masked(&page_out), masked(&native_out));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_playground_prints_the_same_failure_excerpt_as_vc_run() {
    let mut files = examples();
    let search = files.get_mut("services/echo/search.test.yaml").unwrap();
    *search = search.replace("q: ${example.term}", "q: boots");
    let (native, native_out, said) = on_a_socket(&files).await;
    let (page, page_out) = in_the_page(files).await;
    assert_eq!(native, 1, "{said}");
    assert_eq!(page, 1, "{page_out}");
    assert!(page_out.contains("boots"), "{page_out}");
    assert_eq!(masked(&page_out), masked(&native_out));
}
