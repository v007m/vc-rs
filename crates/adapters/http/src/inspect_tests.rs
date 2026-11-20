use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vc_interfaces::{
    ClientSource, ClientSpec, MaterialSource, NEAR_NAMES, Secret, TlsFinding, TlsPart, TlsProbe, TlsSpec,
    TransportFactory as _,
};
use vc_testkit::{ClientCert, TestPki};

use super::inspect;
use crate::HttpTransports;

const CERT: &[u8] = include_bytes!("../tests/fixtures/ec.pem");
const KEY: &[u8] = include_bytes!("../tests/fixtures/ec.sec1.key");
const ENCRYPTED_KEY: &[u8] = include_bytes!("../tests/fixtures/rsa.encrypted.key");
const RSA_KEY: &[u8] = include_bytes!("../tests/fixtures/rsa.pkcs8.key");

/// The fixture certificate's validity: 2026-11-18 23:54:37 to 2126-10-25 23:54:37 UTC.
const FIXTURE_VALIDITY: (i64, i64) = (1_795_046_077, 4_948_646_077);

fn fixture(name: &str) -> MaterialSource {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    MaterialSource::File { path, shown: None }
}

fn file(path: &Path) -> MaterialSource {
    MaterialSource::File { path: path.to_owned(), shown: None }
}

fn inline(bytes: &[u8]) -> MaterialSource {
    MaterialSource::Inline(Secret::new(String::from_utf8_lossy(bytes)))
}

fn pem(cert: Option<MaterialSource>, key: Option<MaterialSource>) -> TlsProbe {
    TlsProbe::Pem { id: "tenantA".to_owned(), cert, key }
}

fn part(field: &'static str) -> TlsPart {
    TlsPart::Client { id: "tenantA".to_owned(), field: Some(field) }
}

fn validity(field: &'static str, (not_before, not_after): (i64, i64)) -> TlsFinding {
    TlsFinding::Validity { part: part(field), not_before, not_after }
}

fn at(seconds: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds.unsigned_abs())
}

#[test]
fn a_good_pair_and_a_bare_cert_give_only_the_leafs_validity() {
    let pair = pem(Some(inline(CERT)), Some(inline(KEY)));
    assert_eq!(inspect(&[pair]), [validity("cert", FIXTURE_VALIDITY)]);
    let bare = pem(Some(fixture("ec.pem")), None);
    assert_eq!(inspect(&[bare]), [validity("cert", FIXTURE_VALIDITY)], "a templated key leaves the cert to date");
    assert_eq!(inspect(&[pem(None, Some(inline(KEY)))]), [], "a key alone is only read");
}

#[test]
fn dates_the_leaf_of_a_certificate_issued_for_any_window() -> Result<(), Box<dyn std::error::Error>> {
    let window = (1_600_000_000, 1_700_000_000);
    let pki = TestPki::new()?;
    let expired = pki.client(&ClientCert::new("expired").valid(at(window.0), at(window.1)))?;
    let probe = pem(Some(inline(expired.cert_pem().as_bytes())), Some(inline(expired.key_pem().as_bytes())));
    assert_eq!(inspect(&[probe]), [validity("cert", window)]);
    let bundle = pki.pkcs12(&expired, "pass")?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("expired.p12");
    std::fs::write(&path, bundle)?;
    restrict(&path)?;
    let probe =
        TlsProbe::Pkcs12 { id: "tenantA".to_owned(), bundle: file(&path), passphrase: Some(Secret::new("pass")) };
    assert_eq!(inspect(&[probe]), [validity("pkcs12", window)], "a bundle's leaf is dated too");
    Ok(())
}

#[test]
fn material_that_cannot_be_used_names_its_part_and_never_its_text() {
    let not_pem = pem(Some(inline(b"not a certificate")), Some(inline(KEY)));
    let unusable = |field, reason: &str| TlsFinding::Unusable { part: part(field), reason: reason.to_owned() };
    assert_eq!(inspect(&[not_pem]), [unusable("cert", "the certificate has no CERTIFICATE block")]);
    let not_x509 = pem(Some(inline(b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n")), None);
    assert_eq!(inspect(&[not_x509]), [unusable("cert", "the certificate is not X.509")]);
    let ca = TlsProbe::Ca { index: 2, source: inline(b"-----BEGIN CERTIFICATE-----\n!!\n-----END CERTIFICATE-----\n") };
    let found = inspect(&[ca]);
    assert!(matches!(&found[..], [TlsFinding::Unusable { part: TlsPart::Ca(2), .. }]), "{found:?}");
    assert_eq!(inspect(&[TlsProbe::Ca { index: 0, source: inline(CERT) }]), [], "a good root has nothing to say");
}

#[test]
fn a_key_that_cannot_be_used_or_does_not_fit_is_the_keys() {
    let encrypted = inspect(&[pem(Some(inline(CERT)), Some(inline(ENCRYPTED_KEY)))]);
    assert!(
        matches!(&encrypted[..], [TlsFinding::Validity { .. }, TlsFinding::Unusable { part: p, reason }]
            if *p == part("key") && reason.starts_with("the private key is encrypted")),
        "{encrypted:?}"
    );
    let mismatched = inspect(&[pem(Some(inline(CERT)), Some(inline(RSA_KEY)))]);
    assert!(
        matches!(&mismatched[..], [TlsFinding::Validity { .. }, TlsFinding::Mismatch { part: p, .. }] if *p == part("key")),
        "{mismatched:?}"
    );
}

#[test]
fn a_bundle_is_decoded_only_with_a_known_passphrase() {
    let bundle = |passphrase: Option<&str>| TlsProbe::Pkcs12 {
        id: "tenantA".to_owned(),
        bundle: fixture("ec.p12"),
        passphrase: passphrase.map(Secret::new),
    };
    let found = inspect(&[bundle(None), bundle(Some("wrong")), bundle(Some("fixture"))]);
    let found: Vec<_> = found.into_iter().filter(|finding| !matches!(finding, TlsFinding::Exposed { .. })).collect();
    assert!(
        matches!(&found[..], [TlsFinding::Mismatch { part: p, .. }, TlsFinding::Validity { .. }] if *p == part("pkcs12")),
        "{found:?}"
    );
    let badcert = TlsProbe::Pkcs12 { id: "tenantA".to_owned(), bundle: fixture("ec.badcert.p12"), passphrase: None };
    let read = inspect(&[badcert]);
    assert!(read.iter().all(|finding| matches!(finding, TlsFinding::Exposed { .. })), "only read: {read:?}");
}

#[test]
fn a_missing_file_lists_its_folder_and_an_unreadable_one_does_not() -> std::io::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("tenant-b.pem"), CERT)?;
    std::fs::write(dir.path().join("tenant-a.key"), KEY)?;
    let found = inspect(&[TlsProbe::Ca { index: 0, source: file(&dir.path().join("tenant-a.pem")) }]);
    let [TlsFinding::Unreadable { part: TlsPart::Ca(0), reason, near }] = &found[..] else { panic!("{found:?}") };
    assert_eq!(near, &["tenant-a.key", "tenant-b.pem"], "sorted");
    assert!(reason.contains("No such file"), "{reason}");
    let folder = inspect(&[pem(Some(file(dir.path())), None)]);
    let [TlsFinding::Unreadable { reason, near, .. }] = &folder[..] else { panic!("{folder:?}") };
    assert_eq!((reason.as_str(), near.len()), ("it is not a regular file", 0));
    let nowhere = inspect(&[pem(Some(file(&dir.path().join("gone/cert.pem"))), None)]);
    assert!(matches!(&nowhere[..], [TlsFinding::Unreadable { near, .. }] if near.is_empty()), "{nowhere:?}");
    Ok(())
}

#[test]
fn a_missing_files_folder_is_listed_up_to_the_limit() -> std::io::Result<()> {
    let dir = tempfile::tempdir()?;
    for index in 0..=NEAR_NAMES {
        std::fs::write(dir.path().join(format!("{index}.pem")), "")?;
    }
    let found = inspect(&[TlsProbe::Ca { index: 0, source: file(&dir.path().join("gone.pem")) }]);
    let [TlsFinding::Unreadable { near, .. }] = &found[..] else { panic!("{found:?}") };
    assert_eq!(near.len(), NEAR_NAMES);
    Ok(())
}

#[cfg(unix)]
fn restrict(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_key_or_bundle_file_others_may_read_is_exposed() -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir()?;
    let (cert, key) = (dir.path().join("a.pem"), dir.path().join("a.key"));
    std::fs::write(&cert, CERT)?;
    std::fs::write(&key, KEY)?;
    for (path, mode) in [(&cert, 0o644), (&key, 0o640)] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    }
    let probe = || pem(Some(file(&cert)), Some(file(&key)));
    let exposed = TlsFinding::Exposed { part: part("key"), mode: 0o640 };
    assert_eq!(inspect(&[probe()]), [exposed, validity("cert", FIXTURE_VALIDITY)], "only the key's mode counts");
    for mode in [0o600, 0o400, 0o700] {
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(mode))?;
        assert_eq!(inspect(&[probe()]), [validity("cert", FIXTURE_VALIDITY)], "{mode:o}");
    }
    for mode in [0o604, 0o610, 0o601] {
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(mode))?;
        let found = inspect(&[probe()]);
        assert!(matches!(found[0], TlsFinding::Exposed { mode: m, .. } if m == mode), "{mode:o}: {found:?}");
    }
    Ok(())
}

#[test]
fn the_factory_inspects_and_a_built_spec_becomes_its_probes() {
    let spec = TlsSpec { ca: vec![inline(CERT)], clients: Vec::new() };
    let pair = ClientSource::Pem { cert: inline(CERT), key: inline(KEY) };
    let bundle = ClientSource::Pkcs12 { bundle: fixture("ec.p12"), passphrase: Secret::new("fixture") };
    let clients =
        [("a", pair), ("b", bundle)].map(|(id, material)| ClientSpec { id: id.into(), hosts: Vec::new(), material });
    let spec = TlsSpec { clients: clients.into(), ..spec };
    let probes = spec.into_probes();
    assert!(
        matches!(&probes[..], [
            TlsProbe::Ca { index: 0, .. },
            TlsProbe::Pem { id: a, cert: Some(_), key: Some(_) },
            TlsProbe::Pkcs12 { id: b, passphrase: Some(passphrase), .. },
        ] if a == "a" && b == "b" && passphrase.expose() == "fixture"),
        "{probes:?}"
    );
    let probe = pem(Some(inline(CERT)), None);
    assert_eq!(HttpTransports::new().inspect(&[probe]), [validity("cert", FIXTURE_VALIDITY)]);
}
