use std::fmt;

use rustls::crypto::ring::sign::any_supported_type;
use rustls::pki_types::pem::{self, PemObject};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::sign::CertifiedKey;
use rustls::{Error as TlsError, InconsistentKeys};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::HttpAdapterError;
use crate::pkcs12::{self, Decoded};

/// A client certificate chain and its private key, from PEM or PKCS#12, and the hosts that get it.
///
/// The key is held in memory that is wiped when dropped. `Debug` shows the `id`, the hosts and
/// the SHA-256 fingerprint of the leaf certificate, never the key.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    id: String,
    hosts: Vec<String>,
    leaf_sha256: Option<Fingerprint>,
    material: Material,
}

/// Where the chain and key come from.
#[derive(Clone)]
enum Material {
    /// PEM text, parsed when the transport is built.
    Pem { cert: Vec<u8>, key: Zeroizing<Vec<u8>> },
    /// A decoded PKCS#12 bundle, or a PEM identity once parsed: the DER chain, leaf first, with
    /// the signer built from the key, and the key's DER.
    Der { certified: CertifiedKey, key: Zeroizing<Vec<u8>> },
}

impl PartialEq for Material {
    /// Same text, or the same chain and key: the signer is built from the key.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Pem { cert, key }, Self::Pem { cert: other_cert, key: other_key }) => {
                (cert, key) == (other_cert, other_key)
            }
            (Self::Der { certified, key }, Self::Der { certified: other, key: other_key }) => {
                (&certified.cert, key) == (&other.cert, other_key)
            }
            _ => false,
        }
    }
}

impl Eq for Material {}

impl ClientIdentity {
    /// An identity from a PEM certificate chain (leaf first) and a PEM private key: RSA (PKCS#1
    /// or PKCS#8), ECDSA P-256 or P-384 (SEC1 or PKCS#8), or Ed25519 (PKCS#8).
    #[must_use]
    pub fn pem(id: impl Into<String>, cert: Vec<u8>, key: Vec<u8>) -> Self {
        let leaf = CertificateDer::pem_slice_iter(&cert).next().and_then(Result::ok);
        let leaf_sha256 = leaf.as_deref().map(Fingerprint::of);
        Self {
            id: id.into(),
            hosts: Vec::new(),
            leaf_sha256,
            material: Material::Pem { cert, key: Zeroizing::new(key) },
        }
    }

    /// An identity from a PKCS#12 bundle (`.p12`, `.pfx`) and its passphrase, encrypted with PBES2
    /// and AES or with the legacy 3DES and RC2 ciphers. It must hold one private key and its
    /// certificate. The leaf is the certificate whose public key has the same DER as the key's,
    /// else the one with the key's local key ID, else the first, and it must verify a signature
    /// the key makes. Its issuers from the bundle follow, matched by the DER of their names,
    /// trusted entries included; when several share a name, the one whose key verifies the
    /// signature wins, then the one that expires last. Self-issued roots and certificates off that
    /// path are left out.
    ///
    /// The bundle is decoded here, once: ring parses the key into the signer the transport uses,
    /// and vc's copy of the key is kept in memory wiped on drop. Decoding runs the bundle's key
    /// derivation for the MAC and again for each encrypted bag, each up to `p12-keystore`'s limit
    /// of 1,000,000 iterations, and scrypt may use up to 1 GiB of memory, so call it off the async
    /// runtime (`spawn_blocking`).
    ///
    /// # Errors
    ///
    /// [`HttpAdapterError::Mismatch`] for a wrong passphrase (the MAC does not match, or a bundle
    /// without one does not decrypt), a bundle with no key or more than one, or no certificate
    /// that matches the key. [`HttpAdapterError::Identity`] for a passphrase PKCS#12 cannot
    /// encode, a bag that does not decrypt, a bundle that is not DER and fails to decrypt or
    /// parse, an unsupported cipher or MAC, key-derivation parameters over the limits, a key of an
    /// unsupported type, a certificate that is not X.509, no certificate, or a file that is not
    /// PKCS#12.
    pub fn pkcs12(id: impl Into<String>, bundle: &[u8], passphrase: &str) -> Result<Self, HttpAdapterError> {
        let id = id.into();
        match pkcs12::decode(bundle, passphrase) {
            Ok(Decoded { certified, key }) => {
                let leaf_sha256 = certified.cert.first().map(|leaf| Fingerprint::of(leaf));
                Ok(Self { id, hosts: Vec::new(), leaf_sha256, material: Material::Der { certified, key } })
            }
            Err(reason) if reason.mismatched() => Err(HttpAdapterError::Mismatch { id, reason: reason.to_string() }),
            Err(reason) => Err(HttpAdapterError::Identity { id, reason: reason.to_string() }),
        }
    }

    /// Sends this identity on requests to these hosts: `host`, `host:port` or `*.domain`, where
    /// `*` covers exactly one leftmost label. Without hosts it is used only when named.
    #[must_use]
    pub fn hosts(mut self, hosts: Vec<String>) -> Self {
        self.hosts = hosts;
        self
    }

    /// Its ID, as errors and reports name it.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Its leaf certificate, once decoded or [parsed](Self::parsed).
    pub(crate) fn leaf(&self) -> Option<&CertificateDer<'static>> {
        match &self.material {
            Material::Der { certified, .. } => certified.cert.first(),
            Material::Pem { .. } => None,
        }
    }

    /// Its host rules, as given.
    pub(crate) fn host_rules(&self) -> &[String] {
        &self.hosts
    }

    /// An error about this identity.
    pub(crate) fn error(&self, reason: impl Into<String>) -> HttpAdapterError {
        HttpAdapterError::Identity { id: self.id.clone(), reason: reason.into() }
    }

    /// This identity with its PEM chain and key parsed once into the signer the transport uses,
    /// checked to belong together, so building the transport parses nothing again (D399); a
    /// PKCS#12 identity, decoded already, as it is.
    ///
    /// # Errors
    ///
    /// As [`ClientIdentity::certified`].
    pub(crate) fn parsed(self) -> Result<Self, HttpAdapterError> {
        let Material::Pem { cert, key } = &self.material else { return Ok(self) };
        let (chain, der) = self.parse_pem(cert, key)?;
        let certified = self.sign(chain, &der)?;
        let key = Zeroizing::new(der.secret_der().to_vec());
        Ok(Self { material: Material::Der { certified, key }, ..self })
    }

    /// The certificate chain and its signing key, for rustls, checked to belong together.
    ///
    /// A PEM key is decoded into memory that is wiped on drop, and ring parses it by reference;
    /// a PKCS#12 identity reuses the signer it built when decoded.
    /// Errors name the identity and the cause, never PEM text: pki-types quotes the bad line.
    pub(crate) fn certified(&self) -> Result<CertifiedKey, HttpAdapterError> {
        match &self.material {
            Material::Pem { cert, key } => {
                let (chain, key) = self.parse_pem(cert, key)?;
                self.sign(chain, &key)
            }
            Material::Der { certified, .. } => Ok(certified.clone()),
        }
    }

    /// The PEM chain and key, the key decoded into memory wiped on drop.
    fn parse_pem(&self, cert: &[u8], key: &[u8]) -> Result<PemParts, HttpAdapterError> {
        let chain = pem_chain(cert).map_err(|reason| self.error(reason))?;
        Ok((chain, pem_key(key).map_err(|reason| self.error(reason))?))
    }

    /// The chain with a signer made from the key, checked to belong together.
    fn sign(
        &self,
        chain: Vec<CertificateDer<'static>>,
        key: &PrivateKeyDer<'_>,
    ) -> Result<CertifiedKey, HttpAdapterError> {
        let certified =
            CertifiedKey::new(chain, any_supported_type(key).map_err(|error| self.error(error.to_string()))?);
        match certified.keys_match() {
            Ok(()) | Err(TlsError::InconsistentKeys(InconsistentKeys::Unknown)) => Ok(certified),
            Err(error) => Err(HttpAdapterError::Mismatch { id: self.id.clone(), reason: error.to_string() }),
        }
    }
}

/// The PEM certificate chain in `cert`, leaf first, or what is wrong with it: not PEM, or no
/// `CERTIFICATE` block.
pub fn pem_chain(cert: &[u8]) -> Result<Vec<CertificateDer<'static>>, String> {
    let chain = CertificateDer::pem_slice_iter(cert).collect::<Result<Vec<_>, _>>();
    let chain = chain.map_err(|error| format!("the certificate {}", pem_reason(&error)))?;
    if chain.is_empty() {
        return Err("the certificate has no CERTIFICATE block".to_owned());
    }
    Ok(chain)
}

/// A PEM identity's chain and decoded key.
type PemParts = (Vec<CertificateDer<'static>>, Zeroizing<PrivateKeyDer<'static>>);

impl fmt::Debug for ClientIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientIdentity")
            .field("id", &self.id)
            .field("hosts", &self.hosts)
            .field("leaf_sha256", &self.leaf_sha256)
            .finish_non_exhaustive()
    }
}

/// A SHA-256 fingerprint, shown as lowercase hex.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Fingerprint([u8; 32]);

impl Fingerprint {
    fn of(der: &[u8]) -> Self {
        Self(Sha256::digest(der).into())
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

/// The PEM private key in `key`, decoded into memory wiped on drop, or what is wrong with it:
/// encrypted, or not PEM.
fn pem_key(key: &[u8]) -> Result<Zeroizing<PrivateKeyDer<'static>>, String> {
    if encrypted(key) {
        return Err(ENCRYPTED.to_owned());
    }
    let key = PrivateKeyDer::from_pem_slice(key).map(Zeroizing::new);
    key.map_err(|error| format!("the private key {}", pem_reason(&error)))
}

/// What is wrong with a PEM private key given without its certificate: encrypted, not PEM, or
/// of a type the signer does not support (D430).
pub fn lone_key(key: &[u8]) -> Result<(), String> {
    any_supported_type(&*pem_key(key)?).map(drop).map_err(|error| error.to_string())
}

const ENCRYPTED: &str = "the private key is encrypted; decrypt it (`openssl pkey -in key.pem -out plain.pem`) \
                         or use a PKCS#12 bundle with a passphrase";

/// `true` for a PKCS#8 `ENCRYPTED PRIVATE KEY` block or a legacy OpenSSL `Proc-Type` encrypted key.
fn encrypted(key: &[u8]) -> bool {
    let has = |needle: &[u8]| key.windows(needle.len()).any(|window| window == needle);
    has(b"-----BEGIN ENCRYPTED PRIVATE KEY-----") || has(b"Proc-Type: 4,ENCRYPTED")
}

/// What is wrong with a PEM text, without quoting it.
const fn pem_reason(error: &pem::Error) -> &'static str {
    match error {
        pem::Error::NoItemsFound => "has no PEM block of the expected kind",
        pem::Error::MissingSectionEnd { .. } => "has a PEM block with no END line",
        pem::Error::Base64Decode(_) => "has a PEM block that is not valid base64",
        _ => "is not valid PEM",
    }
}

#[cfg(test)]
mod tests {
    use rustls::pki_types::pem;

    use super::{ClientIdentity, encrypted, pem_reason};
    use crate::HttpAdapterError;

    fn reason(cert: &[u8], key: &[u8]) -> String {
        match ClientIdentity::pem("tenantA", cert.to_vec(), key.to_vec()).certified() {
            Err(HttpAdapterError::Identity { id, reason }) if id == "tenantA" => reason,
            other => format!("unexpected {other:?}"),
        }
    }

    #[test]
    fn names_what_is_wrong_without_quoting_the_text() {
        let cert = include_bytes!("../tests/fixtures/ec.pem");
        assert_eq!(reason(b"", b""), "the certificate has no CERTIFICATE block");
        assert_eq!(
            reason(b"-----BEGIN CERTIFICATE-----\nAAAA\n", b""),
            "the certificate has a PEM block with no END line"
        );
        assert_eq!(reason(cert, b"secret"), "the private key has no PEM block of the expected kind");
        let bad = b"-----BEGIN PRIVATE KEY-----\nsecret!\n-----END PRIVATE KEY-----\n";
        assert_eq!(reason(cert, bad), "the private key has a PEM block that is not valid base64");
    }

    #[test]
    fn spots_both_encrypted_key_forms() {
        assert!(encrypted(b"-----BEGIN ENCRYPTED PRIVATE KEY-----\n"));
        assert!(encrypted(b"-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\n"));
        assert!(!encrypted(b"-----BEGIN PRIVATE KEY-----\nENCRYPTED\n"));
    }

    #[test]
    fn other_pem_errors_are_not_valid_pem() {
        assert_eq!(pem_reason(&pem::Error::SectionTooLarge), "is not valid PEM");
    }

    #[test]
    fn debug_shows_the_fingerprint_but_not_the_key() {
        let cert = include_bytes!("../tests/fixtures/ec.pem").to_vec();
        let identity = ClientIdentity::pem("ec", cert, b"KEY-BYTES".to_vec()).hosts(vec!["a.test".into()]);
        assert_eq!(identity.id(), "ec");
        let shown = format!("{identity:?}");
        assert!(shown.starts_with(r#"ClientIdentity { id: "ec", hosts: ["a.test"], leaf_sha256: Some("#), "{shown}");
        assert!(!shown.contains("KEY-BYTES"), "{shown}");
        let hex = shown.split("Some(").nth(1).and_then(|rest| rest.split(')').next()).unwrap_or_default();
        assert!(hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()), "{hex}");
    }

    #[test]
    fn identities_are_equal_when_their_chain_and_key_are() -> Result<(), HttpAdapterError> {
        let p12 = |bundle: &[u8]| ClientIdentity::pkcs12("ec", bundle, "fixture");
        let aes = p12(include_bytes!("../tests/fixtures/ec.p12"))?;
        assert_eq!(aes, p12(include_bytes!("../tests/fixtures/ec.legacy.p12"))?, "the cipher does not count");
        let cert = include_bytes!("../tests/fixtures/ec.pem");
        let pem = |key: &[u8]| ClientIdentity::pem("ec", cert.to_vec(), key.to_vec());
        let sec1 = pem(include_bytes!("../tests/fixtures/ec.sec1.key"));
        assert_ne!(aes, sec1);
        assert_eq!(sec1, pem(include_bytes!("../tests/fixtures/ec.sec1.key")));
        assert_ne!(sec1, pem(b"other"));
        Ok(())
    }

    #[test]
    fn a_parsed_pem_identity_keeps_its_chain_and_signer_and_a_bad_one_is_refused() -> Result<(), HttpAdapterError> {
        let cert = include_bytes!("../tests/fixtures/ec.pem").to_vec();
        let pem = ClientIdentity::pem("ec", cert.clone(), include_bytes!("../tests/fixtures/ec.sec1.key").to_vec());
        let parsed = pem.clone().parsed()?;
        assert!(matches!(parsed.material, super::Material::Der { .. }), "parsed into the signer");
        assert_eq!(format!("{parsed:?}"), format!("{pem:?}"));
        assert_eq!(parsed.certified()?.cert, pem.certified()?.cert);
        let bundle = ClientIdentity::pkcs12("ec", include_bytes!("../tests/fixtures/ec.p12"), "fixture")?;
        assert_eq!(bundle.clone().parsed()?, bundle, "a bundle is parsed already");
        let other = ClientIdentity::pem("ec", cert, include_bytes!("../tests/fixtures/rsa.pkcs8.key").to_vec());
        assert!(matches!(other.parsed(), Err(HttpAdapterError::Mismatch { .. })));
        Ok(())
    }

    #[test]
    fn a_pkcs12_identity_shows_its_leaf_and_signs_with_its_key() -> Result<(), HttpAdapterError> {
        let bundle = ClientIdentity::pkcs12("ec", include_bytes!("../tests/fixtures/ec.p12"), "fixture")?;
        let pem = include_bytes!("../tests/fixtures/ec.pem").to_vec();
        let pem = ClientIdentity::pem("ec", pem, include_bytes!("../tests/fixtures/ec.sec1.key").to_vec());
        assert_eq!(format!("{bundle:?}"), format!("{pem:?}"));
        assert_eq!(bundle.certified()?.cert, pem.certified()?.cert);
        Ok(())
    }
}
