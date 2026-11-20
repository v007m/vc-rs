//! `HttpTransports` against `vc-mock-server`: the transport it builds goes through the proxy the
//! environment names, trusts the spec's roots and presents its identities.

use std::error::Error;

use http::{Method, Uri};
use vc_adapter_http::HttpTransports;
use vc_interfaces::{
    CertChoice, CertId, ClientSource, ClientSpec, MaterialSource, Secret, SendContext, TlsSpec, TransportFactory,
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
    let transport = HttpTransports::new().build(&TlsSpec::default(), &environment)?;
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
    let transport = HttpTransports::new().build(&spec, &MemoryEnvironment::new(&[]))?;
    let request = RenderedRequest::new(Method::GET, Uri::try_from(format!("{}/whoami", mock.base_url()))?);
    let context = SendContext { client_cert: CertChoice::Named(CertId::new(0)), ..SendContext::default() };
    let exchange = transport.send(request, &context).await?;
    let answered: serde_json::Value = serde_json::from_slice(&exchange.response.body)?;
    assert_eq!(answered["client"], good.name(), "{answered}");
    Ok(())
}
