use std::path::PathBuf;
use std::sync::Arc;

use vc_interfaces::{
    ClientSource, ClientSpec, MaterialSource, Secret, TlsError, TlsPart, TlsSpec, Transport, TransportFactory as _,
};
use vc_testkit::{FakeTransport, MemoryEnvironment, status};

use super::{HttpTransports, parsed, refused};
use crate::{ClientIdentity, HttpAdapterError};

const CERT: &[u8] = include_bytes!("../tests/fixtures/ec.pem");
const KEY: &[u8] = include_bytes!("../tests/fixtures/ec.sec1.key");
const RSA: &[u8] = include_bytes!("../tests/fixtures/rsa.pem");

fn fixture(name: &str) -> MaterialSource {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    MaterialSource::File { path, shown: Some(format!("certs/{name}")) }
}

fn inline(bytes: &[u8]) -> MaterialSource {
    MaterialSource::Inline(Secret::new(String::from_utf8_lossy(bytes)))
}

fn bundle(passphrase: &str) -> ClientSource {
    ClientSource::Pkcs12 { bundle: fixture("ec.p12"), passphrase: Secret::new(passphrase) }
}

fn client(id: &str, material: ClientSource) -> ClientSpec {
    ClientSpec { id: id.to_owned(), hosts: vec![format!("{id}.test")], material }
}

/// The transport `factory` builds for `spec`, or its refusal.
fn build_with(factory: &HttpTransports, spec: &TlsSpec) -> Result<Arc<dyn Transport>, TlsError> {
    factory.build(spec, &MemoryEnvironment::new(&[])).map(|ports| ports.transport)
}

/// Whether [`HttpTransports::new`] builds a transport for `spec`, or its refusal.
fn build(spec: &TlsSpec) -> Result<(), TlsError> {
    build_with(&HttpTransports::new(), spec).map(|_| ())
}

#[tokio::test]
async fn passes_the_transport_factory_suite() {
    vc_testkit::transport_factory_suite(&HttpTransports::new());
    vc_testkit::transport_factory_suite(&HttpTransports::fixed(Arc::new(FakeTransport::new(|_, _| Ok(status(200))))));
}

#[tokio::test]
async fn reads_files_and_inline_pem_and_parses_each_part_once() -> Result<(), TlsError> {
    let pem = client("pem", ClientSource::Pem { cert: fixture("ec.pem"), key: inline(KEY) });
    let spec =
        TlsSpec { ca: vec![inline(RSA), fixture("ec.pem")], clients: vec![pem, client("p12", bundle("fixture"))] };
    let (roots, checked) = parsed(&spec)?;
    let clients = checked.identities();
    assert_eq!(roots.len(), 2, "each `ca` entry parsed into a root");
    let hosts: Vec<_> = clients.iter().map(|identity| (identity.id(), identity.host_rules())).collect();
    assert_eq!(hosts, [("pem", &["pem.test".to_owned()][..]), ("p12", &["p12.test".to_owned()][..])]);
    let want = ClientIdentity::pem("pem", CERT.to_vec(), KEY.to_vec()).hosts(vec!["pem.test".into()]).parsed();
    assert_eq!(Ok(clients[0]), want.as_ref(), "the PEM pair is parsed into its signer");
    build(&spec)
}

#[tokio::test]
async fn a_fixed_factory_checks_the_material_then_gives_its_transport() {
    let fixed: Arc<dyn Transport> = Arc::new(FakeTransport::new(|_, _| Ok(status(200))));
    let factory = HttpTransports::fixed(Arc::clone(&fixed));
    let given = build_with(&factory, &TlsSpec { ca: vec![inline(RSA)], ..TlsSpec::default() });
    assert!(given.is_ok_and(|given| Arc::ptr_eq(&given, &fixed)), "the injected transport, not a built one");
    let wrong = build_with(&factory, &TlsSpec { clients: vec![client("bank", bundle("wrong"))], ..TlsSpec::default() });
    assert!(matches!(wrong, Err(TlsError::Mismatch { .. })), "V5126 still stops the run");
    assert!(format!("{factory:?}").contains("fixed: true"));
}

#[tokio::test]
async fn a_built_transport_is_also_the_stream_port_and_a_fixed_one_has_none() {
    let environment = MemoryEnvironment::new(&[]);
    let built = HttpTransports::new().build(&TlsSpec::default(), &environment);
    assert!(built.is_ok_and(|ports| ports.streams.is_some()), "sse calls go over the wire (D428)");
    let fixed = HttpTransports::fixed(Arc::new(FakeTransport::new(|_, _| Ok(status(200)))));
    let fixed = fixed.build(&TlsSpec::default(), &environment);
    assert!(fixed.is_ok_and(|ports| ports.streams.is_none()), "an injected transport opens no stream");
}

#[tokio::test]
async fn names_the_part_and_only_a_shown_path() {
    let gone = |shown: Option<&str>| MaterialSource::File {
        path: PathBuf::from("/nowhere/gone.pem"),
        shown: shown.map(str::to_owned),
    };
    let missing = std::fs::read("/nowhere/gone.pem").err().map(|error| error.to_string()).unwrap_or_default();
    let unusable = |part, file| (part, format!("cannot read {file}: {missing}"));
    let reason = |spec| match build(&spec) {
        Err(TlsError::Unusable { part, reason }) => Some((part, reason)),
        _ => None,
    };
    let ca = TlsSpec { ca: vec![inline(RSA), gone(Some("certs/gone.pem"))], ..TlsSpec::default() };
    assert_eq!(reason(ca), Some(unusable(TlsPart::Ca(1), "`certs/gone.pem`")));
    let key = client("good", ClientSource::Pem { cert: inline(CERT), key: gone(None) });
    let key_part = TlsPart::Client { id: "good".into(), field: Some("key") };
    assert_eq!(reason(TlsSpec { clients: vec![key], ..TlsSpec::default() }), Some(unusable(key_part, "its file")));
    let cert = client("good", ClientSource::Pem { cert: gone(None), key: inline(KEY) });
    let cert_part = TlsPart::Client { id: "good".into(), field: Some("cert") };
    assert_eq!(reason(TlsSpec { clients: vec![cert], ..TlsSpec::default() }), Some(unusable(cert_part, "its file")));
}

#[tokio::test]
async fn a_wrong_passphrase_is_a_mismatch_and_a_bad_bundle_unusable() {
    let part = TlsPart::Client { id: "bank".into(), field: Some("pkcs12") };
    let wrong = build(&TlsSpec { clients: vec![client("bank", bundle("wrong"))], ..TlsSpec::default() });
    assert!(matches!(&wrong, Err(TlsError::Mismatch { part: at, .. }) if *at == part), "{wrong:?}");
    let pem = ClientSource::Pkcs12 { bundle: inline(CERT), passphrase: Secret::new("") };
    let bad = build(&TlsSpec { clients: vec![client("bank", pem)], ..TlsSpec::default() });
    assert!(matches!(&bad, Err(TlsError::Unusable { part: at, .. }) if *at == part), "{bad:?}");
}

#[tokio::test]
async fn checks_each_ca_entry_that_a_pem_key_fits_and_each_host_rule() {
    let ca = build(&TlsSpec { ca: vec![inline(CERT), inline(b"not a certificate")], ..TlsSpec::default() });
    let unusable = TlsError::Unusable { part: TlsPart::Ca(1), reason: "no certificate in it parses".into() };
    assert_eq!(ca.err(), Some(unusable));
    let other = include_bytes!("../tests/fixtures/rsa.pkcs8.key");
    let pem = client("good", ClientSource::Pem { cert: inline(CERT), key: inline(other) });
    let mismatch = build(&TlsSpec { clients: vec![pem], ..TlsSpec::default() });
    let part = TlsPart::Client { id: "good".into(), field: None };
    assert!(matches!(&mismatch, Err(TlsError::Mismatch { part: at, .. }) if *at == part), "{mismatch:?}");
    let pem = client("good", ClientSource::Pem { cert: inline(b"no pem"), key: inline(KEY) });
    let unusable = build(&TlsSpec { clients: vec![pem], ..TlsSpec::default() });
    assert!(matches!(&unusable, Err(TlsError::Unusable { part: at, .. }) if *at == part), "{unusable:?}");
    let mut hosts = client("good", ClientSource::Pem { cert: inline(CERT), key: inline(KEY) });
    hosts.hosts = vec!["https://a.test/".into()];
    let fixed = HttpTransports::fixed(Arc::new(FakeTransport::new(|_, _| Ok(status(200)))));
    let rule = build_with(&fixed, &TlsSpec { clients: vec![hosts], ..TlsSpec::default() }).map(|_| ());
    let part = TlsPart::Client { id: "good".into(), field: Some("hosts") };
    assert!(matches!(&rule, Err(TlsError::Unusable { part: at, .. }) if *at == part), "{rule:?}");
}

#[test]
fn an_error_no_part_causes_is_a_transport_that_cannot_be_built() {
    let error = refused(HttpAdapterError::NoHost("x".into()), Some("key"));
    assert_eq!(error, TlsError::Transport { reason: "the URL x has no host".into() });
    let identity = refused(HttpAdapterError::Identity { id: "a".into(), reason: "r".into() }, Some("pkcs12"));
    let part = TlsPart::Client { id: "a".into(), field: Some("pkcs12") };
    assert_eq!(identity, TlsError::Unusable { part, reason: "r".into() });
}
