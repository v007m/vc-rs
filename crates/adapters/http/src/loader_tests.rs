use std::path::PathBuf;

use vc_interfaces::{ClientSource, ClientSpec, MaterialSource, Secret, TlsError, TlsLoader as _, TlsPart, TlsSpec};

use super::HttpTlsLoader;
use crate::TlsOptions;

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

fn load(spec: TlsSpec) -> Result<TlsOptions, TlsError> {
    HttpTlsLoader.load(spec).map(|loaded| loaded.take::<TlsOptions>().unwrap_or_default())
}

#[test]
fn passes_the_tls_loader_suite() {
    vc_testkit::tls_loader_suite(&HttpTlsLoader);
}

#[test]
fn reads_files_and_inline_pem_into_the_transports_options() -> Result<(), TlsError> {
    let pem = client("pem", ClientSource::Pem { cert: fixture("ec.pem"), key: inline(KEY) });
    let spec =
        TlsSpec { ca: vec![inline(RSA), fixture("ec.pem")], clients: vec![pem, client("p12", bundle("fixture"))] };
    let options = load(spec)?;
    assert_eq!(options.ca, [RSA.to_vec(), CERT.to_vec()]);
    let clients: Vec<_> = options.clients.iter().map(|identity| (identity.id(), identity.host_rules())).collect();
    assert_eq!(clients, [("pem", &["pem.test".to_owned()][..]), ("p12", &["p12.test".to_owned()][..])]);
    assert_eq!(
        options.clients[0],
        crate::ClientIdentity::pem("pem", CERT.to_vec(), KEY.to_vec()).hosts(vec!["pem.test".into()])
    );
    Ok(())
}

#[test]
fn names_the_part_and_only_a_shown_path() {
    let gone = |shown: Option<&str>| MaterialSource::File {
        path: PathBuf::from("/nowhere/gone.pem"),
        shown: shown.map(str::to_owned),
    };
    let unusable = |part, file| (part, format!("cannot read {file}: No such file or directory (os error 2)"));
    let reason = |spec| match load(spec) {
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

#[test]
fn a_wrong_passphrase_is_a_mismatch_and_a_bad_bundle_unusable() {
    let part = TlsPart::Client { id: "bank".into(), field: Some("pkcs12") };
    let wrong = load(TlsSpec { clients: vec![client("bank", bundle("wrong"))], ..TlsSpec::default() });
    assert!(matches!(&wrong, Err(TlsError::Mismatch { part: at, .. }) if *at == part), "{wrong:?}");
    let pem = ClientSource::Pkcs12 { bundle: inline(CERT), passphrase: Secret::new("") };
    let bad = load(TlsSpec { clients: vec![client("bank", pem)], ..TlsSpec::default() });
    assert!(matches!(&bad, Err(TlsError::Unusable { part: at, .. }) if *at == part), "{bad:?}");
}

#[test]
fn checks_each_ca_entry_and_that_a_pem_key_fits_its_certificate() {
    let ca = load(TlsSpec { ca: vec![inline(CERT), inline(b"not a certificate")], ..TlsSpec::default() });
    let unusable = TlsError::Unusable { part: TlsPart::Ca(1), reason: "no certificate in it parses".into() };
    assert_eq!(ca.err(), Some(unusable));
    let other = include_bytes!("../tests/fixtures/rsa.pkcs8.key");
    let pem = client("good", ClientSource::Pem { cert: inline(CERT), key: inline(other) });
    let mismatch = load(TlsSpec { clients: vec![pem], ..TlsSpec::default() });
    let part = TlsPart::Client { id: "good".into(), field: None };
    assert!(matches!(&mismatch, Err(TlsError::Mismatch { part: at, .. }) if *at == part), "{mismatch:?}");
    let pem = client("good", ClientSource::Pem { cert: inline(b"no pem"), key: inline(KEY) });
    let unusable = load(TlsSpec { clients: vec![pem], ..TlsSpec::default() });
    assert!(matches!(&unusable, Err(TlsError::Unusable { part: at, .. }) if *at == part), "{unusable:?}");
}
