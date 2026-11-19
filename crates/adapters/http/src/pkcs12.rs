//! PKCS#12 bundles (D352), decoded with `p12-keystore` into a DER chain and a signer for the key.

use p12_keystore::{CertificateBag, LocalKeyId, Pkcs12Archive};
use rustls::crypto::ring::default_provider;
use rustls::crypto::ring::sign::any_supported_type;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::sign::{CertifiedKey, SigningKey};
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
    let certs = archive.certs.iter().map(Cert::parse).collect::<Option<Vec<_>>>().ok_or(Reason::NotX509)?;
    let leaf = leaf(&certs, signer.as_ref(), bag.local_key_id.as_ref()).ok_or(Reason::NoMatchingCertificate)?;
    Ok(Decoded { certified: CertifiedKey::new(chain(&certs, leaf), signer), key })
}

/// A bundle's certificate, parsed once with x509-parser, which also reads v1 certificates. A
/// certificate it cannot parse refuses the bundle, as `p12-keystore`, which parses with it too,
/// already does.
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

    /// Whether the certificate names itself as its issuer, as a root does.
    fn self_issued(&self) -> bool {
        self.issuer() == self.subject()
    }

    /// Whether `chain` already holds this certificate.
    fn is_in(&self, chain: &[&Self]) -> bool {
        chain.iter().any(|link| link.bag.cert == self.bag.cert)
    }

    /// How well this certificate fits as `child`'s issuer: whether its key verifies `child`'s
    /// signature, then how late it expires.
    fn fit(&self, child: &Cert<'_>) -> (bool, i64) {
        let signs = child.x509.verify_signature(Some(self.x509.public_key())).is_ok();
        (signs, self.x509.validity().not_after.timestamp())
    }
}

/// The key's certificate, checked to hold the key: one whose `SubjectPublicKeyInfo` has the same
/// DER as the key's (the one with the key's local key ID if several do, else the first), else the
/// one with the key's local key ID, else the first.
fn leaf<'c, 'a>(certs: &'c [Cert<'a>], signer: &dyn SigningKey, key_id: Option<&LocalKeyId>) -> Option<&'c Cert<'a>> {
    let public = signer.public_key();
    let tagged = |cert: &&Cert<'_>| key_id.is_some_and(|id| cert.bag.local_key_id.as_deref() == Some(id.as_ref()));
    let same = || certs.iter().filter(|cert| Some(cert.public_key()) == public.as_deref());
    let leaf = same().find(tagged).or_else(|| same().next());
    let leaf = leaf.or_else(|| certs.iter().find(tagged)).or_else(|| certs.first())?;
    holds(leaf, signer, public.as_deref()).then_some(leaf)
}

/// Whether `cert` holds `signer`'s key, whose `SubjectPublicKeyInfo` is `public`: it has the same
/// DER, or, encoded another way (explicit EC parameters, say), its key verifies a signature
/// `signer` makes. ring cannot verify with a compressed EC point, so a certificate with one never
/// holds the key unless its DER is the key's.
fn holds(cert: &Cert<'_>, signer: &dyn SigningKey, public: Option<&[u8]>) -> bool {
    if public == Some(cert.public_key()) {
        return true;
    }
    let algorithms = default_provider().signature_verification_algorithms;
    let Some(probe) = signer.choose_scheme(&algorithms.supported_schemes()) else { return false };
    let Ok(signature) = probe.sign(PROBE) else { return false };
    let key = &cert.x509.public_key().subject_public_key.data;
    let verifiers = algorithms.mapping.iter().filter(|(scheme, _)| *scheme == probe.scheme());
    verifiers
        .flat_map(|(_, verifiers)| verifiers.iter())
        .any(|verifier| verifier.verify_signature(key, PROBE, &signature).is_ok())
}

/// What [`holds`] signs.
const PROBE: &[u8] = b"vc: does this certificate hold the PKCS#12 bundle's key";

/// The leaf, then its issuer, that one's issuer and so on, stopping before a self-issued
/// certificate: the server holds its roots, so none is sent. An issuer's subject has the same DER
/// as its child's issuer name; trusted bags count, since a keystore may store a real intermediate
/// as one. Among several such, the one whose key verifies the child's signature wins, then the one
/// that expires last, then the first. Certificates off this path are left out.
fn chain<'c, 'a>(certs: &'c [Cert<'a>], leaf: &'c Cert<'a>) -> Vec<CertificateDer<'static>> {
    let mut chain = vec![leaf];
    while let Some(issuer) = issuer(certs, &chain) {
        chain.push(issuer);
    }
    chain.into_iter().map(|cert| CertificateDer::from(cert.bag.cert.as_der().to_vec())).collect()
}

/// The issuer of the chain's last certificate: one in `certs`, not yet in the chain and not
/// self-issued. Signatures are checked only when several certificates qualify.
fn issuer<'c, 'a>(certs: &'c [Cert<'a>], chain: &[&'c Cert<'a>]) -> Option<&'c Cert<'a>> {
    let child = chain.last().filter(|child| !child.self_issued())?;
    let named = certs.iter().rev().filter(|cert| cert.subject() == child.issuer());
    let mut candidates: Vec<&Cert<'a>> = named.filter(|cert| !cert.self_issued() && !cert.is_in(chain)).collect();
    if candidates.len() < 2 {
        return candidates.pop();
    }
    candidates.into_iter().max_by_key(|cert| cert.fit(child))
}

#[cfg(test)]
#[path = "pkcs12_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pkcs12_chain_tests.rs"]
mod chain_tests;
