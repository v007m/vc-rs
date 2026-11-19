//! `HttpTransport` over TLS against the https mock: trust, ALPN, timings and tunnels.

use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr};

use http::{Method, StatusCode, Uri};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use vc_adapter_http::{HttpOptions, HttpTransport, ProxyConfig, TlsOptions};
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_mock_server::{MockHandle, MockServer, MockTls};
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

/// A CONNECT proxy that reports each request head and then relays bytes both ways.
async fn connect_proxy() -> Result<(SocketAddr, mpsc::UnboundedReceiver<String>), Box<dyn Error>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let (heads, seen) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            tokio::spawn(relay(client, heads.clone()));
        }
    });
    Ok((address, seen))
}

async fn relay(mut client: TcpStream, heads: mpsc::UnboundedSender<String>) -> Option<()> {
    let mut head = vec![0_u8; 1024];
    let read = client.read(&mut head).await.ok()?;
    let head = String::from_utf8_lossy(&head[..read]).into_owned();
    let target = head.strip_prefix("CONNECT ")?.split(' ').next()?.to_owned();
    heads.send(head).ok()?;
    let mut upstream = TcpStream::connect(target).await.ok()?;
    client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await.ok()?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream).await.ok()?;
    Some(())
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
    let head = seen.recv().await.unwrap_or_default();
    assert!(head.starts_with(&format!("CONNECT {} HTTP/1.1\r\n", mock.addr())), "{head}");
    assert!(head.contains("Proxy-Authorization: Basic dTpw\r\n"), "{head}");
    Ok(())
}
