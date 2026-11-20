//! The engine hands the builder's stream transport, else the transport factory's, to the
//! executor, so `sse` and `websocket` calls run end to end against the fake (D420, D428).

use std::sync::Arc;

use serde_json::json;
use vc_interfaces::{Environment, StreamTransport, TlsError, TlsSpec, Transport, TransportFactory, TransportPorts};
use vc_testkit::{FakeStreamTransport, FakeTransport, FakeVcs, MemorySources, status};

use crate::sample::builder;
use crate::{CallReport, CallRequest, EngineBuilder};

const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
);
const FEED: (&str, &[u8]) = (
    "services/feed/feed.api.yaml",
    b"baseUrl: ${var.baseUrl}
apis:
  - id: ticks
    type: sse
    path: /events
    query: {count: 2}
    exchange:
      - receive: {id: '2', text: '2'}
    expect:
      status: 200
      messages: {every: {event: message}}
  - id: chat
    type: websocket
    path: /ws
    exchange:
      - send: hi
      - receive: {type: text, text: hi}
      - send: bye
      - receive: {type: close}
",
);

fn project(streams: Option<FakeStreamTransport>) -> EngineBuilder {
    let builder = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[CONFIG, FEED])))
        .transport(Arc::new(FakeTransport::new(|_, _| Ok(status(500)))));
    match streams {
        Some(streams) => builder.stream_transport(Arc::new(streams)),
        None => builder,
    }
}

async fn call(builder: EngineBuilder, api: &str) -> CallReport {
    let engine = builder.build().unwrap();
    engine.call(&CallRequest { api: api.into(), ..CallRequest::default() }).await.unwrap()
}

#[tokio::test]
async fn an_sse_call_reads_its_events_through_the_builder_s_stream_transport() {
    let streams = FakeStreamTransport::contract();
    let report = call(project(Some(streams.clone())), "feed#ticks").await;
    assert_eq!(report.exit, 0, "{}", report.text);
    assert!(report.checks.iter().all(|check| check.passed), "{:?}", report.checks);
    let (one, two) = (
        json!({"event": "message", "data": 1, "id": "1", "text": "1"}),
        json!({"event": "message", "data": 2, "id": "2", "text": "2"}),
    );
    let response = report.response.unwrap();
    assert_eq!(response["messages"], json!([one, two]));
    assert_eq!(response["exchange"], json!([two]), "the `receive` item's message");
    assert_eq!(streams.calls(), 1);
}

#[tokio::test]
async fn a_websocket_call_sends_and_receives_through_the_builder_s_stream_transport() {
    let report = call(project(Some(FakeStreamTransport::contract())), "feed#chat").await;
    assert_eq!(report.exit, 0, "{}", report.text);
    let echoed = json!({"type": "text", "data": "hi", "text": "hi"});
    let closed = json!({"type": "close", "code": 1000, "reason": "bye"});
    let response = report.response.unwrap();
    assert_eq!(response["messages"], json!([echoed, closed]));
    assert_eq!(response["exchange"], json!(["hi", echoed, "bye", closed]));
}

#[tokio::test]
async fn a_stream_call_without_a_stream_transport_fails_and_says_why() {
    let report = call(project(None), "feed#ticks").await;
    assert_eq!(report.exit, 1);
    assert!(report.text.contains("`sse` calls need a stream transport, and none is set"), "{}", report.text);
}

/// A factory whose ports carry `streams`, as the HTTP adapter's do.
struct Streaming(FakeStreamTransport);

impl TransportFactory for Streaming {
    fn build(&self, _: &TlsSpec, _: &dyn Environment) -> Result<TransportPorts, TlsError> {
        let transport: Arc<dyn Transport> = Arc::new(FakeTransport::new(|_, _| Ok(status(500))));
        let streams: Arc<dyn StreamTransport> = Arc::new(self.0.clone());
        Ok(TransportPorts { transport, streams: Some(streams) })
    }
}

#[tokio::test]
async fn without_a_builder_stream_transport_the_factory_s_stream_port_opens_the_stream() {
    let factory = FakeStreamTransport::contract();
    let report = call(project(None).transport_factory(Arc::new(Streaming(factory.clone()))), "feed#ticks").await;
    assert_eq!(report.exit, 0, "{}", report.text);
    assert_eq!(factory.calls(), 1);
}

#[tokio::test]
async fn the_builder_s_stream_transport_wins_over_the_factory_s() {
    let (given, factory) = (FakeStreamTransport::contract(), FakeStreamTransport::contract());
    let builder = project(Some(given.clone())).transport_factory(Arc::new(Streaming(factory.clone())));
    assert_eq!(call(builder, "feed#ticks").await.exit, 0);
    assert_eq!((given.calls(), factory.calls()), (1, 0));
}
