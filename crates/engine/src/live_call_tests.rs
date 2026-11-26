//! A live stream opens as a call does, tells its caller every message as it comes, masked,
//! sends and closes when asked, ends when its caller leaves, and is recorded as an `adhoc` run
//! (D543, D544).

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::mpsc;
use vc_interfaces::{HistoryReader as _, RunFilter};
use vc_testkit::{
    FakeStreamTransport, FakeTransport, FakeVcs, MemoryEnvironment, MemoryHistory, MemorySources, status,
};

use super::{LiveEvent, LiveIo, LiveSend};
use crate::sample::builder;
use crate::{AdhocCall, CallRequest, Engine};

/// The value of the secret `API_TOKEN`.
const TOKEN: &str = "tok-Lw7P-3cVz-Qe9M";

const CONFIG: &str = concat!(
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
    "history: {bodies: all}\n",
);

const FEED: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n";

/// An engine whose streams are the fake's contract ones, with `API_TOKEN` set, and its history.
fn engine() -> (Engine, Arc<MemoryHistory>) {
    let files: [(&str, &[u8]); 3] = [
        ("vibranium.config.yaml", CONFIG.as_bytes()),
        (".env.schema", b"API_TOKEN=\n"),
        ("services/users/users.api.yaml", FEED.as_bytes()),
    ];
    let history = Arc::new(MemoryHistory::new());
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&files)))
        .environment(Arc::new(MemoryEnvironment::new(&[("API_TOKEN", TOKEN)])))
        .transport(Arc::new(FakeTransport::new(|_, _| Ok(status(200)))))
        .stream_transport(Arc::new(FakeStreamTransport::contract()))
        .history(history.clone(), history.clone())
        .build()
        .unwrap();
    (engine, history)
}

/// An ad hoc WebSocket to the contract's echo, its secret in a header.
fn chat() -> CallRequest {
    let call = AdhocCall {
        kind: "websocket".into(),
        url: "${var.baseUrl}/ws".into(),
        headers: vec![("x-key".into(), "${secret.API_TOKEN}".into())],
        ..AdhocCall::default()
    };
    CallRequest { adhoc: Some(call), ..CallRequest::default() }
}

/// Opens `request`'s stream and runs `talk` as its caller; every event it was told, as text.
async fn live<F, Fut>(engine: &Engine, request: &CallRequest, talk: F) -> Vec<LiveEvent>
where
    F: FnOnce(mpsc::Receiver<LiveEvent>, mpsc::Sender<LiveSend>) -> Fut,
    Fut: Future<Output = Vec<LiveEvent>>,
{
    let (events, told) = mpsc::channel(16);
    let (asks, sends) = mpsc::channel(4);
    let (ran, seen) = tokio::join!(engine.call_stream(request, LiveIo::new(events, sends)), talk(told, asks));
    ran.unwrap();
    seen
}

/// Every event until the stream ends.
async fn drained(mut told: mpsc::Receiver<LiveEvent>, seen: &mut Vec<LiveEvent>) {
    while let Some(event) = told.recv().await {
        seen.push(event);
    }
}

fn message(event: &LiveEvent) -> Option<(bool, Value)> {
    match event {
        LiveEvent::Message { outgoing, fields, .. } => Some((*outgoing, Value::Object(fields.clone()))),
        _ => None,
    }
}

#[tokio::test]
async fn a_live_websocket_tells_what_is_sent_and_echoed_until_the_server_closes_it() {
    let (engine, history) = engine();
    let seen = live(&engine, &chat(), |mut told, asks| async move {
        let mut seen = vec![told.recv().await.unwrap()];
        asks.send(LiveSend::Text("hi".into())).await.unwrap();
        seen.extend([told.recv().await.unwrap(), told.recv().await.unwrap()]);
        asks.send(LiveSend::Text("bye".into())).await.unwrap();
        drained(told, &mut seen).await;
        seen
    })
    .await;
    let LiveEvent::Opened { run_id, head, sends: true, .. } = &seen[0] else { panic!("{seen:?}") };
    assert_eq!(head.response.status.as_u16(), 101);
    let echoed = json!({"type": "text", "data": "hi", "text": "hi"});
    let messages: Vec<_> = seen.iter().filter_map(message).collect();
    assert_eq!(messages[..2], [(true, echoed.clone()), (false, echoed)]);
    assert_eq!(messages[3], (false, json!({"type": "close", "code": 1000, "reason": "bye"})));
    let Some(LiveEvent::Ended { failed: false, code: Some(1000), .. }) = seen.last() else { panic!("{seen:?}") };
    let runs = history.runs(10, &RunFilter { kind: Some("adhoc".into()), ..RunFilter::default() }).unwrap();
    assert_eq!(runs[0].id.get(), *run_id, "the run it said it is recorded as");
    let call = runs[0].call.clone().unwrap();
    assert!(call.url.ends_with("x/ws") && call.status == Some(101), "{call:?}");
}

#[tokio::test]
async fn a_live_stream_masks_its_secrets_in_everything_it_tells() {
    let (engine, _) = engine();
    let seen = live(&engine, &chat(), |mut told, asks| async move {
        let mut seen = vec![told.recv().await.unwrap()];
        asks.send(LiveSend::Text(format!("my token is {TOKEN}"))).await.unwrap();
        seen.extend([told.recv().await.unwrap(), told.recv().await.unwrap()]);
        drop(asks);
        drained(told, &mut seen).await;
        seen
    })
    .await;
    let shown = format!("{seen:?}");
    assert!(seen.iter().filter_map(message).count() == 2 && !shown.contains(TOKEN), "{shown}");
}

#[tokio::test]
async fn a_live_stream_ends_when_its_caller_closes_it_or_leaves() {
    let (engine, _) = engine();
    let closed = live(&engine, &chat(), |mut told, asks| async move {
        let mut seen = vec![told.recv().await.unwrap()];
        asks.send(LiveSend::Close(1000)).await.unwrap();
        drained(told, &mut seen).await;
        seen
    })
    .await;
    let Some(LiveEvent::Ended { reason, failed: false, .. }) = closed.last() else { panic!("{closed:?}") };
    assert_eq!(reason, "the client closed the stream");
    let left = live(&engine, &chat(), |mut told, _asks| async move { vec![told.recv().await.unwrap()] }).await;
    assert!(matches!(left[..], [LiveEvent::Opened { .. }]), "it returned once its caller left: {left:?}");
}

#[tokio::test]
async fn a_live_websocket_opens_at_a_full_url() {
    let (engine, _) = engine();
    let call = AdhocCall { kind: "websocket".into(), url: "ws://127.0.0.1:4010/ws".into(), ..AdhocCall::default() };
    let request = CallRequest { adhoc: Some(call), ..CallRequest::default() };
    let seen = live(&engine, &request, |mut told, _asks| async move { vec![told.recv().await.unwrap()] }).await;
    let LiveEvent::Opened { head, .. } = &seen[0] else { panic!("{seen:?}") };
    assert_eq!(head.request.uri.to_string(), "ws://127.0.0.1:4010/ws");
}

#[tokio::test]
async fn a_live_event_stream_takes_no_sends_and_a_text_sent_on_it_leaves_it_open() {
    let (engine, _) = engine();
    let (kind, url, query) = ("sse".into(), "${var.baseUrl}/events".into(), vec![("count".into(), "2".into())]);
    let call = AdhocCall { kind, url, query, ..AdhocCall::default() };
    let request = CallRequest { adhoc: Some(call), ..CallRequest::default() };
    let seen = live(&engine, &request, |mut told, asks| async move {
        let mut seen = vec![told.recv().await.unwrap(), told.recv().await.unwrap()];
        drop(asks.send(LiveSend::Text("hi".into())).await);
        drained(told, &mut seen).await;
        seen
    })
    .await;
    assert!(matches!(seen[0], LiveEvent::Opened { sends: false, .. }), "{seen:?}");
    let read: Vec<_> = seen.iter().filter_map(message).collect();
    assert!(read.len() == 2 && read.iter().all(|(outgoing, _)| !outgoing), "both read, none sent: {seen:?}");
    let Some(LiveEvent::Ended { failed: false, .. }) = seen.last() else {
        panic!("it ended as the server did: {seen:?}")
    };
}

#[tokio::test]
async fn a_call_of_an_http_api_opens_no_live_stream() {
    let (engine, history) = engine();
    let request = CallRequest { api: "users#list".into(), ..CallRequest::default() };
    let seen = live(&engine, &request, |told, _asks| async move {
        let mut seen = Vec::new();
        drained(told, &mut seen).await;
        seen
    })
    .await;
    let [LiveEvent::Ended { reason, failed: true, code: None }] = &seen[..] else { panic!("{seen:?}") };
    assert!(reason.contains("`sse` or `websocket`"), "{reason}");
    let runs = history.runs(10, &RunFilter::default()).unwrap();
    assert_eq!(
        (runs[0].status, runs[0].call.clone()),
        (Some(vc_model::RunStatus::Failed), None),
        "recorded, nothing sent"
    );
}
