use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use http::header::{ACCEPT_ENCODING, PROXY_AUTHORIZATION};
use http::uri::{Authority, Uri};
use http::{Request, Response, Version};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::client::proxy::matcher::Matcher;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use rustls::RootCertStore;
use vc_interfaces::{CertChoice, SendContext, SentCert, Transport, TransportError};
use vc_model::{ConnectionInfo, Exchange, HttpResponse, HttpVersion, RenderedRequest, Size, Timing, wire_host};

use crate::alert::Failure;
use crate::connector::{TimedConnector, bare_host};
use crate::error::{ConnectError, causes, chain};
use crate::hosts::{ClientTable, HostRules};
use crate::stream::ConnectRecord;
use crate::{ClientIdentity, Connections, HttpAdapterError, ProxyConfig, body, tls};

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
    /// The open connections' count the transport adds to (D675).
    pub connections: Connections,
}

impl Default for HttpOptions {
    fn default() -> Self {
        let pool_idle = Duration::from_secs(90);
        Self { proxy: ProxyConfig::default(), tls: TlsOptions::default(), pool_idle, connections: Connections::new() }
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

/// A pooled client and the `id` of its identity, if any.
type Picked<'pool> = (&'pool Pooled, &'pool Option<Arc<str>>);

/// Marks a wire request that opens a WebSocket, so it goes through the HTTP/1.1-only client; a
/// request's own `upgrade` header does not (D460).
#[derive(Clone, Copy, Debug)]
pub struct Upgrading;

/// The pooled clients of one identity, and its `id`, if any. `upgrades` offers only HTTP/1.1 by
/// ALPN and carries the requests marked [`Upgrading`], a WebSocket's (D460).
struct Pool {
    client: Pooled,
    upgrades: Pooled,
    id: Option<Arc<str>>,
}

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
        Self::with_roots(&tls::roots(&options.tls.ca)?, options, HostRules::check(options.tls.clients.clone())?)
    }

    /// As [`HttpTransport::new`], trusting `roots` rather than `options.tls.ca` and presenting
    /// the identities of `checked` rather than `options.tls.clients`, each parsed and checked
    /// already.
    pub(crate) fn with_roots(
        roots: &Arc<RootCertStore>,
        options: &HttpOptions,
        checked: HostRules,
    ) -> Result<Self, HttpAdapterError> {
        let proxy = Arc::new(options.proxy.matcher());
        let build = |config| {
            let connector = TimedConnector::new(config, Arc::clone(&proxy), options.connections.clone());
            let mut builder = Client::builder(TokioExecutor::new());
            builder.pool_idle_timeout(options.pool_idle).pool_timer(TokioTimer::new()).build(connector)
        };
        let pooled = |identity: Option<&ClientIdentity>| -> Result<Pool, HttpAdapterError> {
            let config = tls::client_config(roots, identity)?;
            let upgrades = build(tls::http1_only(&config));
            Ok(Pool { client: build(config), upgrades, id: identity.map(|identity| Arc::from(identity.id())) })
        };
        let clients = ClientTable::new(checked, pooled(None)?, |identity| pooled(Some(identity)))?;
        Ok(Self { clients: Arc::new(clients), proxy })
    }

    /// The pooled client for the identity `choice` picks for `request`, with that identity's `id`:
    /// its HTTP/1.1-only one when `request` is marked [`Upgrading`].
    fn client(&self, request: &Request<Full<Bytes>>, choice: CertChoice) -> Result<Picked<'_>, TransportError> {
        let pool = self.clients.pick(request.uri(), choice);
        let pool = pool
            .ok_or_else(|| TransportError::InvalidRequest { reason: format!("no client identity for {choice:?}") })?;
        let client = if request.extensions().get::<Upgrading>().is_some() { &pool.upgrades } else { &pool.client };
        Ok((client, &pool.id))
    }

    /// The request hyper sends: the rendered one with its authority written as [`wire_host`]
    /// (D456) plus, for plain HTTP through a proxy with credentials, `proxy-authorization`.
    pub(crate) fn wire_request(&self, request: &RenderedRequest) -> Result<Request<Full<Bytes>>, TransportError> {
        if request.uri.host().is_none() || !matches!(request.uri.scheme_str(), Some("http" | "https")) {
            return Err(TransportError::InvalidRequest { reason: format!("{} is not an http(s) URL", request.uri) });
        }
        let mut wire = Request::new(Full::new(request.body.clone()));
        *wire.method_mut() = request.method.clone();
        *wire.uri_mut() = on_the_wire(&request.uri)?;
        *wire.headers_mut() = request.headers.clone();
        let proxy = self.proxy.intercept(&request.uri).filter(|_| request.uri.scheme_str() == Some("http"));
        if let Some(auth) = proxy.as_ref().and_then(|found| found.basic_auth()) {
            wire.headers_mut().insert(PROXY_AUTHORIZATION, auth.clone());
        }
        Ok(wire)
    }

    /// Sends `wire` and waits for the head of its answer, leaving the body to read.
    pub(crate) async fn answer(
        &self,
        wire: Request<Full<Bytes>>,
        ctx: &SendContext,
    ) -> Result<Answered, TransportError> {
        let (client, client_cert) = self.client(&wire, ctx.client_cert)?;
        let host = bare_host(wire.uri().host().unwrap_or_default()).to_owned();
        let started = Instant::now();
        let response = client
            .request(wire)
            .await
            .map_err(|error| transport_error(&error, &host, sent_cert(client_cert.as_deref(), ctx.client_cert)))?;
        let first_byte = started.elapsed();
        let (mut connection, record) = connection_of(&response);
        connection.client_cert.clone_from(client_cert);
        let mut timing = Timing { first_byte, total: first_byte, ..Timing::default() };
        if let Some(record) = record {
            (timing.dns, timing.connect, timing.tls) = (record.dns, Some(record.connect), record.tls);
        }
        let (parts, body) = response.into_parts();
        Ok(Answered { parts, body, connection, timing, started })
    }
}

/// The head of an answer, its body not read yet.
pub struct Answered {
    pub parts: http::response::Parts,
    body: Incoming,
    connection: ConnectionInfo,
    timing: Timing,
    started: Instant,
}

impl Answered {
    /// The exchange, with the body read and decoded, each capped at `limit`.
    pub async fn read(self, request: RenderedRequest, limit: Size) -> Result<Exchange, TransportError> {
        let Self { parts, body: incoming, connection, mut timing, started } = self;
        let wire_body = body::read(incoming, limit).await?;
        let wire_size = wire_body.len() as u64;
        let decoded = body::decode(&parts.headers, wire_body, limit)?;
        timing.total = started.elapsed();
        timing.download = timing.total.saturating_sub(timing.first_byte);
        let response = HttpResponse { status: parts.status, headers: parts.headers, body: decoded, wire_size };
        Ok(Exchange { request, response, timing, connection, messages: Vec::new() })
    }

    /// The exchange of the head alone, with an empty body, and the body left to stream.
    pub fn streamed(self, request: RenderedRequest) -> (Exchange, Incoming) {
        let Self { parts, body, connection, timing, .. } = self;
        let response = HttpResponse { status: parts.status, headers: parts.headers, body: Bytes::new(), wire_size: 0 };
        (Exchange { request, response, timing, connection, messages: Vec::new() }, body)
    }
}

/// `uri` with the authority a request signer signs (D438, D456): the host lowercased, the
/// scheme's default port left out and any user info dropped. hyper derives the HTTP/1.1 `host`
/// and the HTTP/2 `:authority` from it, so both match what was signed.
fn on_the_wire(uri: &Uri) -> Result<Uri, TransportError> {
    let invalid = |_| TransportError::InvalidRequest { reason: format!("{uri} has no usable host") };
    let host = wire_host(uri).unwrap_or_default();
    let mut parts = uri.clone().into_parts();
    parts.authority = Some(Authority::try_from(host).map_err(invalid)?);
    Uri::from_parts(parts).map_err(|_| TransportError::InvalidRequest { reason: format!("{uri} is not a valid URL") })
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError> {
        let mut wire = self.wire_request(&request)?;
        let encodings = http::HeaderValue::from_static(body::ACCEPT_ENCODING);
        wire.headers_mut().entry(ACCEPT_ENCODING).or_insert(encodings);
        self.answer(wire, ctx).await?.read(request, ctx.body_limit).await
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
fn sent_cert(id: Option<&str>, choice: CertChoice) -> SentCert {
    let none = || if choice == CertChoice::Off { SentCert::Off } else { SentCert::Missing };
    id.map_or_else(none, |id| SentCert::Named(id.to_owned()))
}

/// Maps a client error: a client certificate refused in the handshake or on the first read
/// (TLS 1.3 checks it after the client's handshake is done) names the certificate `sent`, a
/// connect failure keeps its meaning, any other alert fails the handshake, and the rest is a
/// protocol error.
fn transport_error(error: &hyper_util::client::legacy::Error, host: &str, sent: SentCert) -> TransportError {
    let failure = Failure::of(error, host, sent);
    if let Some(refused) = failure.refused() {
        return refused;
    }
    match causes(error).find_map(|cause| cause.downcast_ref::<ConnectError>()) {
        Some(ConnectError::Adapter(adapter)) => adapter.clone().into(),
        Some(ConnectError::Handshake(failed)) => failure.failed(failed.to_string()),
        None => failure.alerted().unwrap_or_else(|| TransportError::Protocol { reason: chain(error) }),
    }
}
