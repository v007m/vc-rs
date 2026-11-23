//! `sse` apis against the mock's `/events` over the real HTTP transport (#92 PR 5, D426–D429):
//! the events read and checked up to the last `receive`, their messages in the json, CTRF and HTML reports, a dropped
//! stream with and without `sse.reconnect`, a held stream that meets the time limit, and
//! `vc call`.

use std::fs;
use std::path::Path;
use std::process::Output;

use serde_json::Value;
use vc_mock_server::{MockHandle, MockServer};

use crate::{Project, STREAM_CONFIG, events, of_kind, said};

/// `feed#ticks` reads three events; `feed#dropped` loses its connection after event 2 of 4, and
/// `feed#resumed` is the same with `sse.reconnect`; `feed#held` stays open after event 1, its call limited to 300ms.
const FEED: (&str, &str) = (
    "services/feed/feed.api.yaml",
    concat!(
        "type: sse\nbaseUrl: ${var.baseUrl}\napis:\n",
        "  - id: ticks\n    path: /events\n    query: {count: '3'}\n",
        "  - id: dropped\n    path: /events\n    query: {count: '4', dropAt: '2', retry: '50'}\n",
        "  - id: resumed\n    path: /events\n    query: {count: '4', dropAt: '2', retry: '50'}\n",
        "    sse: {reconnect: true}\n",
        "  - id: held\n    path: /events\n    query: {count: '1', hold: 'true'}\n    timeout: 300ms\n",
    ),
);

/// Runs `tests` (a test file's text) with `args`; gives the output and the mock.
async fn run(tests: &str, args: &[&str]) -> (Output, Project, MockHandle) {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[STREAM_CONFIG, FEED, ("services/feed/feed.test.yaml", tests)]);
    let output = project.run(&server.base_url(), args).output().unwrap();
    (output, project, server)
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

/// Every file under `dir`'s `data/`, as text.
fn html_data(dir: &Path) -> String {
    let mut text = String::new();
    let mut folders = vec![dir.join("data")];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(folder).unwrap().map(Result::unwrap) {
            if entry.path().is_dir() {
                folders.push(entry.path());
            } else {
                text.push_str(&fs::read_to_string(entry.path()).unwrap());
            }
        }
    }
    text
}

#[tokio::test(flavor = "multi_thread")]
async fn an_sse_call_stops_reading_at_its_last_receive_and_reports_its_messages() {
    let tests = concat!(
        "tests:\n  - id: readsTheTicks\n    call: feed#ticks\n    exchange:\n",
        "      - receive: {id: '2', data: {equals: 2}}\n",
        "    expect:\n      status: 200\n      messages: {length: 2}\n",
        "      body: {equals: ''}\n",
    );
    let args = ["--reporter", "json", "--report", "ctrf,html", "--out", "out"];
    let (output, project, _server) = run(tests, &args).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let events = events(&output.stdout);
    let exchanged = of_kind(&events, "exchanged");
    assert_eq!((exchanged.len(), &exchanged[0]["messages"]), (1, &Value::from(2)), "{events:#?}");
    let ctrf = json(&project.path().join("out/ctrf.json"));
    let streams = &ctrf["results"]["tests"][0]["extra"]["streams"];
    assert_eq!(streams[0]["messages"].as_array().map(Vec::len), Some(2), "{ctrf:#}");
    assert_eq!((&streams[0]["messages"][1]["id"], &streams[0]["truncated"]), (&Value::from("2"), &Value::Bool(false)));
    let data = html_data(&project.path().join("out"));
    assert!(data.contains(r#"\"id\":\"2\""#), "the HTML report stores the messages: {data}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_stream_fails_its_unmet_receive_with_v5130_unless_it_reconnects() {
    let tests = concat!(
        "tests:\n  - id: losesTheLastTwo\n    call: feed#dropped\n    exchange:\n      - receive: {id: '4'}\n",
        "  - id: resumesAfterTheDrop\n    call: feed#resumed\n    exchange:\n      - receive: {id: '4'}\n",
        "    expect:\n      messages: {some: {id: '3'}}\n",
    );
    let (output, _project, server) = run(tests, &["--reporter", "json"]).await;
    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    let events = events(&output.stdout);
    let finished = of_kind(&events, "testFinished");
    let status = |id: &str| finished.iter().find(|event| event["test"] == id).map(|event| event["status"].clone());
    assert_eq!(status("feed.feed#losesTheLastTwo"), Some(Value::from("failed")), "{events:#?}");
    assert_eq!(status("feed.feed#resumesAfterTheDrop"), Some(Value::from("passed")), "{events:#?}");
    assert!(said(&output).contains("V5130"), "{}", said(&output));
    assert_eq!(server.stats().await.unwrap().count("GET /events"), 3, "one open each, and one reconnect");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_stream_with_an_unmet_receive_ends_at_the_time_limit_with_v5130() {
    let tests = "tests:\n  - id: waitsForTwo\n    call: feed#held\n    exchange:\n      - receive: {id: '2'}\n";
    let started = std::time::Instant::now();
    let (output, _project, _server) = run(tests, &[]).await;
    let printed = said(&output);
    assert_eq!(output.status.code(), Some(1), "{printed}");
    assert!(printed.contains("V5130"), "{printed}");
    assert!(started.elapsed() < std::time::Duration::from_secs(20), "the limit ends the read");
}

#[tokio::test(flavor = "multi_thread")]
async fn vc_call_opens_an_sse_api_and_prints_its_opening_response() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[STREAM_CONFIG, FEED]);
    project.write(STREAM_CONFIG.0, &STREAM_CONFIG.1.replace("http://x", &server.base_url()));
    let output = project.vc(&["call", "feed#ticks"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("200 OK"), "{}", said(&output));
    assert_eq!(server.stats().await.unwrap().count("GET /events"), 1);
}
