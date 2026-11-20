use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::json;
use vc_model::{ConnectionInfo, Exchange, HttpResponse, Protocol, RenderedRequest, Timing};

use super::ProtocolRuntimes;

#[test]
fn the_standard_runtimes_cover_every_protocol() {
    let runtimes = ProtocolRuntimes::standard();
    assert_eq!(runtimes.protocols(), Protocol::ALL);
    for protocol in Protocol::ALL {
        assert_eq!(runtimes.get(protocol).map(|runtime| runtime.protocol()), Some(protocol));
    }
}

#[test]
fn the_response_is_the_http_runtime_s_view_of_the_exchange_and_a_stream_s_adds_its_messages() {
    let request = RenderedRequest::new(http::Method::GET, "http://x/users/1".parse().unwrap());
    let body = Bytes::from_static(br#"{"id":1}"#);
    let response = HttpResponse { status: StatusCode::CREATED, headers: HeaderMap::new(), body, wire_size: 0 };
    let exchange = Exchange {
        request,
        response,
        timing: Timing::default(),
        connection: ConnectionInfo::default(),
        messages: Vec::new(),
    };
    let viewed = ProtocolRuntimes::standard().response(Protocol::Http, &exchange).unwrap();
    assert_eq!((&viewed["status"], &viewed["body"]), (&json!(201), &json!({"id": 1})));
    let mut streamed = viewed.clone();
    drop(streamed.as_object_mut().map(|root| root.insert("messages".to_owned(), json!([]))));
    for stream in [Protocol::Sse, Protocol::Websocket] {
        assert_eq!(ProtocolRuntimes::standard().response(stream, &exchange), Some(streamed.clone()), "{stream:?}");
    }
    assert_eq!(ProtocolRuntimes { runtimes: Vec::new() }.response(Protocol::Http, &exchange), None);
}
