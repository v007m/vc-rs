//! The https mode, through a real rustls client: required and optional client certificates and
//! `GET /whoami`.

use std::error::Error;
use std::io;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use http_body_util::BodyExt;
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{AlertDescription, ClientConfig, RootCertStore};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use vc_mock_server::{ClientAuth, MockHandle, MockServer, MockTls};
use vc_testkit::{ClientCert, Identity, KeyKind, TestPki};

type TestResult = Result<(), Box<dyn Error>>;

async fn mock(pki: &TestPki, auth: ClientAuth) -> Result<MockHandle, Box<dyn Error>> {
    let server = pki.server();
    let tls = MockTls::new(vec![server.cert_der().to_vec()], server.key_der().to_vec())
        .client_auth(auth, vec![pki.ca_der().to_vec()])
        .name_client(pki.good().name(), pki.good().cert_der().to_vec());
    Ok(MockServer::start_tls(tls).await?)
}

/// A client that trusts the test CA and sends `cert`, if given.
fn client(pki: &TestPki, cert: Option<&Identity>) -> Result<TlsConnector, Box<dyn Error>> {
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(pki.ca_der().to_vec()))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions()?;
    let builder = builder.with_root_certificates(roots);
    let config = match cert {
        None => builder.with_no_client_auth(),
        Some(cert) => {
            let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_der().to_vec()));
            builder.with_client_auth_cert(vec![CertificateDer::from(cert.cert_der().to_vec())], key)?
        }
    };
    Ok(TlsConnector::from(Arc::new(config)))
}

async fn connect(mock: &MockHandle, connector: &TlsConnector) -> io::Result<TlsStream<TcpStream>> {
    let tcp = TcpStream::connect(mock.addr()).await?;
    connector.connect(ServerName::try_from("localhost").map_err(io::Error::other)?, tcp).await
}

async fn get(mock: &MockHandle, connector: &TlsConnector, path: &str) -> Result<(StatusCode, Vec<u8>), Box<dyn Error>> {
    let stream = TokioIo::new(connect(mock, connector).await?);
    let (mut sender, connection) = hyper::client::conn::http1::handshake(stream).await?;
    tokio::spawn(connection);
    let request = Request::get(path).header("host", "localhost").body(String::new())?;
    let response = sender.send_request(request).await?;
    let status = response.status();
    Ok((status, response.into_body().collect().await?.to_bytes().to_vec()))
}

async fn whoami(mock: &MockHandle, connector: &TlsConnector) -> Result<(StatusCode, Value), Box<dyn Error>> {
    let (status, body) = get(mock, connector, "/whoami").await?;
    Ok((status, serde_json::from_slice(&body)?))
}

/// The alert the server sent when it refused the handshake. Under TLS 1.3 the client finishes
/// first, so the alert arrives on the first read.
async fn refusal(mock: &MockHandle, connector: &TlsConnector) -> Option<AlertDescription> {
    let error = match connect(mock, connector).await {
        Err(error) => error,
        Ok(mut stream) => stream.read(&mut [0_u8; 16]).await.err()?,
    };
    match error.get_ref()?.downcast_ref::<rustls::Error>()? {
        rustls::Error::AlertReceived(alert) => Some(*alert),
        _ => None,
    }
}

#[tokio::test]
async fn required_mode_accepts_the_good_cert_and_names_it() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, ClientAuth::Required).await?;
    assert!(mock.base_url().starts_with("https://127.0.0.1:"), "{}", mock.base_url());
    let (status, body) = whoami(&mock, &client(&pki, Some(pki.good()))?).await?;
    assert_eq!((status, body), (StatusCode::OK, json!({ "client": "good" })));
    assert_eq!(mock.stats().await?.count("GET /whoami"), 1);
    Ok(())
}

#[tokio::test]
async fn required_mode_refuses_no_cert_at_the_handshake() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, ClientAuth::Required).await?;
    assert_eq!(refusal(&mock, &client(&pki, None)?).await, Some(AlertDescription::CertificateRequired));
    Ok(())
}

#[tokio::test]
async fn required_mode_refuses_a_cert_from_another_ca_at_the_handshake() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, ClientAuth::Required).await?;
    assert_eq!(refusal(&mock, &client(&pki, Some(pki.other_ca()))?).await, Some(AlertDescription::UnknownCA));
    Ok(())
}

#[tokio::test]
async fn required_mode_accepts_p384_and_refuses_expired_and_not_yet_valid_certs() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, ClientAuth::Required).await?;
    let p384 = pki.client(&ClientCert::new("p384").key(KeyKind::EcP384))?;
    assert_eq!(whoami(&mock, &client(&pki, Some(&p384))?).await?, (StatusCode::OK, json!({ "client": null })));
    let (now, day) = (SystemTime::now(), Duration::from_hours(24));
    let expired = pki.client(&ClientCert::new("expired").valid(now - 2 * day, now - day))?;
    let early = pki.client(&ClientCert::new("early").valid(now + day, now + 2 * day))?;
    for cert in [expired, early] {
        let alert = refusal(&mock, &client(&pki, Some(&cert))?).await;
        assert_eq!(alert, Some(AlertDescription::CertificateExpired), "{}", cert.name());
    }
    Ok(())
}

#[tokio::test]
async fn optional_mode_answers_403_without_a_cert_and_names_one_when_sent() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, ClientAuth::Optional).await?;
    let (status, body) = whoami(&mock, &client(&pki, None)?).await?;
    assert_eq!((status, body), (StatusCode::FORBIDDEN, json!({ "error": "no client certificate" })));
    assert_eq!(whoami(&mock, &client(&pki, Some(pki.good()))?).await?.1, json!({ "client": "good" }));
    let unnamed = pki.client(&ClientCert::new("unnamed").key(KeyKind::Ed25519))?;
    assert_eq!(whoami(&mock, &client(&pki, Some(&unnamed))?).await?.1, json!({ "client": null }));
    assert_eq!(refusal(&mock, &client(&pki, Some(pki.other_ca()))?).await, Some(AlertDescription::UnknownCA));
    Ok(())
}

#[tokio::test]
async fn the_https_mode_serves_the_usual_routes_and_counts_them() -> TestResult {
    let pki = TestPki::new()?;
    let mock = mock(&pki, ClientAuth::Required).await?;
    let (status, _) = get(&mock, &client(&pki, Some(pki.good()))?, "/echo?a=1").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mock.stats().await?.count("GET /echo"), 1);
    mock.shutdown().await?;
    Ok(())
}
