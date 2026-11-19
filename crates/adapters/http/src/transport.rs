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
use vc_interfaces::{CertChoice, SendContext, SentCert, Transport, TransportError};
use vc_model::{ConnectionInfo, Exchange, HttpResponse, HttpVersion, RenderedRequest, Timing};

use crate::alert::{self, Refusal};
use crate::connector::{TimedConnector, bare_host};
use crate::error::ConnectError;
use crate::hosts::ClientTable;
use crate::stream::ConnectRecord;
use crate::{ClientIdentity, HttpAdapterError, ProxyConfig, body, tls};

/// How the HTTP transport is set up. The engine builds it from config and the environment.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "options struct filled with `..Default::default()` (D160)")]
pub struct HttpOptions {
    /// Which proxy to use, if any.
    pub proxy: ProxyConfig,
    /// Extra trusted roots and the client identities.
    pub tls: TlsOptions,
    /// How long an idle pooled connection is kept.
    pub pool_idle: Duration,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self { proxy: ProxyConfig::default(), tls: TlsOptions::default(), pool_idle: Duration::from_secs(90) }
    }
}

/// The TLS setup shared by every request: extra trusted roots and the client identities.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "options struct filled with `..Default::default()` (D160)")]
pub struct TlsOptions {
    /// Extra trusted roots on top of the OS store: each entry is a PEM bundle of one or more
    /// certificates, or one DER certificate.
    pub ca: Vec<Vec<u8>>,
    /// The client identities; `CertId::new(i)` names entry `i`.
    pub clients: Vec<ClientIdentity>,
}

/// A pooled hyper client over the timed connector.
type Pooled = Client<TimedConnector, Full<Bytes>>;

/// A pooled client and the `id` of the identity it presents, if any.
type Pool = (Pooled, Option<Arc<str>>);

/// The HTTP/1.1 and HTTP/2 transport: hyper's pooled client over the timed connector.
///
/// It keeps one pooled client per client identity, plus one with none, so two identities never
/// share a connection, an HTTP/2 stream or a TLS session. Clones share the pools. Status codes
/// are never errors; the body is decoded and capped.
#[derive(Clone)]
pub struct HttpTransport {
    clients: Arc<ClientTable<Pool>>,
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
    /// [`HttpAdapterError::Certificate`] when a `ca` entry is not a valid certificate, and
    /// [`HttpAdapterError::Identity`] when a client identity cannot be used: a bad PEM text, an
    /// encrypted key, a key that does not match the certificate, or a bad host rule.
    pub fn new(options: &HttpOptions) -> Result<Self, HttpAdapterError> {
        let roots = tls::roots(&options.tls.ca)?;
        let proxy = Arc::new(options.proxy.matcher());
        let pooled = |identity: Option<&ClientIdentity>| -> Result<Pool, HttpAdapterError> {
            let connector = TimedConnector::new(tls::client_config(&roots, identity)?, Arc::clone(&proxy));
            let mut builder = Client::builder(TokioExecutor::new());
            let client = builder.pool_idle_timeout(options.pool_idle).pool_timer(TokioTimer::new()).build(connector);
            Ok((client, identity.map(|identity| Arc::from(identity.id()))))
        };
        let clients = ClientTable::new(&options.tls.clients, pooled(None)?, |identity| pooled(Some(identity)))?;
        Ok(Self { clients: Arc::new(clients), proxy })
    }

    /// The pooled client for the identity `choice` picks for `request`, with that identity's `id`.
    fn client(&self, request: &RenderedRequest, choice: CertChoice) -> Result<&Pool, TransportError> {
        let client = self.clients.pick(&request.uri, choice);
        client.ok_or_else(|| TransportError::InvalidRequest { reason: format!("no client identity for {choice:?}") })
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
        let (client, client_cert) = self.client(&request, ctx.client_cert)?;
        let started = Instant::now();
        let host = bare_host(request.uri.host().unwrap_or_default());
        let sent = || sent(client_cert.as_deref(), ctx.client_cert);
        let response = client.request(wire).await.map_err(|error| transport_error(&error, host, sent()))?;
        let first_byte = started.elapsed();
        let (mut connection, record) = connection_of(&response);
        connection.client_cert.clone_from(client_cert);
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
    let protocol = if response.version() == Version::HTTP_2 { HttpVersion::Http2 } else { HttpVersion::Http1 };
    let record = response.extensions().get::<Arc<ConnectRecord>>();
    let remote = record.map(|record| record.remote);
    let fresh = record.filter(|record| record.claim()).cloned();
    (ConnectionInfo { reused: fresh.is_none(), protocol, remote, client_cert: None }, fresh)
}

/// The certificate a request sent: the identity `id` of its pool, else none, because `choice`
/// turned them off or no identity matched the host.
fn sent(id: Option<&str>, choice: CertChoice) -> SentCert {
    let none = || if choice == CertChoice::Off { SentCert::Off } else { SentCert::Missing };
    id.map_or_else(none, |id| SentCert::Named(id.to_owned()))
}

/// Maps a client error: a client certificate refused in the handshake or on the first read
/// (TLS 1.3 checks it after the client's handshake is done) names the certificate `sent`, a
/// connect failure keeps its meaning, and the rest is a protocol error.
fn transport_error(error: &hyper_util::client::legacy::Error, host: &str, sent: SentCert) -> TransportError {
    if let Some(refusal) = Refusal::find(error) {
        return refusal.at(host, sent);
    }
    match alert::causes(error).find_map(|cause| cause.downcast_ref::<ConnectError>()) {
        Some(ConnectError::Adapter(adapter)) => adapter.clone().into(),
        Some(ConnectError::Handshake(failed)) => alert::failed_handshake(failed, host, &sent),
        None => TransportError::Protocol { reason: chain(error) },
    }
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
