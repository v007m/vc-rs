//! PKCS#12 bundles (D352), decoded with `p12-keystore` into a DER chain and a PKCS#8 key.

use std::fmt;

use der::asn1::AnyRef;
use der::{Decode, Reader, SliceReader, Tag, Tagged};
use p12_keystore::error::Error as P12Error;
use p12_keystore::{CertificateBag, LocalKeyId, Pkcs12Archive};
use rustls::crypto::ring::sign::any_supported_type;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::ParsedCertificate;
use zeroize::Zeroizing;

/// A decoded bundle: the chain, leaf first, and the PKCS#8 key in memory wiped on drop.
pub struct Decoded {
    pub chain: Vec<CertificateDer<'static>>,
    pub key: Zeroizing<Vec<u8>>,
}

/// Why a bundle was refused. [`fmt::Display`] gives the message, which never quotes the bundle,
/// the passphrase or `p12-keystore`'s text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The passphrase has a character PKCS#12's `BMPString` cannot hold: U+FFFF or above.
    NonBmpPassphrase,
    /// The MAC does not match.
    WrongPassphrase,
    /// A bundle without a MAC did not decrypt or parse once decrypted.
    Undecryptable,
    /// A cipher, MAC or PBES2 scheme `p12-keystore` does not support.
    Unsupported,
    /// Key-derivation parameters over `p12-keystore`'s limits, or missing.
    OverLimit,
    /// The private key is not PKCS#8.
    NotPkcs8,
    /// A certificate is not X.509.
    NotX509,
    /// The file is not PKCS#12.
    NotPkcs12,
    /// The bundle has no private key.
    NoKey,
    /// The bundle has this many private keys, more than one.
    Keys(usize),
    /// The bundle has no certificate.
    NoCertificate,
}

const EXPORT_AGAIN: &str = "export it again with AES \
                            (`openssl pkcs12 -export -keypbe AES-256-CBC -certpbe AES-256-CBC -macalg sha256`)";

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonBmpPassphrase => f.write_str(
                "the passphrase has a character PKCS#12 cannot encode (one outside the Basic Multilingual \
                 Plane, such as an emoji, or U+FFFF)",
            ),
            Self::WrongPassphrase => {
                f.write_str("the passphrase is wrong, or the bundle is damaged (its MAC does not match)")
            }
            Self::Undecryptable => f.write_str("could not decrypt the bundle: check the passphrase"),
            Self::Unsupported => write!(f, "the bundle uses a cipher or MAC that is not supported; {EXPORT_AGAIN}"),
            Self::OverLimit => write!(
                f,
                "unsupported or over-limit key-derivation parameters (iterations, scrypt cost or missing \
                 parameters); {EXPORT_AGAIN}"
            ),
            Self::NotPkcs8 => f.write_str("the bundle's private key is not valid PKCS#8"),
            Self::NotX509 => f.write_str("the bundle holds a certificate that is not valid X.509"),
            Self::NotPkcs12 => f.write_str("the file is not a valid PKCS#12 bundle"),
            Self::NoKey => f.write_str("the bundle has no private key"),
            Self::Keys(count) => write!(f, "the bundle holds {count} private keys; export only the one to send"),
            Self::NoCertificate => f.write_str("the bundle has no certificate"),
        }
    }
}

impl std::error::Error for Reason {}

impl Reason {
    /// The reason for `p12-keystore`'s error. Without a MAC a wrong passphrase only shows as
    /// decrypted bytes that do not unpad or parse, so those failures say to check it.
    const fn of(error: &P12Error, has_mac: bool) -> Self {
        match error {
            P12Error::MacError(_) => Self::WrongPassphrase,
            P12Error::InvalidParameters => Self::OverLimit,
            P12Error::Pkcs5Error(_)
            | P12Error::UnpadError
            | P12Error::DerError(_)
            | P12Error::InvalidData
            | P12Error::InvalidPrivateKey
            | P12Error::X509Error(_)
            | P12Error::UnsupportedCertificateType
                if !has_mac =>
            {
                Self::Undecryptable
            }
            P12Error::UnsupportedEncryptionScheme
            | P12Error::UnsupportedMacAlgorithm
            | P12Error::Pkcs5Error(_)
            | P12Error::UnpadError => Self::Unsupported,
            P12Error::InvalidPrivateKey => Self::NotPkcs8,
            P12Error::X509Error(_) | P12Error::UnsupportedCertificateType => Self::NotX509,
            _ => Self::NotPkcs12,
        }
    }
}

/// Decodes a bundle that holds one private key and its certificate.
pub fn decode(bytes: &[u8], password: &str) -> Result<Decoded, Reason> {
    if password.chars().any(|c| u32::from(c) >= 0xFFFF) {
        return Err(Reason::NonBmpPassphrase);
    }
    let archive = Pkcs12Archive::from_pkcs12(bytes, password).map_err(|error| Reason::of(&error, has_mac(bytes)))?;
    from_archive(&archive)
}

/// Whether the outer `PFX` sequence has a third field, the MAC (RFC 7292 §4). A file that is
/// not such a sequence counts as having one, so its failure stays "not PKCS#12".
fn has_mac(bytes: &[u8]) -> bool {
    let fields = || -> der::Result<bool> {
        let outer = AnyRef::from_der(bytes)?;
        let mut reader = SliceReader::new(outer.value())?;
        let _version: AnyRef<'_> = reader.decode()?;
        let _auth_safe: AnyRef<'_> = reader.decode()?;
        Ok(outer.tag() != Tag::Sequence || !reader.is_finished())
    };
    fields().unwrap_or(true)
}

/// The key and its chain from a decrypted bundle.
fn from_archive(archive: &Pkcs12Archive) -> Result<Decoded, Reason> {
    let [bag] = archive.keys.as_slice() else {
        return Err(if archive.keys.is_empty() { Reason::NoKey } else { Reason::Keys(archive.keys.len()) });
    };
    let key = Zeroizing::new(bag.key.as_der().to_vec());
    let leaf = leaf(&archive.certs, &key, bag.local_key_id.as_ref()).ok_or(Reason::NoCertificate)?;
    Ok(Decoded { chain: chain(&archive.certs, leaf), key })
}

/// The key's certificate: one whose public key is the key's, the one with the key's local key ID
/// if several are; when none is, the one with the key's local key ID, else the first.
fn leaf<'a>(certs: &'a [CertificateBag], key: &[u8], key_id: Option<&LocalKeyId>) -> Option<&'a CertificateBag> {
    let owns = |bag: &&CertificateBag| key_id.is_some_and(|id| bag.local_key_id.as_deref() == Some(id.as_ref()));
    let public = public_key(key);
    let matching: Vec<&CertificateBag> =
        certs.iter().filter(|bag| public.is_some() && public_key_of(bag) == public).collect();
    matching
        .iter()
        .copied()
        .find(owns)
        .or_else(|| matching.first().copied())
        .or_else(|| certs.iter().find(owns))
        .or_else(|| certs.first())
}

/// The PKCS#8 key's `SubjectPublicKeyInfo`, as rustls's `keys_match` compares it.
fn public_key(key: &[u8]) -> Option<Vec<u8>> {
    let signer = any_supported_type(&PrivateKeyDer::Pkcs8(key.into())).ok()?;
    signer.public_key().map(|spki| spki.to_vec())
}

/// A certificate's `SubjectPublicKeyInfo`.
fn public_key_of(bag: &CertificateBag) -> Option<Vec<u8>> {
    let der = CertificateDer::from(bag.cert.as_der());
    ParsedCertificate::try_from(&der).ok().map(|cert| cert.subject_public_key_info().to_vec())
}

/// The leaf, then its issuer, that one's issuer and so on, matched by subject name up to a
/// self-signed certificate. Bags marked trusted (trust anchors, not ours to send) and
/// certificates outside the chain are left out.
fn chain(certs: &[CertificateBag], leaf: &CertificateBag) -> Vec<CertificateDer<'static>> {
    let mut chain = vec![leaf];
    let mut current = leaf;
    while current.cert.issuer() != current.cert.subject() {
        let issuer = certs.iter().find(|bag| {
            !bag.trusted
                && bag.cert.subject() == current.cert.issuer()
                && !chain.iter().any(|in_chain| in_chain.cert == bag.cert)
        });
        let Some(issuer) = issuer else { break };
        chain.push(issuer);
        current = issuer;
    }
    chain.into_iter().map(|bag| CertificateDer::from(bag.cert.as_der().to_vec())).collect()
}

#[cfg(test)]
#[path = "pkcs12_tests.rs"]
mod tests;
