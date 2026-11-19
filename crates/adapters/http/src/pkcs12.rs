//! PKCS#12 bundles (D352), decoded with `p12-keystore` into a DER chain and a signer for the key.

use p12_keystore::{CertificateBag, LocalKeyId, Pkcs12Archive};
use rustls::crypto::ring::sign::any_supported_type;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::sign::CertifiedKey;
use x509_parser::certificate::X509Certificate;
use zeroize::Zeroizing;

use crate::pkcs12_reason::Layout;
pub use crate::pkcs12_reason::Reason;

/// A decoded bundle: the chain, leaf first, with the signer ring built from the key, and vc's
/// copy of the PKCS#8 key, in memory wiped on drop.
pub struct Decoded {
    pub certified: CertifiedKey,
    pub key: Zeroizing<Vec<u8>>,
}

/// Decodes a bundle that holds one private key and its certificate.
pub fn decode(bytes: &[u8], password: &str) -> Result<Decoded, Reason> {
    if password.chars().any(|c| u32::from(c) >= 0xFFFF) {
        return Err(Reason::NonBmpPassphrase);
    }
    let archive = Pkcs12Archive::from_pkcs12(bytes, password).map_err(|error| Reason::of(&error, Layout::of(bytes)))?;
    from_archive(&archive)
}

/// The key, parsed once into a signer, and its chain from a decrypted bundle.
fn from_archive(archive: &Pkcs12Archive) -> Result<Decoded, Reason> {
    let [bag] = archive.keys.as_slice() else {
        return Err(if archive.keys.is_empty() { Reason::NoKey } else { Reason::Keys(archive.keys.len()) });
    };
    if archive.certs.is_empty() {
        return Err(Reason::NoCertificate);
    }
    let key = Zeroizing::new(bag.key.as_der().to_vec());
    let signer = any_supported_type(&PrivateKeyDer::Pkcs8(key.as_slice().into())).map_err(|_| Reason::KeyType)?;
    let certs: Vec<Cert<'_>> = archive.certs.iter().filter_map(Cert::parse).collect();
    let public = signer.public_key();
    let leaf = leaf(&certs, public.as_deref(), bag.local_key_id.as_ref()).ok_or(Reason::NoMatchingCertificate)?;
    Ok(Decoded { certified: CertifiedKey::new(chain(&certs, leaf), signer), key })
}

/// A bundle's certificate, parsed once with x509-parser, which also reads v1 certificates.
struct Cert<'a> {
    bag: &'a CertificateBag,
    x509: X509Certificate<'a>,
}

impl<'a> Cert<'a> {
    fn parse(bag: &'a CertificateBag) -> Option<Self> {
        let (_, x509) = x509_parser::parse_x509_certificate(bag.cert.as_der()).ok()?;
        Some(Self { bag, x509 })
    }

    /// The DER of the subject name.
    fn subject(&self) -> &[u8] {
        self.x509.subject().as_raw()
    }

    /// The DER of the issuer name.
    fn issuer(&self) -> &[u8] {
        self.x509.issuer().as_raw()
    }

    /// The DER of the `SubjectPublicKeyInfo`.
    fn public_key(&self) -> &[u8] {
        self.x509.public_key().raw
    }

    /// How well this certificate fits as `child`'s issuer: whether its key verifies `child`'s
    /// signature, then how late it expires.
    fn fit(&self, child: &Cert<'_>) -> (bool, i64) {
        let signs = child.x509.verify_signature(Some(self.x509.public_key())).is_ok();
        (signs, self.x509.validity().not_after.timestamp())
    }
}

/// The key's certificate: one whose public key is the key's, the one with the key's local key ID
/// if several are, else the first.
fn leaf<'c, 'a>(certs: &'c [Cert<'a>], public: Option<&[u8]>, key_id: Option<&LocalKeyId>) -> Option<&'c Cert<'a>> {
    let owns = |cert: &&Cert<'_>| key_id.is_some_and(|id| cert.bag.local_key_id.as_deref() == Some(id.as_ref()));
    let matching = || certs.iter().filter(|cert| Some(cert.public_key()) == public);
    matching().find(owns).or_else(|| matching().next())
}

/// The leaf, then its issuer, that one's issuer and so on, up to a self-issued certificate. An
/// issuer's subject has the same DER as its child's issuer name; trusted bags count, since a
/// keystore may store a real intermediate as one. Among several such, the one whose key verifies
/// the child's signature wins, then the one that expires last, then the first. Certificates off
/// this path are left out.
fn chain<'c, 'a>(certs: &'c [Cert<'a>], leaf: &'c Cert<'a>) -> Vec<CertificateDer<'static>> {
    let mut chain = vec![leaf];
    let mut current = leaf;
    while current.issuer() != current.subject() {
        let issuer = certs
            .iter()
            .rev()
            .filter(|cert| {
                cert.subject() == current.issuer() && !chain.iter().any(|link| link.bag.cert == cert.bag.cert)
            })
            .max_by_key(|cert| cert.fit(current));
        let Some(issuer) = issuer else { break };
        chain.push(issuer);
        current = issuer;
    }
    chain.into_iter().map(|cert| CertificateDer::from(cert.bag.cert.as_der().to_vec())).collect()
}

#[cfg(test)]
#[path = "pkcs12_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pkcs12_chain_tests.rs"]
mod chain_tests;
