//! Client certificates: PEM identities, refusals at construction, the per-identity pools and host
//! matching, against the https mock asking for an optional certificate; and refusals by a server
//! that requires one (V5127), on TLS 1.2 and 1.3.

use std::error::Error;

use http::{Method, StatusCode, Uri};
use vc_adapter_http::{ClientIdentity, HttpAdapterError, HttpOptions, HttpTransport, TlsOptions};
use vc_interfaces::{CertChoice, CertId, SendContext, SentCert, Transport, TransportError};
use vc_mock_server::{ClientAuth, MockHandle, MockServer, MockTls};
use vc_model::{Exchange, HttpVersion, RenderedRequest};
use vc_testkit::{ClientCert, Identity, KeyKind, TestPki};

type TestResult = Result<(), Box<dyn Error>>;

fn identity(id: &str, cert: &Identity) -> ClientIdentity {
    ClientIdentity::pem(id, cert.cert_pem().as_bytes().to_vec(), cert.key_pem().as_bytes().to_vec())
}

fn transport(pki: &TestPki, clients: Vec<ClientIdentity>) -> Result<HttpTransport, HttpAdapterError> {
    let tls = TlsOptions { ca: vec![pki.ca_pem().into_bytes()], clients };
    HttpTransport::new(&HttpOptions { tls, ..HttpOptions::default() })
}

/// The https mock's TLS setup: the test PKI's server certificate, asking for a client
/// certificate from its CA as `auth` says.
fn server_tls(pki: &TestPki, auth: ClientAuth) -> MockTls {
    let server = pki.server();
    MockTls::new(vec![server.cert_der().to_vec()], server.key_der().to_vec())
        .client_auth(auth, vec![pki.ca_der().to_vec()])
}

/// The https mock asking for an optional certificate, naming each of `clients` on `/whoami`.
async fn mock(pki: &TestPki, clients: &[&Identity]) -> Result<MockHandle, Box<dyn Error>> {
    let tls = server_tls(pki, ClientAuth::Optional);
    let tls = clients.iter().fold(tls, |tls, client| tls.name_client(client.name(), client.cert_der().to_vec()));
    Ok(MockServer::start_tls(tls).await?)
}

async fn whoami(transport: &HttpTransport, url: &str, client_cert: CertChoice) -> Result<Exchange, Box<dyn Error>> {
    let request = RenderedRequest::new(Method::GET, Uri::try_from(format!("{url}/whoami"))?);
    Ok(transport.send(request, &SendContext { client_cert, ..SendContext::default() }).await?)
}

fn client(exchange: &Exchange) -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(serde_json::from_slice::<serde_json::Value>(&exchange.response.body)?["client"].clone())
}

fn refusal(cert: &[u8], key: &[u8]) -> Result<String, Box<dyn Error>> {
    let pki = TestPki::new()?;
    match transport(&pki, vec![identity("ok", pki.good()), ClientIdentity::pem("bad", cert.to_vec(), key.to_vec())]) {
        Err(HttpAdapterError::Identity { id, reason }) if id == "bad" && !reason.contains("BEGIN") => Ok(reason),
        other => Err(format!("expected an identity error, got {other:?}").into()),
    }
}

#[test]
fn reads_rsa_and_ec_keys_in_each_pem_form() -> TestResult {
    let pki = TestPki::new()?;
    let rsa = include_bytes!("fixtures/rsa.pem").to_vec();
    let clients = vec![
        ClientIdentity::pem("rsaPkcs1", rsa.clone(), include_bytes!("fixtures/rsa.pkcs1.key").to_vec()),
        ClientIdentity::pem("rsaPkcs8", rsa, include_bytes!("fixtures/rsa.pkcs8.key").to_vec()),
        ClientIdentity::pem(
            "ecSec1",
            include_bytes!("fixtures/ec.pem").to_vec(),
            include_bytes!("fixtures/ec.sec1.key").to_vec(),
        ),
    ];
    transport(&pki, clients)?;
    Ok(())
}

#[test]
fn refuses_a_key_that_does_not_match_the_certificate() -> TestResult {
    let pki = TestPki::new()?;
    let other = pki.client(&ClientCert::new("other"))?;
    let reason = refusal(pki.good().cert_pem().as_bytes(), other.key_pem().as_bytes())?;
    assert!(reason.contains("mismatch") || reason.contains("consistent"), "{reason}");
    Ok(())
}

#[test]
fn refuses_an_encrypted_key_with_help() -> TestResult {
    let reason = refusal(include_bytes!("fixtures/rsa.pem"), include_bytes!("fixtures/rsa.encrypted.key"))?;
    assert!(reason.starts_with("the private key is encrypted; decrypt it"), "{reason}");
    Ok(())
}

#[tokio::test]
async fn sends_ec_p384_and_ed25519_identities() -> TestResult {
    let pki = TestPki::new()?;
    let p384 = pki.client(&ClientCert::new("p384").key(KeyKind::EcP384))?;
    let ed25519 = pki.client(&ClientCert::new("ed25519").key(KeyKind::Ed25519))?;
    let mock = mock(&pki, &[&p384, &ed25519]).await?;
    let transport = transport(&pki, vec![identity("p384", &p384), identity("ed25519", &ed25519)])?;
    for (index, name) in [(0, "p384"), (1, "ed25519")] {
        let exchange = whoami(&transport, &mock.base_url(), CertChoice::Named(CertId::new(index))).await?;
        assert_eq!(client(&exchange)?, name);
    }
    Ok(())
}

#[tokio::test]
async fn two_identities_on_one_host_get_a_handshake_and_a_pool_each() -> TestResult {
    let pki = TestPki::new()?;
    let tenant_b = pki.client(&ClientCert::new("tenantB"))?;
    let mock = mock(&pki, &[pki.good(), &tenant_b]).await?;
    let transport = transport(&pki, vec![identity("tenantA", pki.good()), identity("tenantB", &tenant_b)])?;
    let url = mock.base_url();
    let calls = [(0, "good", false), (1, "tenantB", false), (0, "good", true), (1, "tenantB", true)];
    for (index, name, reused) in calls {
        let exchange = whoami(&transport, &url, CertChoice::Named(CertId::new(index))).await?;
        assert_eq!((client(&exchange)?, exchange.connection.reused), (name.into(), reused), "{name}");
        let id = if index == 0 { "tenantA" } else { "tenantB" };
        assert_eq!(exchange.connection.client_cert.as_deref(), Some(id));
    }
    let none = whoami(&transport, &url, CertChoice::Off).await?;
    assert_eq!((none.response.status, none.connection.reused), (StatusCode::FORBIDDEN, false));
    assert_eq!(none.connection.client_cert, None);
    let unknown = whoami(&transport, &url, CertChoice::Named(CertId::new(2))).await.err();
    let unknown = unknown.and_then(|error| error.downcast::<TransportError>().ok());
    assert!(matches!(unknown.as_deref(), Some(TransportError::InvalidRequest { .. })), "{unknown:?}");
    Ok(())
}

#[tokio::test]
async fn by_host_sends_the_identity_only_to_the_matching_port() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, &[pki.good()]).await?;
    let port = mock.addr().port();
    let elsewhere = identity("elsewhere", pki.good()).hosts(vec![format!("127.0.0.1:{}", port.wrapping_add(1))]);
    let here = identity("here", pki.good()).hosts(vec![format!("127.0.0.1:{port}")]);
    let exchange = whoami(&transport(&pki, vec![elsewhere.clone(), here])?, &mock.base_url(), CertChoice::Host).await?;
    assert_eq!((client(&exchange)?, exchange.connection.client_cert.as_deref()), ("good".into(), Some("here")));
    let exchange = whoami(&transport(&pki, vec![elsewhere])?, &mock.base_url(), CertChoice::Host).await?;
    assert_eq!((exchange.response.status, exchange.connection.client_cert), (StatusCode::FORBIDDEN, None));
    Ok(())
}

/// A change to the mock's TLS setup.
type Setup = fn(MockTls) -> MockTls;

/// The servers that require a certificate: TLS 1.2 only, and TLS 1.3 over HTTP/1.1 and HTTP/2,
/// where the refusal arrives on the first read.
const SETUPS: [(&str, Setup); 3] =
    [("TLS 1.2", MockTls::tls12), ("TLS 1.3", std::convert::identity), ("TLS 1.3 and h2", MockTls::h2)];

/// A call's certificate choice, and the certificate configured as identity `sent`, if any.
type Call<'a> = (CertChoice, Option<&'a Identity>);

/// Sends identity `sent`, the only one configured.
const SENT: CertChoice = CertChoice::Named(CertId::new(0));

/// How a server that requires a certificate, set up by `setup`, answers `call`.
async fn refused(pki: &TestPki, setup: Setup, call: Call<'_>) -> Result<TransportError, Box<dyn Error>> {
    let mock = MockServer::start_tls(setup(server_tls(pki, ClientAuth::Required))).await?;
    let (choice, cert) = call;
    let transport = transport(pki, cert.map(|cert| identity("sent", cert)).into_iter().collect())?;
    let request = RenderedRequest::new(Method::GET, Uri::try_from(format!("{}/whoami", mock.base_url()))?);
    let outcome = transport.send(request, &SendContext { client_cert: choice, ..SendContext::default() }).await;
    outcome.err().ok_or_else(|| "the server let the call in".into())
}

#[tokio::test]
async fn a_certificate_from_another_ca_is_refused_as_unknown_ca_on_tls_1_2_and_1_3() -> TestResult {
    let pki = TestPki::new()?;
    for (name, setup) in SETUPS {
        let error = refused(&pki, setup, (SENT, Some(pki.other_ca()))).await?;
        let expected = TransportError::ClientCertRejected {
            host: "127.0.0.1".into(),
            alert: "unknown_ca".into(),
            reason: "it doesn't trust the issuer".into(),
            sent: SentCert::Named("sent".into()),
        };
        assert_eq!(error, expected, "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn no_certificate_is_refused_as_required_with_help_on_tls_1_2_and_1_3() -> TestResult {
    let pki = TestPki::new()?;
    for (name, setup) in SETUPS {
        let error = refused(&pki, setup, (CertChoice::Host, None)).await?;
        let expected = TransportError::ClientCertRejected {
            host: "127.0.0.1".into(),
            alert: "certificate_required".into(),
            reason: "it requires a client certificate".into(),
            sent: SentCert::Missing,
        };
        assert_eq!(error, expected, "{name}");
        let hint = "; no client certificate was sent: if the server requires one, add an `http.tls.clients` entry \
                    for `127.0.0.1`";
        assert!(error.to_string().ends_with(hint), "{error}");
    }
    Ok(())
}

#[tokio::test]
async fn certificates_turned_off_are_named_as_the_cause_of_a_refusal() -> TestResult {
    let pki = TestPki::new()?;
    for (name, setup) in SETUPS {
        let error = refused(&pki, setup, (CertChoice::Off, Some(pki.good()))).await?;
        assert!(matches!(&error, TransportError::ClientCertRejected { sent: SentCert::Off, .. }), "{name}: {error:?}");
        let text = error.to_string();
        assert!(text.ends_with("; client certificates are off for this call (`clientCert: none`)"), "{text}");
    }
    Ok(())
}

#[tokio::test]
async fn an_expired_certificate_is_refused_as_expired() -> TestResult {
    let pki = TestPki::new()?;
    let day = std::time::Duration::from_hours(24);
    let ended = std::time::SystemTime::now() - day;
    let old = pki.client(&ClientCert::new("old").valid(ended - day, ended))?;
    let error = refused(&pki, std::convert::identity, (SENT, Some(&old))).await?;
    assert!(
        matches!(&error, TransportError::ClientCertRejected { alert, .. } if alert == "certificate_expired"),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn sends_the_certificate_over_h2() -> TestResult {
    let pki = TestPki::new()?;
    let good = pki.good();
    let tls = server_tls(&pki, ClientAuth::Required).name_client(good.name(), good.cert_der().to_vec()).h2();
    let mock = MockServer::start_tls(tls).await?;
    let transport = transport(&pki, vec![identity("good", good)])?;
    let exchange = whoami(&transport, &mock.base_url(), CertChoice::Named(CertId::new(0))).await?;
    assert_eq!((client(&exchange)?, exchange.connection.protocol), ("good".into(), HttpVersion::Http2));
    Ok(())
}
