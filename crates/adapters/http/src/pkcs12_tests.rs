use std::time::{Duration, Instant};

use p12_keystore::error::Error as P12Error;
use p12_keystore::{
    Certificate, CertificateBag, KeyStore, KeyStoreEntry, LocalKeyId, Pkcs12Archive, PrivateKey, PrivateKeyBag,
    PrivateKeyChain,
};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vc_testkit::TestPki;

use super::{Reason, decode, from_archive};
use crate::pkcs12_reason::Layout;

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

pub fn cert_bag(cert: &Certificate, local_key_id: Option<&[u8]>, trusted: bool) -> CertificateBag {
    CertificateBag { friendly_name: None, local_key_id: local_key_id.map(<[u8]>::to_vec), trusted, cert: cert.clone() }
}

/// The chain `from_archive` sends for `key` (with `key_id`) and `certs`, as DER.
pub fn chain_for(key: PrivateKey, key_id: Option<&[u8]>, certs: Vec<CertificateBag>) -> Result<Vec<Vec<u8>>, Reason> {
    let local_key_id = key_id.map(|id| LocalKeyId(id.to_vec()));
    let keys = vec![PrivateKeyBag { friendly_name: None, local_key_id, key }];
    let decoded = from_archive(&Pkcs12Archive { keys, certs, secrets: Vec::new() })?;
    Ok(decoded.certified.cert.iter().map(|der| der.to_vec()).collect())
}

/// `der` as BER: the outer `SEQUENCE` given an indefinite length, closed by end-of-contents.
fn ber(der: &[u8]) -> Vec<u8> {
    let skip = if der[1] < 0x80 { 2 } else { 2 + usize::from(der[1] & 0x7F) };
    [&[0x30, 0x80][..], &der[skip..], &[0, 0]].concat()
}

#[test]
fn gives_a_neutral_reason_for_a_bundle_that_is_not_der() {
    let ber = ber(include_bytes!("../tests/fixtures/ec.p12"));
    assert_eq!(reason_for(&ber, "fixture"), Some(Reason::Unreadable));
}

#[test]
fn decodes_openssl_bundles_with_aes_3des_and_rc2_and_without_a_mac() -> Result<(), Reason> {
    for bytes in [
        &include_bytes!("../tests/fixtures/ec.p12")[..],
        include_bytes!("../tests/fixtures/ec.legacy.p12"),
        include_bytes!("../tests/fixtures/ec.nomac.p12"),
    ] {
        let decoded = decode(bytes, "fixture")?;
        assert_eq!(decoded.certified.cert.len(), 1);
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
fn tells_a_bag_that_does_not_decrypt_from_a_bad_bag_that_was_never_encrypted() {
    let twopass = include_bytes!("../tests/fixtures/ec.twopass.p12");
    assert_eq!(reason_for(twopass, "fixture"), Some(Reason::BagUndecryptable));
    assert_eq!(reason_for(twopass, "encrypt"), Some(Reason::WrongPassphrase));
    let bad_cert = include_bytes!("../tests/fixtures/ec.badcert.p12");
    assert_eq!(reason_for(bad_cert, "fixture"), Some(Reason::NotX509));
    assert_eq!(reason_for(bad_cert, "wrong"), Some(Reason::NotX509));
}

#[test]
fn reads_what_a_bundle_holds() {
    let layout = |mac, encrypted_safe, shrouded_key| Some(Layout { mac, encrypted_safe, shrouded_key });
    let unknown = None;
    let ber = ber(include_bytes!("../tests/fixtures/ec.p12"));
    let cases = [
        (&include_bytes!("../tests/fixtures/ec.p12")[..], layout(true, true, true)),
        (include_bytes!("../tests/fixtures/ec.legacy.p12"), layout(true, true, true)),
        (include_bytes!("../tests/fixtures/ec.nomac.p12"), layout(false, false, true)),
        (include_bytes!("../tests/fixtures/ec.badcert.p12"), layout(false, false, false)),
        (&[0x31, 0x06, 0x02, 0x01, 0x03, 0x02, 0x01, 0x00], layout(true, false, false)),
        (b"fixture", layout(true, false, false)),
        (&ber, unknown),
        (&[0x30, 0x03, 0x02, 0x01, 0x03], unknown),
    ];
    for (index, (bytes, expected)) in cases.into_iter().enumerate() {
        assert_eq!(Layout::of(bytes), expected, "case {index}");
    }
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
    assert_eq!(chain, [leaf.as_der()]);
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
fn takes_a_leaf_that_encodes_the_key_another_way_once_it_verifies_the_keys_signature() -> TestResult {
    let explicit = decode(include_bytes!("../tests/fixtures/ec.explicit.p12"), "fixture")?;
    let [leaf] = explicit.certified.cert.as_slice() else { return Err("one certificate expected".into()) };
    let key_spki = explicit.certified.key.public_key().ok_or("no public key")?;
    let (_, parsed) = x509_parser::parse_x509_certificate(leaf)?;
    assert_ne!(parsed.public_key().raw, key_spki.as_ref(), "explicit EC parameters, not the named curve");
    let (cert, key) = (Certificate::from_der(leaf)?, PrivateKey::from_der(&explicit.key)?);
    assert_eq!(chain_for(key, None, vec![cert_bag(&cert, None, false)])?, [leaf.to_vec()], "the first bag");
    let compressed = include_bytes!("../tests/fixtures/ec.compressed.p12");
    assert_eq!(reason_for(compressed, "fixture"), Some(Reason::NoMatchingCertificate), "ring cannot verify it");
    Ok(())
}

#[test]
fn says_so_when_no_certificate_matches_the_key_or_ring_cannot_sign_with_it() -> TestResult {
    let (ec, rsa) = (pem_cert(EC_CERT)?, pem_cert(RSA_CERT)?);
    let certs = || vec![cert_bag(&ec, None, false), cert_bag(&rsa, Some(b"k"), false)];
    let pki = TestPki::new()?;
    let key = PrivateKey::from_der(pki.good().key_der())?;
    assert_eq!(chain_for(key, Some(b"k"), certs()), Err(Reason::NoMatchingCertificate));
    let p521 = PrivateKeyDer::from_pem_slice(include_bytes!("../tests/fixtures/p521.pkcs8.key"))?;
    assert_eq!(chain_for(PrivateKey::from_der(p521.secret_der())?, None, certs()), Err(Reason::KeyType));
    Ok(())
}

#[test]
fn leaves_out_the_root_trusted_or_not_and_unrelated_certificates() -> TestResult {
    let pki = TestPki::new()?;
    let (ca, leaf) = (Certificate::from_der(pki.ca_der())?, Certificate::from_der(pki.good().cert_der())?);
    let unrelated = pem_cert(RSA_CERT)?;
    let key = || PrivateKey::from_der(pki.good().key_der());
    let certs =
        |trusted| vec![cert_bag(&unrelated, None, false), cert_bag(&ca, None, trusted), cert_bag(&leaf, None, false)];
    assert_eq!(chain_for(key()?, None, certs(true))?, [leaf.as_der()]);
    assert_eq!(chain_for(key()?, None, certs(false))?, [leaf.as_der()]);
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
    let unrelated = pem_cert(RSA_CERT)?;
    let bags = vec![cert_bag(&b, None, false), cert_bag(&unrelated, None, false), cert_bag(&a, None, false)];
    let chain = chain_for(key, None, bags)?;
    assert_eq!(chain, [a.as_der(), b.as_der()]);
    Ok(())
}

#[test]
fn names_each_other_failure() {
    let layout = |mac, encrypted_safe, shrouded_key| Some(Layout { mac, encrypted_safe, shrouded_key });
    let (plain, plain_nomac) = (layout(true, false, false), layout(false, false, false));
    let unknown = None;
    let (safe, key) = (layout(true, true, false), layout(false, false, true));
    let cases = [
        (P12Error::UnpadError, plain, Reason::BagUndecryptable),
        (P12Error::Pkcs5Error("x".into()), plain_nomac, Reason::Undecryptable),
        (P12Error::UnsupportedMacAlgorithm, plain_nomac, Reason::Unsupported),
        (P12Error::UnsupportedEncryptionScheme, safe, Reason::Unsupported),
        (P12Error::InvalidParameters, key, Reason::OverLimit),
        (P12Error::InvalidPrivateKey, plain, Reason::NotPkcs8),
        (P12Error::InvalidPrivateKey, safe, Reason::BagUndecryptable),
        (P12Error::InvalidPrivateKey, key, Reason::Undecryptable),
        (P12Error::UnsupportedCertificateType, plain_nomac, Reason::NotX509),
        (P12Error::UnsupportedCertificateType, key, Reason::NotX509),
        (P12Error::UnsupportedCertificateType, safe, Reason::BagUndecryptable),
        (P12Error::InvalidData, plain_nomac, Reason::NotPkcs12),
        (P12Error::InvalidData, safe, Reason::BagUndecryptable),
        (P12Error::InvalidData, key, Reason::Undecryptable),
        (P12Error::InvalidVersion, key, Reason::NotPkcs12),
        (P12Error::UnpadError, unknown, Reason::Unreadable),
        (P12Error::UnsupportedCertificateType, unknown, Reason::Unreadable),
        (P12Error::InvalidPrivateKey, unknown, Reason::Unreadable),
        (P12Error::InvalidData, unknown, Reason::Unreadable),
        (P12Error::InvalidVersion, unknown, Reason::NotPkcs12),
        (P12Error::UnsupportedMacAlgorithm, unknown, Reason::Unsupported),
    ];
    for (error, layout, reason) in cases {
        assert_eq!(Reason::of(&error, layout), reason, "{error:?} in {layout:?}");
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
        (Reason::BagUndecryptable, "could not decrypt a bag: the bundle may use a separate encryption password"),
        (Reason::Unreadable, "could not read the bundle: check the passphrase, or export it again with AES"),
        (Reason::Unsupported, "the bundle uses a cipher or MAC that is not supported; export it again with AES"),
        (Reason::OverLimit, "unsupported or over-limit key-derivation parameters (iterations, scrypt cost or missing"),
        (Reason::NotPkcs8, "the bundle's private key is not valid PKCS#8"),
        (Reason::KeyType, "the bundle's private key is of a type that is not supported (use RSA, ECDSA P-256"),
        (Reason::NotX509, "the bundle holds a certificate that is not valid X.509"),
        (Reason::NotPkcs12, "the file is not a valid PKCS#12 bundle"),
        (Reason::NoKey, "the bundle has no private key"),
        (Reason::Keys(3), "the bundle holds 3 private keys; export only the one to send"),
        (Reason::NoCertificate, "the bundle has no certificate"),
        (Reason::NoMatchingCertificate, "no certificate in the bundle matches its private key"),
    ];
    for (reason, start) in starts {
        assert!(reason.to_string().starts_with(start), "{reason}");
    }
    for reason in [Reason::OverLimit, Reason::BagUndecryptable, Reason::Unreadable] {
        assert!(reason.to_string().ends_with("-macalg sha256`)"), "{reason}");
    }
}
