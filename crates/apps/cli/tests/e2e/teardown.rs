//! A secret a failing `afterAll` or `afterRun` quotes shows masked in the json summary, the
//! CTRF, `JUnit` and HTML reports and the history, and an `on: failure` alert fires for the run
//! whose tests all passed, naming the blocks, with no part of the secret in any channel's
//! payload (D304, D308).

use std::net::{Ipv4Addr, SocketAddr};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use vc_mock_server::MockServer;

use crate::redaction::{assert_hidden, files, history_text};
use crate::{Project, events, of_kind, said};

const TOKEN: &str = "tok-fixture-Qm7Vz3Kp9Wd";

/// `echo#ping`, and `echo#send`, which sends `${secret.API_TOKEN}` as the body's `token`.
const API: (&str, &str) = (
    "services/echo/echo.api.yaml",
    "baseUrl: ${var.baseUrl}\napis:\n  - id: ping\n    method: POST\n    path: /echo\n\
     \x20 - id: send\n    method: POST\n    path: /echo\n    body: {token: '${secret.API_TOKEN}'}\n",
);

/// A test that passes, and an `afterAll` whose check quotes the echoed token.
const TEST: (&str, &str) = (
    "services/echo/ping.test.yaml",
    "afterAll:\n  - id: sweep\n    call: echo#send\n    expect: {body: {match: {body.token: wrong}}}\n\
     tests:\n  - id: pingsTheEcho\n    call: echo#ping\n",
);

/// The config: an `afterRun` whose check quotes the echoed token, and an `on: failure` rule
/// sending to every channel at `hook`.
fn config(hook: &str) -> String {
    format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {{baseUrl: 'http://x'}}\n\
         history: {{bodies: all}}\n\
         afterRun:\n  - id: flush\n    call: echo#send\n    expect: {{body: {{match: {{body.token: wrong}}}}}}\n\
         alerts:\n  - on: [failure]\n    notify:\n      - webhook: {{url: '{hook}/webhook'}}\n\
         \x20     - slack: {{webhook: '{hook}/slack'}}\n      - teams: {{webhook: '{hook}/teams'}}\n\
         \x20     - pagerduty: {{routingKey: pd-route-1, url: '{hook}/pagerduty'}}\n"
    )
}

/// A server that keeps each body posted to `/<channel>`, with the channel.
async fn hooks() -> (String, UnboundedReceiver<(String, String)>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (sent, kept) = unbounded_channel();
    let app = Router::new().route("/{channel}", routing::post(keep)).with_state(sent);
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, kept, server)
}

async fn keep(
    State(sent): State<UnboundedSender<(String, String)>>,
    Path(channel): Path<String>,
    body: String,
) -> StatusCode {
    sent.send((channel, body)).unwrap();
    StatusCode::OK
}

#[tokio::test(flavor = "multi_thread")]
async fn a_secret_a_failed_teardown_quotes_is_masked_in_every_report_and_alert() {
    let server = MockServer::start().await.unwrap();
    let (hook, mut kept, hooks) = hooks().await;
    let config = config(&hook);
    let schema = (".env.schema", "# the api's token\nAPI_TOKEN=\n");
    let project = Project::new(&[("vibranium.config.yaml", config.as_str()), API, TEST, schema]);
    let args = ["--reporter", "json", "--report", "ctrf,junit,html", "--out", "out"];
    let mut vc = project.run(&server.base_url(), &args);
    let run = vc.env("API_TOKEN", TOKEN).output().unwrap();
    assert_eq!(run.status.code(), Some(1), "the teardowns failed the run: {}", said(&run));
    hooks.abort();

    let events = events(&run.stdout);
    let summary = of_kind(&events, "summary");
    let blocks: Vec<_> =
        summary[0]["teardownFailures"].as_array().unwrap().iter().map(|f| f["block"].clone()).collect();
    assert_eq!(blocks, ["afterAll", "afterRun"], "{summary:?}");
    let masked = |failure: &serde_json::Value| failure["message"].as_str().unwrap_or_default().contains("••••");
    assert!(summary[0]["teardownFailures"].as_array().unwrap().iter().all(masked), "{summary:?}");

    let mut payloads = Vec::new();
    while let Ok(payload) = kept.try_recv() {
        payloads.push(payload);
    }
    payloads.sort();
    let channels: Vec<_> = payloads.iter().map(|(channel, _)| channel.as_str()).collect();
    assert_eq!(channels, ["pagerduty", "slack", "teams", "webhook"], "a failure alert fired on every channel");
    for (channel, body) in &payloads {
        let named = body.contains("afterAll in echo.ping") && body.contains("afterRun");
        assert!(named, "{channel} names the failed blocks: {body}");
    }

    let history = history_text(&project.path().join(".vibranium/history.db"));
    let mut seen = vec![("history.db rows".to_owned(), history), ("vc output".to_owned(), said(&run))];
    seen.extend(payloads.into_iter().map(|(channel, body)| (format!("{channel} payload"), body)));
    files(project.path().join(".vibranium").as_path(), &mut seen);
    files(project.path().join("out").as_path(), &mut seen);
    for report in ["out/ctrf.json", "out/junit.xml", "out/data/summary.js"] {
        let text = seen.iter().find(|(name, _)| name.ends_with(report)).map(|(_, text)| text.as_str());
        let text = text.unwrap_or_else(|| panic!("{report} was written"));
        assert!(text.contains("afterRun") && text.contains("••••"), "{report} names the teardown, masked");
    }
    assert_hidden(&seen, &[TOKEN]);
}
