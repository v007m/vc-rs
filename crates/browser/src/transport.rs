//! The playground's network: the demo service's router, called in process. A page has no
//! sockets, so every other host is refused as one that cannot be reached.

use std::fmt;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use http::header::{ACCEPT_ENCODING, CONTENT_LENGTH, HOST};
use http::{HeaderValue, Request, StatusCode};
use http_body_util::LengthLimitError;
use tower::ServiceExt as _;
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::{ConnectionInfo, Exchange, HttpResponse, RenderedRequest, Size, Timing};

/// The hosts the demo service answers on, as the example project's config names them.
pub const DEMO_HOSTS: &[&str] = &["localhost:7421", "127.0.0.1:7421"];

/// What the HTTP adapter asks for; the demo never compresses, but the request is the same.
const ACCEPT: &str = "gzip, br, zstd, deflate";

/// Sends each request to a router in process, as the HTTP adapter sends it over a socket: with
/// `accept-encoding`, `host` and `content-length` added, the body limit kept and the time taken.
#[derive(Clone)]
pub struct InProcessTransport {
    router: Router,
    hosts: Vec<String>,
}

impl fmt::Debug for InProcessTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InProcessTransport").field("hosts", &self.hosts).finish_non_exhaustive()
    }
}

impl InProcessTransport {
    /// `router`, answering on `hosts` (`host:port`); every other host is not reached.
    #[must_use]
    pub fn new(router: Router, hosts: &[&str]) -> Self {
        Self { router, hosts: hosts.iter().map(|&host| host.to_owned()).collect() }
    }

    /// The demo service, fresh, on [`DEMO_HOSTS`].
    #[must_use]
    pub fn demo() -> Self {
        Self::new(vc_demo::router(), DEMO_HOSTS)
    }

    fn wire_request(&self, request: &RenderedRequest) -> Result<Request<Body>, TransportError> {
        let authority = request.uri.authority().filter(|_| matches!(request.uri.scheme_str(), Some("http" | "https")));
        let Some(authority) = authority else {
            return Err(TransportError::InvalidRequest { reason: format!("{} is not an http(s) URL", request.uri) });
        };
        let default = if request.uri.scheme_str() == Some("https") { 443 } else { 80 };
        let port = authority.port_u16().unwrap_or(default);
        let host = format!("{}:{port}", authority.host());
        if !self.hosts.contains(&host) {
            let reason =
                format!("the playground has no network: only the demo service at {} answers", self.hosts.join(" and "));
            return Err(TransportError::Connect { host, reason });
        }
        let mut wire = Request::new(Body::from(request.body.clone()));
        *wire.method_mut() = request.method.clone();
        *wire.uri_mut() = request.uri.clone();
        *wire.headers_mut() = request.headers.clone();
        let headers = wire.headers_mut();
        headers.entry(ACCEPT_ENCODING).or_insert(HeaderValue::from_static(ACCEPT));
        if let Ok(value) = HeaderValue::from_str(authority.as_str()) {
            headers.entry(HOST).or_insert(value);
        }
        if !request.body.is_empty() {
            headers.entry(CONTENT_LENGTH).or_insert(HeaderValue::from(request.body.len()));
        }
        Ok(wire)
    }
}

#[async_trait]
impl Transport for InProcessTransport {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError> {
        let wire = self.wire_request(&request)?;
        let started = Instant::now();
        let answered = self.router.clone().oneshot(wire).await;
        let response = answered.map_err(|never| TransportError::Protocol { reason: never.to_string() })?;
        let first_byte = started.elapsed();
        let (parts, body) = response.into_parts();
        let limit = usize::try_from(ctx.body_limit.bytes()).unwrap_or(usize::MAX);
        let body = axum::body::to_bytes(body, limit).await.map_err(|error| body_error(error, ctx.body_limit))?;
        let total = started.elapsed();
        let timing = Timing { first_byte, download: total.saturating_sub(first_byte), total, ..Timing::default() };
        let mut headers = parts.headers;
        if sized(parts.status) {
            headers.entry(CONTENT_LENGTH).or_insert(HeaderValue::from(body.len()));
        }
        let wire_size = u64::try_from(body.len()).unwrap_or(u64::MAX);
        let response = HttpResponse { status: parts.status, headers, body, wire_size };
        Ok(Exchange { request, response, timing, connection: ConnectionInfo::default() })
    }
}

/// Whether a server sends `content-length` with this status, as hyper's does.
fn sized(status: StatusCode) -> bool {
    !(status.is_informational() || status == StatusCode::NO_CONTENT || status == StatusCode::NOT_MODIFIED)
}

/// A body over the limit, or one the router could not give.
fn body_error(error: axum::Error, limit: Size) -> TransportError {
    let inner = error.into_inner();
    if inner.downcast_ref::<LengthLimitError>().is_some() {
        return TransportError::BodyTooLarge { limit };
    }
    TransportError::Protocol { reason: inner.to_string() }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::extract::Path;
    use axum::routing::get;
    use http::{Method, StatusCode, Uri};
    use vc_interfaces::{SendContext, Transport, TransportError};
    use vc_model::RenderedRequest;
    use vc_testkit::transport_suite;

    use super::{DEMO_HOSTS, InProcessTransport};

    /// The demo service with the `/status/{code}` route the transport suite calls.
    fn suite_router() -> Router {
        let status =
            |Path(code): Path<u16>| async move { StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST) };
        vc_demo::router().route("/status/{code}", get(status))
    }

    #[tokio::test]
    async fn the_demo_in_process_meets_the_transport_contract() {
        let transport = InProcessTransport::new(suite_router(), DEMO_HOSTS);
        transport_suite(&transport, "http://localhost:7421").await;
    }

    #[tokio::test]
    async fn every_other_host_is_not_reached() {
        let transport = InProcessTransport::demo();
        let request = RenderedRequest::new(Method::GET, Uri::from_static("https://users.staging.example/users"));
        let error = transport.send(request, &SendContext::default()).await.unwrap_err();
        let TransportError::Connect { host, reason } = error else { panic!("{error:?}") };
        assert_eq!(host, "users.staging.example:443");
        assert!(reason.contains("only the demo service at localhost:7421"), "{reason}");
    }

    #[tokio::test]
    async fn a_url_without_a_host_is_not_sent() {
        let request = RenderedRequest::new(Method::GET, Uri::from_static("/users"));
        let error = InProcessTransport::demo().send(request, &SendContext::default()).await.unwrap_err();
        assert!(matches!(error, TransportError::InvalidRequest { .. }), "{error:?}");
    }
}
