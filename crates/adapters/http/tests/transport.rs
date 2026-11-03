//! `HttpTransport` against `vc-mock-server`: the transport contract, decoding, the body cap,
//! connection reuse, timeouts, proxies and errors.

use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use http::header::{ACCEPT_ENCODING, CONTENT_ENCODING};
use http::{HeaderValue, Method, StatusCode, Uri};
use serde_json::Value;
use tokio::net::TcpListener;
use vc_adapter_http::{HttpOptions, HttpTransport, ProxyConfig};
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_mock_server::MockServer;
use vc_model::{Exchange, Protocol, RenderedRequest, Size};
use vc_testkit::transport_suite;

type TestResult = Result<(), Box<dyn Error>>;

fn get(url: &str) -> Result<RenderedRequest, Box<dyn Error>> {
    Ok(RenderedRequest::new(Method::GET, Uri::try_from(url)?))
}

async fn send(transport: &HttpTransport, request: RenderedRequest) -> Result<Exchange, TransportError> {
    transport.send(request, &SendContext::default()).await
}

fn json(exchange: &Exchange) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&exchange.response.body)?)
}

#[tokio::test]
async fn meets_the_transport_contract() -> TestResult {
    let mock = MockServer::start().await?;
    transport_suite(&HttpTransport::new(&HttpOptions::default())?, &mock.base_url()).await;
    Ok(())
}

async fn large_with(encoding: &'static str) -> Result<Exchange, Box<dyn Error>> {
    let mock = MockServer::start().await?;
    let mut request = get(&format!("{}/large?kb=50", mock.base_url()))?;
    request.headers.insert(ACCEPT_ENCODING, HeaderValue::from_static(encoding));
    Ok(send(&HttpTransport::new(&HttpOptions::default())?, request).await?)
}

#[tokio::test]
async fn decodes_gzip_and_brotli_and_reports_the_wire_size() -> TestResult {
    for encoding in ["gzip", "br"] {
        let exchange = large_with(encoding).await?;
        assert_eq!(exchange.response.headers.get(CONTENT_ENCODING).map(HeaderValue::as_bytes), Some(encoding.as_bytes()));
        assert_eq!(json(&exchange)?["items"].as_array().map(Vec::len), Some(50), "{encoding}");
        assert!(exchange.response.wire_size < exchange.response.body.len() as u64 / 10, "{encoding} compresses on the wire");
    }
    Ok(())
}

#[tokio::test]
async fn asks_for_compression_by_default() -> TestResult {
    let mock = MockServer::start().await?;
    let exchange = send(&HttpTransport::new(&HttpOptions::default())?, get(&format!("{}/echo", mock.base_url()))?).await?;
    assert_eq!(json(&exchange)?["headers"]["accept-encoding"], "gzip, br, zstd, deflate");
    assert_eq!(exchange.request.headers.get(ACCEPT_ENCODING), None, "the exchange keeps the request as rendered");
    Ok(())
}

#[tokio::test]
async fn refuses_a_body_over_the_limit_on_the_wire_and_after_decoding() -> TestResult {
    let mock = MockServer::start().await?;
    let transport = HttpTransport::new(&HttpOptions::default())?;
    let limit = Size::from_bytes(10 * 1024);
    let ctx = SendContext { body_limit: limit };
    for encoding in ["identity", "gzip"] {
        let mut request = get(&format!("{}/large?kb=50", mock.base_url()))?;
        request.headers.insert(ACCEPT_ENCODING, HeaderValue::from_static(encoding));
        let error = transport.send(request, &ctx).await.err();
        assert_eq!(error, Some(TransportError::BodyTooLarge { limit }), "{encoding}");
    }
    Ok(())
}

#[tokio::test]
async fn times_a_new_connection_and_marks_the_next_request_as_reused() -> TestResult {
    let mock = MockServer::start().await?;
    let transport = HttpTransport::new(&HttpOptions::default())?;
    let url = format!("{}/echo", mock.base_url());
    let first = send(&transport, get(&url)?).await?;
    let second = send(&transport, get(&url)?).await?;
    assert!(!first.connection.reused && first.timing.connect.is_some(), "{:?}", first.timing);
    assert_eq!((first.timing.dns, first.timing.tls), (None, None), "an IP literal over plain HTTP");
    assert!(second.connection.reused, "{:?}", second.connection);
    assert_eq!((second.timing.connect, second.timing.dns), (None, None));
    assert_eq!(first.connection.remote, Some(mock.addr()));
    assert_eq!(second.connection.remote, Some(mock.addr()));
    assert_eq!((first.connection.protocol, second.connection.protocol), (Protocol::Http1, Protocol::Http1));
    assert!(first.timing.total >= first.timing.first_byte);
    Ok(())
}

#[tokio::test]
async fn a_cancelled_slow_request_leaves_the_transport_usable() -> TestResult {
    let mock = MockServer::start().await?;
    let transport = HttpTransport::new(&HttpOptions::default())?;
    let slow = send(&transport, get(&format!("{}/slow?ms=2000", mock.base_url()))?);
    assert!(tokio::time::timeout(Duration::from_millis(100), slow).await.is_err(), "the slow call is cut off");
    let next = send(&transport, get(&format!("{}/status/204", mock.base_url()))?).await?;
    assert_eq!(next.response.status, StatusCode::NO_CONTENT);
    Ok(())
}

#[tokio::test]
async fn sends_plain_http_through_the_proxy_in_absolute_form() -> TestResult {
    let mock = MockServer::start().await?;
    let lookup = |name: &str| (name == "HTTP_PROXY").then(|| format!("http://u:p@{}", mock.addr()));
    let options = HttpOptions { proxy: ProxyConfig::from_env(&lookup), ..HttpOptions::default() };
    let exchange = send(&HttpTransport::new(&options)?, get("http://api.invalid/echo?via=proxy")?).await?;
    let echoed = json(&exchange)?;
    assert_eq!(echoed["headers"]["host"], "api.invalid", "{echoed}");
    assert_eq!(echoed["headers"]["proxy-authorization"], "Basic dTpw", "{echoed}");
    assert_eq!(echoed["query"]["via"], "proxy", "{echoed}");
    assert_eq!(exchange.request.headers.len(), 0, "proxy credentials stay out of the exchange");
    Ok(())
}

#[tokio::test]
async fn no_proxy_hosts_go_direct() -> TestResult {
    let mock = MockServer::start().await?;
    let proxy = ProxyConfig { http: Some("http://127.0.0.1:9".into()), https: None, no_proxy: Some("127.0.0.1".into()) };
    let options = HttpOptions { proxy, ..HttpOptions::default() };
    let exchange = send(&HttpTransport::new(&options)?, get(&format!("{}/echo", mock.base_url()))?).await?;
    assert_eq!(exchange.response.status, StatusCode::OK);
    Ok(())
}

async fn closed_port() -> Result<SocketAddr, Box<dyn Error>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    Ok(listener.local_addr()?)
}

#[tokio::test]
async fn reports_connect_dns_and_invalid_url_errors_with_their_codes() -> TestResult {
    let transport = HttpTransport::new(&HttpOptions::default())?;
    let refused = send(&transport, get(&format!("http://{}/", closed_port().await?))?).await.err();
    assert!(matches!(refused, Some(TransportError::Connect { .. })), "{refused:?}");
    let unknown = send(&transport, get("http://no-such-host.invalid/")?).await.err();
    assert!(matches!(unknown, Some(TransportError::Dns { ref host, .. }) if host == "no-such-host.invalid"), "{unknown:?}");
    for url in ["/relative", "ftp://files.test/x"] {
        let error = send(&transport, get(url)?).await.err();
        assert!(matches!(error, Some(TransportError::InvalidRequest { .. })), "{url}: {error:?}");
    }
    Ok(())
}

#[tokio::test]
async fn an_invalid_extra_root_is_refused_at_construction() {
    let options = HttpOptions { extra_roots: vec![b"not a certificate".to_vec()], ..HttpOptions::default() };
    assert!(HttpTransport::new(&options).is_err());
}
