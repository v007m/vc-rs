use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderValue, Method, StatusCode, Uri};
use serde_json::Value;
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::{Exchange, RenderedRequest, Size};

/// The contract every [`Transport`] meets (D112).
///
/// `base` is a URL without a trailing slash whose server speaks the echo contract of
/// `vc-mock-server`: `ANY /echo` answers 200 with a JSON object of the request's `method`,
/// `path`, `query`, lowercase `headers` and `body` (parsed when JSON), and `GET /status/{code}`
/// answers with that status. [`crate::FakeTransport::echo`] speaks it without a network.
///
/// # Panics
///
/// When the transport breaks the contract.
pub async fn transport_suite(transport: &dyn Transport, base: &str) {
    sends_the_request_and_returns_it_with_the_response(transport, base).await;
    sends_the_body(transport, base).await;
    returns_error_statuses_as_responses(transport, base).await;
    refuses_a_body_over_the_limit(transport, base).await;
}

/// Builds a request to `base` + `path`.
fn request(method: Method, base: &str, path: &str) -> RenderedRequest {
    let uri = Uri::try_from(format!("{base}{path}")).unwrap_or_default();
    RenderedRequest::new(method, uri)
}

/// Sends `request`; asserts it succeeds, so `None` never reaches a caller.
async fn send(transport: &dyn Transport, request: &RenderedRequest) -> Option<Exchange> {
    let result = transport.send(request.clone(), &SendContext::default()).await;
    assert_eq!(result.as_ref().err(), None, "sending {} {} failed", request.method, request.uri);
    result.ok()
}

fn json(exchange: &Exchange) -> Value {
    serde_json::from_slice(&exchange.response.body).unwrap_or(Value::Null)
}

async fn sends_the_request_and_returns_it_with_the_response(transport: &dyn Transport, base: &str) {
    let mut sent = request(Method::GET, base, "/echo?probe=1");
    sent.headers.insert("x-probe", HeaderValue::from_static("yes"));
    let Some(exchange) = send(transport, &sent).await else { return };
    assert_eq!(exchange.request, sent, "the exchange carries the request as given");
    assert_eq!(exchange.response.status, StatusCode::OK);
    let echoed = json(&exchange);
    assert_eq!(echoed["method"], "GET", "{echoed}");
    assert_eq!(echoed["path"], "/echo", "{echoed}");
    assert_eq!(echoed["query"]["probe"], "1", "{echoed}");
    assert_eq!(echoed["headers"]["x-probe"], "yes", "{echoed}");
    let timing = exchange.timing;
    assert!(timing.total >= timing.first_byte && timing.total >= timing.download, "{timing:?}");
}

async fn sends_the_body(transport: &dyn Transport, base: &str) {
    let mut sent = request(Method::POST, base, "/echo");
    sent.headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    sent.body = Bytes::from_static(br#"{"name":"Ada"}"#);
    let Some(exchange) = send(transport, &sent).await else { return };
    let echoed = json(&exchange);
    assert_eq!(echoed["method"], "POST", "{echoed}");
    assert_eq!(echoed["body"]["name"], "Ada", "{echoed}");
}

async fn returns_error_statuses_as_responses(transport: &dyn Transport, base: &str) {
    let Some(exchange) = send(transport, &request(Method::GET, base, "/status/503")).await else { return };
    assert_eq!(exchange.response.status, StatusCode::SERVICE_UNAVAILABLE, "a 5xx is a response, not an error");
}

async fn refuses_a_body_over_the_limit(transport: &dyn Transport, base: &str) {
    let limit = Size::from_bytes(8);
    let ctx = SendContext { body_limit: limit };
    let result = transport.send(request(Method::GET, base, "/echo?pad=0123456789"), &ctx).await;
    assert_eq!(result.err(), Some(TransportError::BodyTooLarge { limit }));
}
