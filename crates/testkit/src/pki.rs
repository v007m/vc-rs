//! A test PKI for mTLS (D352): two CAs and the certificates tests need, generated in memory with
//! rcgen on ring, fresh for every test.

use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, PKCS_ECDSA_P256_SHA256, PKCS_ED25519, SignatureAlgorithm,
};

/// Why a certificate could not be made.
pub type PkiError = rcgen::Error;

/// The key algorithm of a certificate [`TestPki::client`] issues. Keys are PKCS#8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyKind {
    /// ECDSA on P-256 with SHA-256.
    EcP256,
    /// Ed25519.
    Ed25519,
}

impl KeyKind {
    const fn algorithm(self) -> &'static SignatureAlgorithm {
        match self {
            Self::EcP256 => &PKCS_ECDSA_P256_SHA256,
            Self::Ed25519 => &PKCS_ED25519,
        }
    }
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
        Ok(Self(CertifiedIssuer::self_signed(params, KeyPair::generate()?)?))
    }

    fn issue(&self, name: &str, params: &CertificateParams, key: KeyKind) -> Result<Identity, PkiError> {
        let key_pair = KeyPair::generate_for(key.algorithm())?;
        let cert = params.signed_by(&key_pair, &self.0)?;
        Ok(Identity {
            name: name.to_owned(),
            cert_der: cert.der().to_vec(),
            cert_pem: cert.pem(),
            key_der: key_pair.serialize_der(),
            key_pem: key_pair.serialize_pem(),
        })
    }

    fn client(&self, name: &str, key: KeyKind) -> Result<Identity, PkiError> {
        let mut params = CertificateParams::default();
        params.distinguished_name.push(DnType::CommonName, name);
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        self.issue(name, &params, key)
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
    /// When rcgen cannot generate a key or sign a certificate.
    pub fn new() -> Result<Self, PkiError> {
        let ca = Authority::new("vc test CA")?;
        let mut params = CertificateParams::new(["localhost".to_owned(), "127.0.0.1".to_owned()])?;
        params.distinguished_name.push(DnType::CommonName, "localhost");
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let server = ca.issue("localhost", &params, KeyKind::EcP256)?;
        let good = ca.client("good", KeyKind::EcP256)?;
        let other_ca = Authority::new("vc other CA")?.client("otherCa", KeyKind::EcP256)?;
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

    /// Issues another client certificate from the CA, with `name` as its common name.
    ///
    /// # Errors
    /// When rcgen cannot generate the key or sign the certificate.
    pub fn client(&self, name: &str, key: KeyKind) -> Result<Identity, PkiError> {
        self.ca.client(name, key)
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyKind, TestPki};

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
        let ed25519 = one.client("edge", KeyKind::Ed25519).unwrap();
        let ec = one.client("edge", KeyKind::EcP256).unwrap();
        assert_ne!(ed25519.key_der(), ec.key_der());
        assert!(ed25519.key_der().len() < ec.key_der().len(), "an Ed25519 PKCS#8 key is the shorter");
    }
}
