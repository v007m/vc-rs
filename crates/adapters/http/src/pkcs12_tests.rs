use std::time::{Duration, Instant};

use p12_keystore::error::Error as P12Error;
use p12_keystore::{
    Certificate, CertificateBag, KeyStore, KeyStoreEntry, LocalKeyId, Pkcs12Archive, PrivateKey, PrivateKeyBag,
    PrivateKeyChain,
};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vc_testkit::TestPki;

use super::{Reason, decode, from_archive, has_mac};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const RSA_CERT: &[u8] = include_bytes!("../tests/fixtures/rsa.pem");
const RSA_ALT_CERT: &[u8] = include_bytes!("../tests/fixtures/rsa.alt.pem");
const RSA_KEY: &[u8] = include_bytes!("../tests/fixtures/rsa.pkcs8.key");
const EC_CERT: &[u8] = include_bytes!("../tests/fixtures/ec.pem");

fn reason_for(bytes: &[u8], password: &str) -> Option<Reason> {
    decode(bytes, password).err()
}

fn pem_cert(pem: &[u8]) -> TestResult<Certificate> {
    Ok(Certificate::from_der(&CertificateDer::from_pem_slice(pem)?)?)
}

fn rsa_key() -> TestResult<PrivateKey> {
    Ok(PrivateKey::from_der(PrivateKeyDer::from_pem_slice(RSA_KEY)?.secret_der())?)
}

fn bundle(entries: Vec<KeyStoreEntry>) -> TestResult<Vec<u8>> {
    let mut store = KeyStore::new();
    for (index, entry) in entries.into_iter().enumerate() {
        store.add_entry(&format!("entry{index}"), entry);
    }
    Ok(store.writer("pw").write()?)
}

fn cert_bag(cert: &Certificate, local_key_id: Option<&[u8]>, trusted: bool) -> CertificateBag {
    CertificateBag { friendly_name: None, local_key_id: local_key_id.map(<[u8]>::to_vec), trusted, cert: cert.clone() }
}

/// The chain `from_archive` sends for `key` (with `key_id`) and `certs`, as DER.
fn chain_for(key: PrivateKey, key_id: Option<&[u8]>, certs: Vec<CertificateBag>) -> Result<Vec<Vec<u8>>, Reason> {
    let local_key_id = key_id.map(|id| LocalKeyId(id.to_vec()));
    let keys = vec![PrivateKeyBag { friendly_name: None, local_key_id, key }];
    let decoded = from_archive(&Pkcs12Archive { keys, certs, secrets: Vec::new() })?;
    Ok(decoded.chain.iter().map(|der| der.to_vec()).collect())
}

#[test]
fn decodes_openssl_bundles_with_aes_3des_and_rc2_and_without_a_mac() -> Result<(), Reason> {
    for bytes in [
        &include_bytes!("../tests/fixtures/ec.p12")[..],
        include_bytes!("../tests/fixtures/ec.legacy.p12"),
        include_bytes!("../tests/fixtures/ec.nomac.p12"),
    ] {
        let decoded = decode(bytes, "fixture")?;
        assert_eq!(decoded.chain.len(), 1);
        assert!(!decoded.key.is_empty());
    }
    Ok(())
}

#[test]
fn says_what_is_wrong_without_quoting_the_bundle() {
    let aes = include_bytes!("../tests/fixtures/ec.p12");
    assert_eq!(reason_for(aes, "wrong"), Some(Reason::WrongPassphrase));
    assert_eq!(reason_for(include_bytes!("../tests/fixtures/ec.rc4.p12"), "fixture"), Some(Reason::Unsupported));
    assert_eq!(reason_for(b"fixture", "fixture"), Some(Reason::NotPkcs12));
    assert_eq!(reason_for(include_bytes!("../tests/fixtures/ec.nomac.p12"), "wrong"), Some(Reason::Undecryptable));
}

#[test]
fn tells_a_bundle_without_a_mac() {
    assert!(has_mac(include_bytes!("../tests/fixtures/ec.p12")));
    assert!(!has_mac(include_bytes!("../tests/fixtures/ec.nomac.p12")));
    assert!(has_mac(&[0x31, 0x06, 0x02, 0x01, 0x03, 0x02, 0x01, 0x00]), "a SET is not a PFX");
    assert!(has_mac(b"fixture"));
}

#[test]
fn refuses_a_passphrase_pkcs12_cannot_encode() {
    let aes = include_bytes!("../tests/fixtures/ec.p12");
    assert_eq!(reason_for(aes, "fixture\u{1F512}"), Some(Reason::NonBmpPassphrase));
    assert_eq!(reason_for(aes, "fixture\u{FFFF}"), Some(Reason::NonBmpPassphrase));
    assert_eq!(reason_for(aes, "fixture\u{FFFE}"), Some(Reason::WrongPassphrase));
}

#[test]
fn refuses_a_bundle_over_the_iteration_limit_quickly() {
    let started = Instant::now();
    assert_eq!(reason_for(include_bytes!("../tests/fixtures/ec.slow.p12"), "fixture"), Some(Reason::OverLimit));
    assert!(started.elapsed() < Duration::from_secs(1), "took {:?}", started.elapsed());
}

#[test]
fn wants_exactly_one_key_and_a_certificate() -> TestResult {
    let (cert, key) = (pem_cert(RSA_CERT)?, rsa_key()?);
    let chain =
        |certs: Vec<Certificate>| KeyStoreEntry::PrivateKeyChain(PrivateKeyChain::new("id", key.clone(), certs));
    assert_eq!(reason_for(&bundle(vec![KeyStoreEntry::Certificate(cert.clone())])?, "pw"), Some(Reason::NoKey));
    assert_eq!(reason_for(&bundle(vec![chain(vec![])])?, "pw"), Some(Reason::NoCertificate));
    let two = bundle(vec![chain(vec![cert.clone()]), chain(vec![cert])])?;
    assert_eq!(reason_for(&two, "pw"), Some(Reason::Keys(2)));
    Ok(())
}

#[test]
fn picks_the_leaf_by_its_public_key_without_a_local_key_id() -> TestResult {
    let pki = TestPki::new()?;
    let (ca, leaf) = (Certificate::from_der(pki.ca_der())?, Certificate::from_der(pki.good().cert_der())?);
    let key = PrivateKey::from_der(pki.good().key_der())?;
    let chain = chain_for(key, None, vec![cert_bag(&ca, None, false), cert_bag(&leaf, None, false)])?;
    assert_eq!(chain, [leaf.as_der(), ca.as_der()]);
    Ok(())
}

#[test]
fn breaks_a_public_key_tie_by_local_key_id_then_bag_order() -> TestResult {
    let (ec, rsa, alt) = (pem_cert(EC_CERT)?, pem_cert(RSA_CERT)?, pem_cert(RSA_ALT_CERT)?);
    let certs =
        || vec![cert_bag(&ec, Some(b"k"), false), cert_bag(&rsa, None, false), cert_bag(&alt, Some(b"k"), false)];
    assert_eq!(chain_for(rsa_key()?, Some(b"k"), certs())?, [alt.as_der()]);
    assert_eq!(chain_for(rsa_key()?, Some(b"z"), certs())?, [rsa.as_der()]);
    assert_eq!(chain_for(rsa_key()?, None, certs())?, [rsa.as_der()]);
    Ok(())
}

#[test]
fn falls_back_to_the_local_key_id_then_the_first_bag_when_no_public_key_matches() -> TestResult {
    let (ec, rsa) = (pem_cert(EC_CERT)?, pem_cert(RSA_CERT)?);
    let certs = || vec![cert_bag(&ec, None, false), cert_bag(&rsa, Some(b"k"), false)];
    let pki = TestPki::new()?;
    let key = || PrivateKey::from_der(pki.good().key_der());
    assert_eq!(chain_for(key()?, Some(b"k"), certs())?, [rsa.as_der()]);
    assert_eq!(chain_for(key()?, None, certs())?, [ec.as_der()]);
    Ok(())
}

#[test]
fn sends_only_the_leafs_issuers_and_never_a_trusted_entry() -> TestResult {
    let pki = TestPki::new()?;
    let (ca, leaf) = (Certificate::from_der(pki.ca_der())?, Certificate::from_der(pki.good().cert_der())?);
    let unrelated = pem_cert(RSA_CERT)?;
    let key = || PrivateKey::from_der(pki.good().key_der());
    let certs =
        |trusted| vec![cert_bag(&unrelated, None, false), cert_bag(&ca, None, trusted), cert_bag(&leaf, None, false)];
    assert_eq!(chain_for(key()?, None, certs(true))?, [leaf.as_der()]);
    assert_eq!(chain_for(key()?, None, certs(false))?, [leaf.as_der(), ca.as_der()]);
    Ok(())
}

#[test]
fn stops_a_chain_that_comes_back_round() -> TestResult {
    let (a, b) = (
        pem_cert(include_bytes!("../tests/fixtures/cycle.a.pem"))?,
        pem_cert(include_bytes!("../tests/fixtures/cycle.b.pem"))?,
    );
    let key = PrivateKey::from_der(
        PrivateKeyDer::from_pem_slice(include_bytes!("../tests/fixtures/cycle.key"))?.secret_der(),
    )?;
    let chain = chain_for(key, None, vec![cert_bag(&b, None, false), cert_bag(&a, None, false)])?;
    assert_eq!(chain, [a.as_der(), b.as_der()]);
    Ok(())
}

#[test]
fn names_each_other_failure() {
    let cases = [
        (P12Error::UnpadError, true, Reason::Unsupported),
        (P12Error::Pkcs5Error("x".into()), true, Reason::Unsupported),
        (P12Error::UnsupportedMacAlgorithm, true, Reason::Unsupported),
        (P12Error::UnsupportedMacAlgorithm, false, Reason::Unsupported),
        (P12Error::InvalidParameters, false, Reason::OverLimit),
        (P12Error::InvalidPrivateKey, true, Reason::NotPkcs8),
        (P12Error::UnsupportedCertificateType, true, Reason::NotX509),
        (P12Error::InvalidData, true, Reason::NotPkcs12),
        (P12Error::InvalidVersion, true, Reason::NotPkcs12),
        (P12Error::UnpadError, false, Reason::Undecryptable),
        (P12Error::InvalidData, false, Reason::Undecryptable),
        (P12Error::InvalidPrivateKey, false, Reason::Undecryptable),
        (P12Error::UnsupportedCertificateType, false, Reason::Undecryptable),
        (P12Error::InvalidVersion, false, Reason::NotPkcs12),
    ];
    for (error, has_mac, reason) in cases {
        assert_eq!(Reason::of(&error, has_mac), reason, "{error:?} with a MAC: {has_mac}");
    }
}

#[test]
fn gives_each_reason_one_message() {
    let starts = [
        (
            Reason::NonBmpPassphrase,
            "the passphrase has a character PKCS#12 cannot encode (one outside the Basic Multilingual",
        ),
        (Reason::WrongPassphrase, "the passphrase is wrong"),
        (Reason::Undecryptable, "could not decrypt the bundle: check the passphrase"),
        (Reason::Unsupported, "the bundle uses a cipher or MAC that is not supported; export it again with AES"),
        (Reason::OverLimit, "unsupported or over-limit key-derivation parameters (iterations, scrypt cost or missing"),
        (Reason::NotPkcs8, "the bundle's private key is not valid PKCS#8"),
        (Reason::NotX509, "the bundle holds a certificate that is not valid X.509"),
        (Reason::NotPkcs12, "the file is not a valid PKCS#12 bundle"),
        (Reason::NoKey, "the bundle has no private key"),
        (Reason::Keys(3), "the bundle holds 3 private keys; export only the one to send"),
        (Reason::NoCertificate, "the bundle has no certificate"),
    ];
    for (reason, start) in starts {
        assert!(reason.to_string().starts_with(start), "{reason}");
    }
    assert!(Reason::OverLimit.to_string().ends_with("-macalg sha256`)"));
}
