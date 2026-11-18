//! A test PKI for mTLS (D352): two CAs and the certificates tests need, generated in memory with
//! rcgen on ring, fresh for every test.

use std::error::Error;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, PKCS_ECDSA_P256_SHA256, PKCS_ECDSA_P384_SHA384, PKCS_ED25519, SignatureAlgorithm,
};

/// Why a test certificate could not be made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PkiError {
    /// Generating a key or signing a certificate failed.
    #[error("cannot make a test certificate")]
    Certificate(#[source] Box<dyn Error + Send + Sync>),
    /// A validity date is before 1970 or after 9999.
    #[error("a test certificate's validity dates must fall between 1970 and 9999")]
    Validity,
}

fn certificate(error: rcgen::Error) -> PkiError {
    PkiError::Certificate(Box::new(error))
}

/// The key algorithm of a certificate [`TestPki::client`] issues. Keys are PKCS#8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyKind {
    /// ECDSA on P-256 with SHA-256.
    EcP256,
    /// ECDSA on P-384 with SHA-384.
    EcP384,
    /// Ed25519.
    Ed25519,
}

impl KeyKind {
    const fn algorithm(self) -> &'static SignatureAlgorithm {
        match self {
            Self::EcP256 => &PKCS_ECDSA_P256_SHA256,
            Self::EcP384 => &PKCS_ECDSA_P384_SHA384,
            Self::Ed25519 => &PKCS_ED25519,
        }
    }
}

/// What [`TestPki::client`] issues: a common name, a key kind (EC P-256 unless set) and a
/// validity window (rcgen's default, 1975 to 4096, unless set).
#[derive(Clone, Debug)]
pub struct ClientCert {
    name: String,
    key: KeyKind,
    validity: Option<(SystemTime, SystemTime)>,
}

impl ClientCert {
    /// A client certificate with `name` as its common name.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self { name: name.to_owned(), key: KeyKind::EcP256, validity: None }
    }

    /// Uses a `key` of that kind.
    #[must_use]
    pub const fn key(mut self, key: KeyKind) -> Self {
        self.key = key;
        self
    }

    /// Valid from `not_before` until `not_after`, for expired and not-yet-valid certificates.
    #[must_use]
    pub const fn valid(mut self, not_before: SystemTime, not_after: SystemTime) -> Self {
        self.validity = Some((not_before, not_after));
        self
    }

    fn params(&self) -> Result<CertificateParams, PkiError> {
        let mut params = CertificateParams::default();
        params.distinguished_name.push(DnType::CommonName, self.name.as_str());
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        if let Some((not_before, not_after)) = self.validity {
            let epoch = rcgen::date_time_ymd(1970, 1, 1);
            params.not_before = epoch + since_1970(not_before)?;
            params.not_after = epoch + since_1970(not_after)?;
        }
        Ok(params)
    }
}

/// The first second X.509 dates cannot hold: 10000-01-01.
const YEAR_10000: Duration = Duration::from_hours(70_389_528);

/// `at` as time since 1970, which rcgen's dates add without this crate naming their type.
fn since_1970(at: SystemTime) -> Result<Duration, PkiError> {
    at.duration_since(UNIX_EPOCH).ok().filter(|since| *since < YEAR_10000).ok_or(PkiError::Validity)
}

/// A certificate and its private key, as DER and as PEM. Its `name` is the subject's common name.
#[derive(Clone)]
pub struct Identity {
    name: String,
    cert_der: Vec<u8>,
    cert_pem: String,
    key_der: Vec<u8>,
    key_pem: String,
}

impl Identity {
    /// The subject's common name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The certificate, DER.
    #[must_use]
    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der
    }

    /// The certificate, PEM.
    #[must_use]
    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }

    /// The PKCS#8 private key, DER.
    #[must_use]
    pub fn key_der(&self) -> &[u8] {
        &self.key_der
    }

    /// The PKCS#8 private key, PEM (`PRIVATE KEY`).
    #[must_use]
    pub fn key_pem(&self) -> &str {
        &self.key_pem
    }
}

/// A CA that signs certificates.
struct Authority(CertifiedIssuer<'static, KeyPair>);

impl Authority {
    fn new(name: &str) -> Result<Self, PkiError> {
        let mut params = CertificateParams::default();
        params.distinguished_name.push(DnType::CommonName, name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let key_pair = KeyPair::generate().map_err(certificate)?;
        Ok(Self(CertifiedIssuer::self_signed(params, key_pair).map_err(certificate)?))
    }

    fn issue(&self, name: &str, params: &CertificateParams, key: KeyKind) -> Result<Identity, PkiError> {
        let key_pair = KeyPair::generate_for(key.algorithm()).map_err(certificate)?;
        let cert = params.signed_by(&key_pair, &self.0).map_err(certificate)?;
        Ok(Identity {
            name: name.to_owned(),
            cert_der: cert.der().to_vec(),
            cert_pem: cert.pem(),
            key_der: key_pair.serialize_der(),
            key_pem: key_pair.serialize_pem(),
        })
    }

    fn client(&self, spec: &ClientCert) -> Result<Identity, PkiError> {
        self.issue(&spec.name, &spec.params()?, spec.key)
    }
}

/// A CA, a server certificate for `localhost` and `127.0.0.1`, a `good` client certificate from
/// that CA, and an `otherCa` client certificate from a second CA the server does not trust.
pub struct TestPki {
    ca: Authority,
    server: Identity,
    good: Identity,
    other_ca: Identity,
}

impl TestPki {
    /// Generates a fresh PKI; every key is ECDSA P-256.
    ///
    /// # Errors
    /// [`PkiError::Certificate`] when a key cannot be generated or a certificate signed.
    pub fn new() -> Result<Self, PkiError> {
        let ca = Authority::new("vc test CA")?;
        let names = ["localhost".to_owned(), "127.0.0.1".to_owned()];
        let mut params = CertificateParams::new(names).map_err(certificate)?;
        params.distinguished_name.push(DnType::CommonName, "localhost");
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let server = ca.issue("localhost", &params, KeyKind::EcP256)?;
        let good = ca.client(&ClientCert::new("good"))?;
        let other_ca = Authority::new("vc other CA")?.client(&ClientCert::new("otherCa"))?;
        Ok(Self { ca, server, good, other_ca })
    }

    /// The CA certificate, DER: the root both the server and its clients trust.
    #[must_use]
    pub fn ca_der(&self) -> &[u8] {
        self.ca.0.der()
    }

    /// The CA certificate, PEM.
    #[must_use]
    pub fn ca_pem(&self) -> String {
        self.ca.0.pem()
    }

    /// The server certificate for `localhost` and `127.0.0.1`, issued by the CA.
    #[must_use]
    pub const fn server(&self) -> &Identity {
        &self.server
    }

    /// The `good` client certificate, issued by the CA.
    #[must_use]
    pub const fn good(&self) -> &Identity {
        &self.good
    }

    /// The `otherCa` client certificate, issued by a second CA the server does not trust.
    #[must_use]
    pub const fn other_ca(&self) -> &Identity {
        &self.other_ca
    }

    /// Issues another client certificate from the CA.
    ///
    /// # Errors
    /// [`PkiError::Validity`] for dates before 1970 or after 9999, [`PkiError::Certificate`] when
    /// the key cannot be generated or the certificate signed.
    pub fn client(&self, spec: &ClientCert) -> Result<Identity, PkiError> {
        self.ca.client(spec)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{ClientCert, KeyKind, PkiError, TestPki, YEAR_10000};

    #[test]
    fn issues_pem_and_der_for_each_identity() {
        let pki = TestPki::new().unwrap();
        assert!(pki.ca_pem().starts_with("-----BEGIN CERTIFICATE-----"));
        for identity in [pki.server(), pki.good(), pki.other_ca()] {
            assert!(identity.cert_pem().starts_with("-----BEGIN CERTIFICATE-----"), "{}", identity.name());
            assert!(identity.key_pem().starts_with("-----BEGIN PRIVATE KEY-----"), "{}", identity.name());
            assert_eq!(identity.cert_der()[0], 0x30, "a DER sequence");
        }
        assert_eq!((pki.good().name(), pki.other_ca().name()), ("good", "otherCa"));
    }

    #[test]
    fn each_pki_and_each_client_gets_its_own_keys() {
        let (one, two) = (TestPki::new().unwrap(), TestPki::new().unwrap());
        assert_ne!(one.ca_der(), two.ca_der());
        let issue = |key| one.client(&ClientCert::new("edge").key(key)).unwrap();
        let (ed25519, p256, p384) = (issue(KeyKind::Ed25519), issue(KeyKind::EcP256), issue(KeyKind::EcP384));
        assert_ne!(ed25519.key_der(), p256.key_der());
        assert!(ed25519.key_der().len() < p256.key_der().len(), "an Ed25519 PKCS#8 key is the shorter");
        assert!(p256.key_der().len() < p384.key_der().len(), "a P-384 key is longer than a P-256 one");
    }

    #[test]
    fn refuses_validity_dates_outside_1970_to_9999() {
        let pki = TestPki::new().unwrap();
        let early = ClientCert::new("early").valid(UNIX_EPOCH - Duration::from_secs(1), UNIX_EPOCH);
        let late = ClientCert::new("late").valid(UNIX_EPOCH, UNIX_EPOCH + YEAR_10000);
        assert!(matches!(pki.client(&early), Err(PkiError::Validity)));
        assert!(matches!(pki.client(&late), Err(PkiError::Validity)));
        let error = PkiError::Certificate(Box::new(rcgen::Error::KeyGenerationUnavailable));
        assert!(std::error::Error::source(&error).is_some(), "rcgen's error stays the source");
    }
}
