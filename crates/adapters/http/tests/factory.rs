//! `HttpTransports` against `vc-mock-server`: the transport it builds goes through the proxy the
//! environment names, trusts the spec's roots and presents its identities, and its stream port
//! opens a body stream (D428).

use std::error::Error;

use http::{Method, Uri};
use vc_adapter_http::HttpTransports;
use vc_interfaces::{
    CertChoice, CertId, ClientSource, ClientSpec, MaterialSource, Secret, SendContext, StreamFrame, StreamMode,
    TlsSpec, TransportFactory,
};
use vc_mock_server::{ClientAuth, MockServer, MockTls};
use vc_model::RenderedRequest;
use vc_testkit::{MemoryEnvironment, TestPki};

type TestResult = Result<(), Box<dyn Error>>;

fn inline(pem: &str) -> MaterialSource {
    MaterialSource::Inline(Secret::new(pem))
}

#[tokio::test]
async fn the_built_transport_goes_through_the_proxy_the_environment_names() -> TestResult {
    let mock = MockServer::start().await?;
    let proxy = format!("http://{}", mock.addr());
    let environment = MemoryEnvironment::new(&[("HTTP_PROXY", proxy.as_str())]);
    let transport = HttpTransports::new().build(&TlsSpec::default(), &environment)?.transport;
    let request = RenderedRequest::new(Method::GET, Uri::try_from("http://api.invalid/echo")?);
    let exchange = transport.send(request, &SendContext::default()).await?;
    let echoed: serde_json::Value = serde_json::from_slice(&exchange.response.body)?;
    assert_eq!(echoed["headers"]["host"], "api.invalid", "{echoed}");
    Ok(())
}

#[tokio::test]
async fn the_built_transport_trusts_the_specs_roots_and_presents_its_identity() -> TestResult {
    let pki = TestPki::new()?;
    let (server, good) = (pki.server(), pki.good());
    let tls = MockTls::new(vec![server.cert_der().to_vec()], server.key_der().to_vec())
        .client_auth(ClientAuth::Required, vec![pki.ca_der().to_vec()])
        .name_client(good.name(), good.cert_der().to_vec());
    let mock = MockServer::start_tls(tls).await?;
    let material = ClientSource::Pem { cert: inline(good.cert_pem()), key: inline(good.key_pem()) };
    let client = ClientSpec { id: "good".to_owned(), hosts: Vec::new(), material };
    let spec = TlsSpec { ca: vec![inline(&pki.ca_pem())], clients: vec![client] };
    let ports = HttpTransports::new().build(&spec, &MemoryEnvironment::new(&[]))?;
    let transport = ports.transport;
    let request = RenderedRequest::new(Method::GET, Uri::try_from(format!("{}/whoami", mock.base_url()))?);
    let context = SendContext { client_cert: CertChoice::Named(CertId::new(0)), ..SendContext::default() };
    let exchange = transport.send(request, &context).await?;
    let answered: serde_json::Value = serde_json::from_slice(&exchange.response.body)?;
    assert_eq!(answered["client"], good.name(), "{answered}");
    Ok(())
}

#[tokio::test]
async fn the_built_stream_port_opens_a_body_stream() -> TestResult {
    let mock = MockServer::start().await?;
    let ports = HttpTransports::new().build(&TlsSpec::default(), &MemoryEnvironment::new(&[]))?;
    let streams = ports.streams.ok_or("no stream port")?;
    let request = RenderedRequest::new(Method::GET, Uri::try_from(format!("{}/events?count=1", mock.base_url()))?);
    let opened = streams.open(request, StreamMode::Body, &SendContext::default()).await?;
    let first = opened.stream.ok_or("no stream")?.next().await;
    assert_eq!(first, Some(Ok(StreamFrame::Chunk(bytes::Bytes::from_static(b"id: 1\ndata: 1\n\n")))));
    Ok(())
}

#[tokio::test]
async fn the_factory_counts_the_connections_its_transports_hold_open() -> TestResult {
    let mock = MockServer::start().await?;
    let factory = HttpTransports::new();
    assert_eq!(factory.open_connections(), Some(0));
    let transport = factory.build(&TlsSpec::default(), &MemoryEnvironment::new(&[]))?.transport;
    let request = RenderedRequest::new(Method::GET, Uri::try_from(format!("{}/echo", mock.base_url()))?);
    let _sent = transport.send(request.clone(), &SendContext::default()).await?;
    let _again = transport.send(request, &SendContext::default()).await?;
    assert_eq!(factory.open_connections(), Some(1), "the pooled connection is counted once, and reused");
    drop(transport);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while factory.open_connections() != Some(0) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(factory.open_connections(), Some(0), "a closed pool's connections are no longer counted");
    Ok(())
}
