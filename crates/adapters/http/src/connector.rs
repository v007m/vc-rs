use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use http::{HeaderValue, Uri};
use hyper_util::client::proxy::matcher::Matcher;
use rustls::ClientConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tower_service::Service;

use crate::HttpAdapterError;
use crate::stream::{Conn, ConnectRecord, Stream};
use crate::tls;

/// The largest proxy reply head to a CONNECT that is read before giving up.
const MAX_TUNNEL_REPLY: usize = 8 * 1024;

/// Opens connections for hyper's pool and times each phase: DNS (the system resolver, through
/// tokio), TCP connect, the proxy tunnel and the TLS handshake.
#[derive(Clone)]
pub struct TimedConnector {
    tls: Arc<ClientConfig>,
    proxy: Arc<Matcher>,
}

impl TimedConnector {
    pub const fn new(tls: Arc<ClientConfig>, proxy: Arc<Matcher>) -> Self {
        Self { tls, proxy }
    }

    async fn connect(self, dst: Uri) -> Result<Conn, HttpAdapterError> {
        let target = Target::from_uri(&dst)?;
        let proxy = self.proxy.intercept(&dst);
        let hop = match &proxy {
            Some(found) => Target::proxy(found.uri())?,
            None => target.clone(),
        };
        let (mut tcp, mut record) = open(&hop).await?;
        if !target.tls {
            return Ok(Conn::new(Stream::Plain(tcp), record, proxy.is_some()));
        }
        if let Some(found) = &proxy {
            let started = Instant::now();
            tunnel(&mut tcp, &target, found.basic_auth()).await?;
            record.connect += started.elapsed();
        }
        let started = Instant::now();
        let stream = tls::handshake(self.tls, tcp, &target.host).await?;
        record.tls = Some(started.elapsed());
        Ok(Conn::new(Stream::Tls(Box::new(stream)), record, false))
    }
}

impl Service<Uri> for TimedConnector {
    type Response = Conn;
    type Error = HttpAdapterError;
    type Future = Pin<Box<dyn Future<Output = Result<Conn, HttpAdapterError>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, dst: Uri) -> Self::Future {
        Box::pin(self.clone().connect(dst))
    }
}

/// Where a connection goes: host (IPv6 without brackets), port, and whether it speaks TLS.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    host: String,
    port: u16,
    tls: bool,
}

impl Target {
    fn from_uri(uri: &Uri) -> Result<Self, HttpAdapterError> {
        let host = uri.host().ok_or_else(|| HttpAdapterError::NoHost(uri.to_string()))?;
        let host = host.trim_start_matches('[').trim_end_matches(']').to_owned();
        let tls = uri.scheme_str() == Some("https");
        let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
        Ok(Self { host, port, tls })
    }

    /// A proxy to reach over plain TCP; `https://` and SOCKS proxies are not supported.
    fn proxy(uri: &Uri) -> Result<Self, HttpAdapterError> {
        let proxy = Self::from_uri(uri)?;
        if uri.scheme_str() != Some("http") {
            let reason = "only http:// proxies are supported".to_owned();
            return Err(HttpAdapterError::Connect { host: proxy.authority(), reason });
        }
        Ok(proxy)
    }

    fn authority(&self) -> String {
        if self.host.contains(':') { format!("[{}]:{}", self.host, self.port) } else { format!("{}:{}", self.host, self.port) }
    }
}

/// Resolves and connects, timing both phases.
async fn open(target: &Target) -> Result<(TcpStream, ConnectRecord), HttpAdapterError> {
    let (addresses, dns) = resolve(target).await?;
    let started = Instant::now();
    let (stream, remote) = dial(&addresses, target).await?;
    stream.set_nodelay(true).ok();
    Ok((stream, ConnectRecord::new(dns, started.elapsed(), remote)))
}

/// The addresses for `target`, and how long the lookup took (`None` for an IP literal).
async fn resolve(target: &Target) -> Result<(Vec<SocketAddr>, Option<Duration>), HttpAdapterError> {
    if let Ok(ip) = target.host.parse::<IpAddr>() {
        return Ok((vec![SocketAddr::new(ip, target.port)], None));
    }
    let fail = |reason: String| HttpAdapterError::Dns { host: target.host.clone(), reason };
    let started = Instant::now();
    let found = tokio::net::lookup_host((target.host.as_str(), target.port)).await.map_err(|e| fail(e.to_string()))?;
    let addresses: Vec<SocketAddr> = found.collect();
    if addresses.is_empty() {
        return Err(fail("no addresses".to_owned()));
    }
    Ok((addresses, Some(started.elapsed())))
}

/// Tries each address in turn; the last failure is the one reported.
async fn dial(addresses: &[SocketAddr], target: &Target) -> Result<(TcpStream, SocketAddr), HttpAdapterError> {
    let mut last = String::new();
    for address in addresses {
        match TcpStream::connect(address).await {
            Ok(stream) => return Ok((stream, *address)),
            Err(error) => last = error.to_string(),
        }
    }
    Err(HttpAdapterError::Connect { host: target.authority(), reason: last })
}

/// Asks the proxy on `stream` for a tunnel to `target` (HTTP CONNECT).
async fn tunnel(stream: &mut TcpStream, target: &Target, auth: Option<&HeaderValue>) -> Result<(), HttpAdapterError> {
    let authority = target.authority();
    let mut head = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n").into_bytes();
    if let Some(auth) = auth {
        head.extend_from_slice(b"Proxy-Authorization: ");
        head.extend_from_slice(auth.as_bytes());
        head.extend_from_slice(b"\r\n");
    }
    head.extend_from_slice(b"\r\n");
    let fail = |reason: String| HttpAdapterError::Tunnel { host: authority.clone(), reason };
    stream.write_all(&head).await.map_err(|e| fail(e.to_string()))?;
    let reply = read_head(stream).await.map_err(|e| fail(e.to_string()))?;
    let status = reply.split(|b| *b == b'\r').next().unwrap_or_default();
    if status.starts_with(b"HTTP/1.1 200") || status.starts_with(b"HTTP/1.0 200") {
        return Ok(());
    }
    Err(fail(String::from_utf8_lossy(status).into_owned()))
}

/// Reads a reply head, up to and including the blank line.
async fn read_head(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut reply = Vec::new();
    let mut chunk = [0_u8; 512];
    while !reply.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).await?;
        if read == 0 || reply.len() + read > MAX_TUNNEL_REPLY {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "no complete reply from the proxy"));
        }
        reply.extend_from_slice(&chunk[..read]);
    }
    Ok(reply)
}

#[cfg(test)]
#[path = "connector_tests.rs"]
mod tests;
