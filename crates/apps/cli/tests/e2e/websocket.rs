//! `websocket` apis against the mock's `/ws` over the real HTTP transport (#93 PR 6, D460–D465):
//! an exchange of sends and receives with a group and the server's close, its messages in the
//! json and CTRF reports, a `ws://` base URL, and `vc call`.

use std::fs;
use std::process::Output;

use serde_json::Value;
use vc_mock_server::{MockHandle, MockServer};

use crate::{Project, STREAM_CONFIG, events, of_kind, said};

/// `chat#room` opens the mock's `/ws`, which answers `join` and `say` and closes on `bye`;
/// `chat#leaves` says `bye` and reads until the server closes.
const CHAT: (&str, &str) = (
    "services/chat/chat.api.yaml",
    concat!(
        "type: websocket\nbaseUrl: ${var.baseUrl}\napis:\n  - id: room\n    path: /ws\n    timeout: 5s\n",
        "  - id: leaves\n    path: /ws\n    timeout: 5s\n    exchange:\n      - send: bye\n",
    ),
);

/// A test that joins, says hi, waits for both replies in any order, and says bye.
const ROOM: &str = concat!(
    "tests:\n  - id: echoesEachMessageToTheRoom\n    call: chat#room\n    exchange:\n",
    "      - send: {type: join}\n",
    "      - receive: {data: {match: {type: joined}}}\n",
    "      - send: {type: say, text: hi}\n",
    "      - receive:\n          - data: {match: {type: seen}}\n          - data: {match: {type: message, text: hi}}\n",
    "      - send: bye\n",
    "      - receive: {type: close, code: 1000, reason: bye}\n",
    "    expect:\n      status: 101\n      messages: {length: 4}\n",
    "    save:\n      memberId: ${response.exchange[1].data.memberId}\n",
);

/// Runs `tests` against the mock at `base` (its URL with `scheme`) with `args`.
async fn run(tests: &str, scheme: &str, args: &[&str]) -> (Output, Project, MockHandle) {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[STREAM_CONFIG, CHAT, ("services/chat/rooms.test.yaml", tests)]);
    let base = server.base_url().replacen("http", scheme, 1);
    let output = project.run(&base, args).output().unwrap();
    (output, project, server)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_websocket_call_sends_and_receives_its_exchange_and_reports_its_messages() {
    let args = ["--reporter", "json", "--report", "ctrf", "--out", "out"];
    let (output, project, server) = run(ROOM, "http", &args).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let events = events(&output.stdout);
    let exchanged = of_kind(&events, "exchanged");
    assert_eq!((exchanged.len(), &exchanged[0]["messages"]), (1, &Value::from(4)), "{events:#?}");
    let ctrf: Value = serde_json::from_slice(&fs::read(project.path().join("out/ctrf.json")).unwrap()).unwrap();
    let messages = &ctrf["results"]["tests"][0]["extra"]["streams"][0]["messages"];
    assert_eq!(messages[0]["data"]["memberId"], "m1", "{ctrf:#}");
    assert_eq!((&messages[3]["type"], &messages[3]["code"]), (&Value::from("close"), &Value::from(1000)), "{ctrf:#}");
    assert_eq!(server.stats().await.unwrap().count("GET /ws"), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ws_base_url_opens_the_same_websocket() {
    let (output, _project, server) = run(ROOM, "ws", &[]).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert_eq!(server.stats().await.unwrap().count("GET /ws"), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unmet_receive_fails_with_v5130_once_the_server_closes() {
    let tests = concat!(
        "tests:\n  - id: waitsForAReplyAfterBye\n    call: chat#room\n    exchange:\n",
        "      - send: bye\n      - receive: {data: {match: {type: joined}}}\n",
    );
    let (output, _project, _server) = run(tests, "http", &[]).await;
    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    assert!(said(&output).contains("V5130"), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn vc_call_opens_a_websocket_api_and_prints_its_handshake() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[STREAM_CONFIG, CHAT]);
    project.write(STREAM_CONFIG.0, &STREAM_CONFIG.1.replace("http://x", &server.base_url()));
    let output = project.vc(&["call", "chat#leaves"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("101 Switching Protocols"), "{}", said(&output));
    assert_eq!(server.stats().await.unwrap().count("GET /ws"), 1);
}
