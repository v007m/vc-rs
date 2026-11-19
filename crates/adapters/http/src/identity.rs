use std::fmt;

use rustls::pki_types::pem::{self, PemObject};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::HttpAdapterError;

/// A client certificate chain and its private key, from PEM, and the hosts that get it.
///
/// The key is held in memory that is wiped when dropped. `Debug` shows the `id`, the hosts and
/// the SHA-256 fingerprint of the leaf certificate, never the key.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    id: String,
    hosts: Vec<String>,
    cert: Vec<u8>,
    key: Zeroizing<Vec<u8>>,
}

impl ClientIdentity {
    /// An identity from a PEM certificate chain (leaf first) and a PEM private key: RSA (PKCS#1
    /// or PKCS#8), ECDSA P-256 or P-384 (SEC1 or PKCS#8), or Ed25519 (PKCS#8).
    #[must_use]
    pub fn pem(id: impl Into<String>, cert: Vec<u8>, key: Vec<u8>) -> Self {
        Self { id: id.into(), hosts: Vec::new(), cert, key: Zeroizing::new(key) }
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

    /// Its host rules, as given.
    pub(crate) fn host_rules(&self) -> &[String] {
        &self.hosts
    }

    /// An error about this identity.
    pub(crate) fn error(&self, reason: impl Into<String>) -> HttpAdapterError {
        HttpAdapterError::Identity { id: self.id.clone(), reason: reason.into() }
    }

    /// The certificate chain and the private key, for rustls.
    ///
    /// Errors name the identity and the cause, never PEM text: pki-types quotes the bad line.
    pub(crate) fn parse(&self) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), HttpAdapterError> {
        let chain = CertificateDer::pem_slice_iter(&self.cert).collect::<Result<Vec<_>, _>>();
        let chain = chain.map_err(|error| self.error(format!("the certificate {}", pem_reason(&error))))?;
        if chain.is_empty() {
            return Err(self.error("the certificate has no CERTIFICATE block"));
        }
        if encrypted(&self.key) {
            return Err(self.error(ENCRYPTED));
        }
        let key = PrivateKeyDer::from_pem_slice(&self.key);
        Ok((chain, key.map_err(|error| self.error(format!("the private key {}", pem_reason(&error))))?))
    }
}

impl fmt::Debug for ClientIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let leaf = CertificateDer::pem_slice_iter(&self.cert).next().and_then(Result::ok);
        f.debug_struct("ClientIdentity")
            .field("id", &self.id)
            .field("hosts", &self.hosts)
            .field("leaf_sha256", &leaf.map(|leaf| Fingerprint(Sha256::digest(leaf).into())))
            .finish_non_exhaustive()
    }
}

/// A SHA-256 fingerprint, shown as lowercase hex.
struct Fingerprint([u8; 32]);

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
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
        match ClientIdentity::pem("tenantA", cert.to_vec(), key.to_vec()).parse() {
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
        let shown = format!("{identity:?}");
        assert!(shown.starts_with(r#"ClientIdentity { id: "ec", hosts: ["a.test"], leaf_sha256: Some("#), "{shown}");
        assert!(!shown.contains("KEY-BYTES"), "{shown}");
        let hex = shown.split("Some(").nth(1).and_then(|rest| rest.split(')').next()).unwrap_or_default();
        assert!(hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()), "{hex}");
    }
}
