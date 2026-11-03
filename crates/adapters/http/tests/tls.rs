//! `HttpTransport` over TLS against a local rustls server: trust, ALPN, timings and tunnels.

use std::convert::Infallible;
use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode, Uri};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use vc_adapter_http::{HttpOptions, HttpTransport, ProxyConfig};
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::{Exchange, Protocol, RenderedRequest};

type TestResult = Result<(), Box<dyn Error>>;

const CA: &[u8] = include_bytes!("fixtures/ca.der");
const LEAF: &[u8] = include_bytes!("fixtures/leaf.der");
const LEAF_KEY: &[u8] = include_bytes!("fixtures/leaf.key.der");

fn server_config(alpn: &[&[u8]]) -> Result<Arc<ServerConfig>, Box<dyn Error>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(LEAF_KEY.to_vec()));
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(LEAF.to_vec())], key)?;
    config.alpn_protocols = alpn.iter().map(|protocol| protocol.to_vec()).collect();
    Ok(Arc::new(config))
}

async fn hello(_: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(Response::new(Full::new(Bytes::from_static(b"hello over tls"))))
}

/// Serves `hello` over TLS, speaking whichever protocol ALPN picked.
async fn serve(acceptor: TlsAcceptor, tcp: TcpStream) {
    let Ok(tls) = acceptor.accept(tcp).await else { return };
    let h2 = tls.get_ref().1.alpn_protocol() == Some(b"h2");
    let io = TokioIo::new(tls);
    if h2 {
        let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new()).serve_connection(io, service_fn(hello)).await;
    } else {
        let _ = hyper::server::conn::http1::Builder::new().serve_connection(io, service_fn(hello)).await;
    }
}

async fn tls_server(alpn: &[&[u8]]) -> Result<SocketAddr, Box<dyn Error>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let acceptor = TlsAcceptor::from(server_config(alpn)?);
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            tokio::spawn(serve(acceptor.clone(), tcp));
        }
    });
    Ok(address)
}

fn trusting(proxy: ProxyConfig) -> Result<HttpTransport, Box<dyn Error>> {
    Ok(HttpTransport::new(&HttpOptions { proxy, extra_roots: vec![CA.to_vec()], ..HttpOptions::default() })?)
}

async fn get(transport: &HttpTransport, url: &str) -> Result<Exchange, Box<dyn Error>> {
    let request = RenderedRequest::new(Method::GET, Uri::try_from(url)?);
    Ok(transport.send(request, &SendContext::default()).await?)
}

#[tokio::test]
async fn speaks_h2_to_a_trusted_server_and_times_the_handshake() -> TestResult {
    let address = tls_server(&[b"h2", b"http/1.1"]).await?;
    let transport = trusting(ProxyConfig::default())?;
    let first = get(&transport, &format!("https://{address}/")).await?;
    assert_eq!((first.response.status, &first.response.body[..]), (StatusCode::OK, &b"hello over tls"[..]));
    assert_eq!(first.connection.protocol, Protocol::Http2);
    assert!(first.timing.tls.is_some() && first.timing.connect.is_some(), "{:?}", first.timing);
    let second = get(&transport, &format!("https://{address}/")).await?;
    assert!(second.connection.reused && second.timing.tls.is_none(), "{:?}", second.connection);
    Ok(())
}

#[tokio::test]
async fn falls_back_to_http1_when_the_server_offers_only_that() -> TestResult {
    let address = tls_server(&[b"http/1.1"]).await?;
    let exchange = get(&trusting(ProxyConfig::default())?, &format!("https://localhost:{}/", address.port())).await?;
    assert_eq!(exchange.connection.protocol, Protocol::Http1);
    assert!(exchange.timing.dns.is_some(), "a name is looked up");
    Ok(())
}

#[tokio::test]
async fn refuses_a_server_the_store_does_not_trust() -> TestResult {
    let address = tls_server(&[b"h2"]).await?;
    let transport = HttpTransport::new(&HttpOptions::default())?;
    let request = RenderedRequest::new(Method::GET, Uri::try_from(format!("https://{address}/"))?);
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
    let address = tls_server(&[b"h2", b"http/1.1"]).await?;
    let (proxy, mut seen) = connect_proxy().await?;
    let config = ProxyConfig { https: Some(format!("http://u:p@{proxy}")), ..ProxyConfig::default() };
    let exchange = get(&trusting(config)?, &format!("https://{address}/")).await?;
    assert_eq!((exchange.response.status, exchange.connection.protocol), (StatusCode::OK, Protocol::Http2));
    assert_eq!(exchange.connection.remote, Some(proxy), "the TCP peer is the proxy");
    let head = seen.recv().await.unwrap_or_default();
    assert!(head.starts_with(&format!("CONNECT {address} HTTP/1.1\r\n")), "{head}");
    assert!(head.contains("Proxy-Authorization: Basic dTpw\r\n"), "{head}");
    Ok(())
}
