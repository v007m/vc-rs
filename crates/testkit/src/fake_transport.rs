use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::{Map, Value, json};
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::{Exchange, HttpResponse, RenderedRequest, Timing};

/// What a [`FakeTransport`] answers to the `n`th request (counting from 0).
pub type Respond = dyn Fn(&RenderedRequest, usize) -> Result<HttpResponse, TransportError> + Send + Sync;

/// A transport that answers from a script, with no network.
///
/// It counts calls with an atomic, so clones share the count without a lock. Like a real
/// transport it refuses a body over `ctx.body_limit`; it reports zero timings and a new HTTP/1
/// connection.
#[derive(Clone)]
pub struct FakeTransport {
    respond: Arc<Respond>,
    calls: Arc<AtomicUsize>,
}

impl FakeTransport {
    /// Answers every request with `respond(request, call_index)`.
    pub fn new(
        respond: impl Fn(&RenderedRequest, usize) -> Result<HttpResponse, TransportError> + Send + Sync + 'static,
    ) -> Self {
        Self { respond: Arc::new(respond), calls: Arc::new(AtomicUsize::new(0)) }
    }

    /// Answers the `n`th request with `script[n]`, and every later one with the last entry.
    /// An empty script answers 200 with no body.
    #[must_use]
    pub fn scripted(script: Vec<Result<HttpResponse, TransportError>>) -> Self {
        Self::new(move |_, call| script.get(call).or_else(|| script.last()).cloned().unwrap_or_else(|| Ok(status(200))))
    }

    /// Speaks the echo contract of `vc-mock-server` (see [`crate::transport_suite`]); other
    /// paths get 404.
    #[must_use]
    pub fn echo() -> Self {
        Self::new(|request, _| Ok(echo(request)))
    }

    /// How many requests were sent so far.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl fmt::Debug for FakeTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeTransport").field("calls", &self.calls()).finish_non_exhaustive()
    }
}

#[async_trait]
impl Transport for FakeTransport {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let response = (self.respond)(&request, call)?;
        if response.body.len() as u64 > ctx.body_limit.bytes() {
            return Err(TransportError::BodyTooLarge { limit: ctx.body_limit });
        }
        Ok(Exchange { request, response, timing: Timing::default(), connection: vc_model::ConnectionInfo::default() })
    }
}

/// A response with `code` and no body or headers (an invalid code gives 500).
#[must_use]
pub fn status(code: u16) -> HttpResponse {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    HttpResponse { status, headers: HeaderMap::new(), body: Bytes::new(), wire_size: 0 }
}

fn echo(request: &RenderedRequest) -> HttpResponse {
    let path = request.uri.path();
    if let Some(code) = path.strip_prefix("/status/") {
        return status(code.parse().unwrap_or(400));
    }
    if path != "/echo" {
        return status(404);
    }
    let body = Bytes::from(describe(request).to_string());
    HttpResponse { status: StatusCode::OK, headers: HeaderMap::new(), wire_size: body.len() as u64, body }
}

fn describe(request: &RenderedRequest) -> Value {
    let query: Map<String, Value> = request
        .uri
        .query()
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, value)| (name.to_owned(), Value::from(value)))
        .collect();
    let headers: Map<String, Value> = request
        .headers
        .iter()
        .map(|(name, value)| (name.as_str().to_owned(), Value::from(String::from_utf8_lossy(value.as_bytes()))))
        .collect();
    json!({
        "method": request.method.as_str(),
        "path": request.uri.path(),
        "query": query,
        "headers": headers,
        "body": body_value(&request.body),
    })
}

fn body_value(body: &Bytes) -> Value {
    if body.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(body).unwrap_or_else(|_| Value::from(String::from_utf8_lossy(body)))
}

#[cfg(test)]
mod tests {
    use http::{Method, StatusCode, Uri};
    use vc_interfaces::{SendContext, Transport, TransportError};
    use vc_model::RenderedRequest;

    use super::{FakeTransport, status};
    use crate::transport_suite;

    fn get(path: &str) -> RenderedRequest {
        RenderedRequest::new(Method::GET, Uri::try_from(format!("http://fake.test{path}")).unwrap())
    }

    #[tokio::test]
    async fn the_echo_fake_passes_the_transport_suite() {
        transport_suite(&FakeTransport::echo(), "http://fake.test").await;
    }

    #[tokio::test]
    async fn echo_answers_404_off_contract_and_400_for_a_bad_status() {
        let fake = FakeTransport::echo();
        let ctx = SendContext::default();
        assert_eq!(fake.send(get("/nope"), &ctx).await.unwrap().response.status, StatusCode::NOT_FOUND);
        assert_eq!(fake.send(get("/status/abc"), &ctx).await.unwrap().response.status, StatusCode::BAD_REQUEST);
        assert_eq!(fake.calls(), 2);
    }

    #[tokio::test]
    async fn a_script_plays_in_order_then_repeats_its_last_entry() {
        let timeout = TransportError::Timeout { after: std::time::Duration::from_millis(1) };
        let fake = FakeTransport::scripted(vec![Err(timeout.clone()), Ok(status(201))]);
        let clone = fake.clone();
        let ctx = SendContext::default();
        assert_eq!(fake.send(get("/"), &ctx).await.err(), Some(timeout));
        assert_eq!(clone.send(get("/"), &ctx).await.unwrap().response.status, StatusCode::CREATED);
        assert_eq!(fake.send(get("/"), &ctx).await.unwrap().response.status, StatusCode::CREATED);
        assert_eq!(clone.calls(), 3, "clones share the count");
    }

    #[tokio::test]
    async fn an_empty_script_answers_200() {
        let fake = FakeTransport::scripted(Vec::new());
        assert_eq!(fake.send(get("/"), &SendContext::default()).await.unwrap().response.status, StatusCode::OK);
    }

    #[test]
    fn an_invalid_status_code_is_500() {
        assert_eq!(status(42).status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
