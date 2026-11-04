use std::error::Error as _;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use http::header::{ACCEPT_ENCODING, PROXY_AUTHORIZATION};
use http::{Request, Response, Version};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::client::proxy::matcher::Matcher;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::{ConnectionInfo, Exchange, HttpResponse, Protocol, RenderedRequest, Timing};

use crate::connector::TimedConnector;
use crate::stream::ConnectRecord;
use crate::{HttpAdapterError, ProxyConfig, body, tls};

/// How the HTTP transport is set up. The engine builds it from config and the environment.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "options struct filled with `..Default::default()` (D160)")]
pub struct HttpOptions {
    /// Which proxy to use, if any.
    pub proxy: ProxyConfig,
    /// Extra trusted root certificates (DER), on top of the OS store.
    pub extra_roots: Vec<Vec<u8>>,
    /// How long an idle pooled connection is kept.
    pub pool_idle: Duration,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self { proxy: ProxyConfig::default(), extra_roots: Vec::new(), pool_idle: Duration::from_secs(90) }
    }
}

/// The HTTP/1.1 and HTTP/2 transport: hyper's pooled client over the timed connector.
///
/// Clones share the pool. Status codes are never errors; the body is decoded and capped.
#[derive(Clone)]
pub struct HttpTransport {
    client: Client<TimedConnector, Full<Bytes>>,
    proxy: Arc<Matcher>,
}

impl fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpTransport").finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// Builds the transport. It must be used inside a tokio runtime.
    ///
    /// # Errors
    ///
    /// [`HttpAdapterError::Certificate`] when an extra root is not a valid certificate.
    pub fn new(options: &HttpOptions) -> Result<Self, HttpAdapterError> {
        let tls = tls::client_config(&options.extra_roots)?;
        let proxy = Arc::new(options.proxy.matcher());
        let connector = TimedConnector::new(tls, Arc::clone(&proxy));
        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(options.pool_idle)
            .pool_timer(TokioTimer::new())
            .build(connector);
        Ok(Self { client, proxy })
    }

    /// The request hyper sends: the rendered one plus `accept-encoding` and, for plain HTTP
    /// through a proxy with credentials, `proxy-authorization`.
    fn wire_request(&self, request: &RenderedRequest) -> Result<Request<Full<Bytes>>, TransportError> {
        if request.uri.host().is_none() || !matches!(request.uri.scheme_str(), Some("http" | "https")) {
            return Err(TransportError::InvalidRequest { reason: format!("{} is not an http(s) URL", request.uri) });
        }
        let mut wire = Request::new(Full::new(request.body.clone()));
        *wire.method_mut() = request.method.clone();
        *wire.uri_mut() = request.uri.clone();
        *wire.headers_mut() = request.headers.clone();
        let headers = wire.headers_mut();
        headers.entry(ACCEPT_ENCODING).or_insert(http::HeaderValue::from_static(body::ACCEPT_ENCODING));
        let proxy = self.proxy.intercept(&request.uri).filter(|_| request.uri.scheme_str() == Some("http"));
        if let Some(auth) = proxy.as_ref().and_then(|found| found.basic_auth()) {
            headers.insert(PROXY_AUTHORIZATION, auth.clone());
        }
        Ok(wire)
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError> {
        let wire = self.wire_request(&request)?;
        let started = Instant::now();
        let response = self.client.request(wire).await.map_err(|error| transport_error(&error))?;
        let first_byte = started.elapsed();
        let (connection, record) = connection_of(&response);
        let (parts, incoming) = response.into_parts();
        let wire_body = body::read(incoming, ctx.body_limit).await?;
        let wire_size = wire_body.len() as u64;
        let decoded = body::decode(&parts.headers, wire_body, ctx.body_limit)?;
        let total = started.elapsed();
        let mut timing = Timing { first_byte, download: total.saturating_sub(first_byte), total, ..Timing::default() };
        if let Some(record) = record {
            (timing.dns, timing.connect, timing.tls) = (record.dns, Some(record.connect), record.tls);
        }
        let response = HttpResponse { status: parts.status, headers: parts.headers, body: decoded, wire_size };
        Ok(Exchange { request, response, timing, connection })
    }
}

/// The connection a response came over, and its connect record if this response is the first
/// on a new connection.
fn connection_of(response: &Response<Incoming>) -> (ConnectionInfo, Option<Arc<ConnectRecord>>) {
    let protocol = if response.version() == Version::HTTP_2 { Protocol::Http2 } else { Protocol::Http1 };
    let record = response.extensions().get::<Arc<ConnectRecord>>();
    let remote = record.map(|record| record.remote);
    let fresh = record.filter(|record| record.claim()).cloned();
    (ConnectionInfo { reused: fresh.is_none(), protocol, remote }, fresh)
}

/// Maps a client error: a connect failure keeps its meaning; the rest is a protocol error.
fn transport_error(error: &hyper_util::client::legacy::Error) -> TransportError {
    let mut source = error.source();
    while let Some(cause) = source {
        if let Some(adapter) = cause.downcast_ref::<HttpAdapterError>() {
            return adapter.clone().into();
        }
        source = cause.source();
    }
    TransportError::Protocol { reason: chain(error) }
}

/// The error and its causes, joined with `: `.
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}
