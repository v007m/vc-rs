//! `HttpTransport` over TLS against the https mock: trust, ALPN, timings and tunnels.

use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use http::{Method, StatusCode, Uri};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use vc_adapter_http::{ClientIdentity, HttpOptions, HttpTransport, ProxyConfig, TlsOptions};
use vc_interfaces::{CertChoice, CertId, SendContext, Transport, TransportError};
use vc_mock_server::{ClientAuth, MockHandle, MockServer, MockTls};
use vc_model::{Exchange, HttpVersion, RenderedRequest};
use vc_testkit::TestPki;

type TestResult = Result<(), Box<dyn Error>>;

/// The https mock with the test PKI's server certificate, offering h2 too when `h2` is set.
async fn mock(pki: &TestPki, h2: bool) -> Result<MockHandle, Box<dyn Error>> {
    let server = pki.server();
    let tls = MockTls::new(vec![server.cert_der().to_vec()], server.key_der().to_vec());
    Ok(MockServer::start_tls(if h2 { tls.h2() } else { tls }).await?)
}

fn trusting(pki: &TestPki, proxy: ProxyConfig) -> Result<HttpTransport, Box<dyn Error>> {
    let tls = TlsOptions { ca: vec![pki.ca_pem().into_bytes()], ..TlsOptions::default() };
    Ok(HttpTransport::new(&HttpOptions { proxy, tls, ..HttpOptions::default() })?)
}

async fn get(transport: &HttpTransport, url: &str) -> Result<Exchange, Box<dyn Error>> {
    let request = RenderedRequest::new(Method::GET, Uri::try_from(url)?);
    Ok(transport.send(request, &SendContext::default()).await?)
}

#[tokio::test]
async fn speaks_h2_to_a_trusted_server_and_times_the_handshake() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, true).await?;
    let transport = trusting(&pki, ProxyConfig::default())?;
    let first = get(&transport, &format!("{}/echo", mock.base_url())).await?;
    assert_eq!((first.response.status, first.connection.protocol), (StatusCode::OK, HttpVersion::Http2));
    assert!(first.timing.tls.is_some() && first.timing.connect.is_some(), "{:?}", first.timing);
    let second = get(&transport, &format!("{}/echo", mock.base_url())).await?;
    assert!(second.connection.reused && second.timing.tls.is_none(), "{:?}", second.connection);
    Ok(())
}

#[tokio::test]
async fn falls_back_to_http1_when_the_server_offers_only_that() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, false).await?;
    let url = format!("https://localhost:{}/echo", mock.addr().port());
    let exchange = get(&trusting(&pki, ProxyConfig::default())?, &url).await?;
    assert_eq!(exchange.connection.protocol, HttpVersion::Http1);
    assert!(exchange.timing.dns.is_some(), "a name is looked up");
    Ok(())
}

#[tokio::test]
async fn refuses_a_server_the_store_does_not_trust() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, true).await?;
    let transport = HttpTransport::new(&HttpOptions::default())?;
    let request = RenderedRequest::new(Method::GET, Uri::try_from(format!("{}/echo", mock.base_url()))?);
    let error = transport.send(request, &SendContext::default()).await.err();
    assert!(matches!(error, Some(TransportError::Tls { ref host, .. }) if host == "127.0.0.1"), "{error:?}");
    Ok(())
}

/// What a CONNECT proxy saw of a tunnel: its request head as soon as it is read, then the first
/// bytes it relayed to the origin.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Head(String),
    First([u8; 6]),
}

/// A CONNECT proxy that reports what it saw of each tunnel and then relays bytes both ways.
async fn connect_proxy() -> Result<(SocketAddr, mpsc::UnboundedReceiver<Seen>), Box<dyn Error>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let (sights, seen) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            tokio::spawn(relay(client, sights.clone()));
        }
    });
    Ok((address, seen))
}

/// The next thing the proxy saw, or none after a few seconds, so a test fails instead of hanging.
async fn next(seen: &mut mpsc::UnboundedReceiver<Seen>) -> Option<Seen> {
    tokio::time::timeout(Duration::from_secs(5), seen.recv()).await.ok().flatten()
}

async fn relay(mut client: TcpStream, sights: mpsc::UnboundedSender<Seen>) -> Option<()> {
    let mut upstream = open_tunnel(&mut client, &sights).await?;
    client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await.ok()?;
    let mut first = [0_u8; 6];
    client.read_exact(&mut first).await.ok()?;
    upstream.write_all(&first).await.ok()?;
    sights.send(Seen::First(first)).ok()?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream).await.ok()?;
    Some(())
}

/// Reads a CONNECT request head from `client` up to its blank line, reports it, and connects to
/// its target.
async fn open_tunnel(client: &mut TcpStream, sights: &mpsc::UnboundedSender<Seen>) -> Option<TcpStream> {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(client.read_u8().await.ok()?);
    }
    let head = String::from_utf8(head).ok()?;
    let target = head.strip_prefix("CONNECT ")?.split(' ').next()?.to_owned();
    sights.send(Seen::Head(head)).ok()?;
    TcpStream::connect(target).await.ok()
}

#[tokio::test]
async fn tunnels_https_through_a_connect_proxy() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, true).await?;
    let (proxy, mut seen) = connect_proxy().await?;
    let config = ProxyConfig { https: Some(format!("http://u:p@{proxy}")), ..ProxyConfig::default() };
    let exchange = get(&trusting(&pki, config)?, &format!("{}/echo", mock.base_url())).await?;
    assert_eq!((exchange.response.status, exchange.connection.protocol), (StatusCode::OK, HttpVersion::Http2));
    assert_eq!(exchange.connection.remote, Some(proxy), "the TCP peer is the proxy");
    let head = match next(&mut seen).await {
        Some(Seen::Head(head)) => head,
        other => format!("{other:?}"),
    };
    assert!(head.starts_with(&format!("CONNECT {} HTTP/1.1\r\n", mock.addr())), "{head}");
    assert!(head.contains("Proxy-Authorization: Basic dTpw\r\n"), "{head}");
    Ok(())
}

#[tokio::test]
async fn sends_the_client_certificate_through_the_tunnel_only() -> TestResult {
    let pki = TestPki::new()?;
    let (server, good) = (pki.server(), pki.good());
    let tls = MockTls::new(vec![server.cert_der().to_vec()], server.key_der().to_vec())
        .client_auth(ClientAuth::Required, vec![pki.ca_der().to_vec()])
        .name_client(good.name(), good.cert_der().to_vec());
    let mock = MockServer::start_tls(tls).await?;
    let (proxy, mut seen) = connect_proxy().await?;
    let identity = ClientIdentity::pem("good", good.cert_pem().as_bytes().to_vec(), good.key_pem().as_bytes().to_vec());
    let tls = TlsOptions { ca: vec![pki.ca_pem().into_bytes()], clients: vec![identity] };
    let proxy_config = ProxyConfig { https: Some(format!("http://{proxy}")), ..ProxyConfig::default() };
    let transport = HttpTransport::new(&HttpOptions { proxy: proxy_config, tls, ..HttpOptions::default() })?;
    let request = RenderedRequest::new(Method::GET, Uri::try_from(format!("{}/whoami", mock.base_url()))?);
    let ctx = SendContext { client_cert: CertChoice::Named(CertId::new(0)), ..SendContext::default() };
    let exchange = transport.send(request, &ctx).await?;
    let body: serde_json::Value = serde_json::from_slice(&exchange.response.body)?;
    assert_eq!((body["client"].as_str(), exchange.connection.remote), (Some("good"), Some(proxy)));
    let authority = mock.addr();
    let head = Seen::Head(format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n\r\n"));
    assert_eq!(next(&mut seen).await, Some(head), "only the tunnel request");
    let first = match next(&mut seen).await {
        Some(Seen::First(first)) => first,
        _ => [0; 6],
    };
    assert_eq!((first[0], first[1], first[5]), (0x16, 0x03, 0x01), "then a TLS ClientHello, relayed: {first:?}");
    Ok(())
}

#[tokio::test]
async fn trusts_a_ca_given_as_one_der_certificate() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, true).await?;
    let tls = TlsOptions { ca: vec![pki.ca_der().to_vec()], ..TlsOptions::default() };
    let transport = HttpTransport::new(&HttpOptions { tls, ..HttpOptions::default() })?;
    assert_eq!(get(&transport, &format!("{}/echo", mock.base_url())).await?.response.status, StatusCode::OK);
    Ok(())
}
